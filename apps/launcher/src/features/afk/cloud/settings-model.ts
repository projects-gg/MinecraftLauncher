// The cloud settings sheet's draft: the account's name, server, version and console language, and
// its commands and auto-replies (CloudEditConfig, secret rows with this computer's words where it
// has them). The backend checks everything again (pl_cloud_afk::validate) and the service has the
// last word; these checks only answer while typing.
import type { CloudEditConfig, CloudIssue, CloudRowText } from "@/bindings";
import type { CloudEdit, CloudEditData } from "./api";
import { looksSensitive, withoutLeadingIgnorable } from "./sensitive";

export type Issue = CloudIssue;
export type Command = CloudEditConfig["commands"][number];
export type Rule = CloudEditConfig["rules"][number];
export type Words = CloudRowText;

export interface Draft {
  name: string;
  host: string;
  /** As typed ("" is 0: the client finds it). */
  port: string;
  version: string;
  lang: string;
  config: CloudEditConfig;
}

export const TRIGGERS = ["join", "start", "reconnect", "interval", "daily"] as const;
export type Trigger = (typeof TRIGGERS)[number];
export const SOURCES = ["any", "player", "private", "server"] as const;

const NAME = /^[A-Za-z0-9_]{3,16}$/;
const CLOCK = /^([01]\d|2[0-3]):[0-5]\d$/;
const VARIABLES = new Set(["username", "serverip", "serverport", "datetime"]);
const MAX_TEXT = 256;
const DAY = 86_400;

export function toDraft(view: CloudEditData): Draft {
  const { account } = view;
  return {
    name: account.name,
    host: account.server.host,
    port: account.server.port ? String(account.server.port) : "",
    version: account.version,
    lang: account.lang,
    config: structuredClone(view.config),
  };
}

export function isDirty(draft: Draft, baseline: Draft): boolean {
  return JSON.stringify(draft) !== JSON.stringify(baseline);
}

/** The port as a number (NaN while it is not one). */
export function portOf(draft: Draft): number {
  const text = draft.port.trim();
  if (text === "") return 0;
  return /^\d{1,5}$/.test(text) ? Number(text) : Number.NaN;
}

/** The server would change (every stored seal names it). */
export function serverChanges(draft: Draft, view: CloudEditData): boolean {
  const { host, port } = view.account.server;
  return draft.host.trim().toLowerCase() !== host || portOf(draft) !== port;
}

/** What the backend saves: only what changed, the config whole. */
export function toEdit(draft: Draft, baseline: Draft, view: CloudEditData): CloudEdit {
  const changed = <K extends keyof Draft>(key: K) =>
    JSON.stringify(draft[key]) !== JSON.stringify(baseline[key]);
  const serverChanged = serverChanges(draft, view);
  return {
    rev: view.account.rev,
    name: changed("name") ? draft.name.trim() : null,
    server: serverChanged ? { host: draft.host.trim(), port: portOf(draft) } : null,
    version: changed("version") ? draft.version : null,
    lang: changed("lang") ? draft.lang : null,
    config: changed("config") || serverChanged ? draft.config : null,
  };
}

/** The next free row id (`c1`, `r2`): unique across commands and rules. */
export function nextId(config: CloudEditConfig, prefix: "c" | "r"): string {
  const taken = new Set([...config.commands.map((c) => c.id), ...config.rules.map((r) => r.id)]);
  let n = 1;
  while (taken.has(`${prefix}${n}`)) n++;
  return `${prefix}${n}`;
}

export function newCommand(config: CloudEditConfig): Command {
  return {
    id: nextId(config, "c"),
    enabled: true,
    on: "join",
    delay: 2,
    every: null,
    at: null,
    words: { kind: "plain", text: "" },
  };
}

export function newRule(config: CloudEditConfig, minCooldown: number): Rule {
  return {
    id: nextId(config, "r"),
    enabled: true,
    pattern: "",
    regex: false,
    from: "any",
    ownerOnly: false,
    cooldown: Math.max(30, minCooldown),
    reply: { kind: "plain", text: "" },
  };
}

/** A row's trigger changed: its own fields, with sensible starting values. */
export function withTrigger(row: Command, on: Trigger, minInterval: number): Command {
  if (on === "interval") {
    const min = Math.max(300, minInterval);
    return { ...row, on, delay: null, at: null, every: row.every ?? { min, max: min * 2 } };
  }
  if (on === "daily") return { ...row, on, delay: null, every: null, at: row.at ?? ["08:00"] };
  return { ...row, on, every: null, at: null, delay: row.delay ?? 2 };
}

/** Words typed into a row: a password command is secret at once (it is sealed before it is sent). */
export function typed(words: Words, text: string): Words {
  if (words.kind === "secret" || looksSensitive(text)) return { kind: "secret", text, kept: false };
  return { kind: "plain", text };
}

/**
 * The contract's text rule (pl_cloud_afk::check_text), for the line under a field; the shared
 * cases are crates/pl-cloud-afk/tests/vectors/text-rule-v1.json.
 */
export function textProblem(text: string): string | null {
  if (text === "") return "empty";
  if (text.length > MAX_TEXT) return "length";
  // A lone UTF-16 surrogate (a broken paste): not well-formed Unicode.
  if (/\p{Cs}/u.test(text)) return "unicode";
  if (text.startsWith(" ") || text.endsWith(" ")) return "space";
  if (/[\p{Cc}\p{Zl}\p{Zp}]/u.test(text)) return "control";
  // A client trims these first: a `\` after them still starts its internal command.
  if (withoutLeadingIgnorable(text).startsWith("\\")) return "backslash";
  for (const match of text.matchAll(/%([A-Za-z0-9_\u0080-\uffff]+)%/g)) {
    if (!VARIABLES.has((match[1] ?? "").replace(/[A-Z]/g, (c) => c.toLowerCase())))
      return "variable";
  }
  return null;
}

function wordsProblem(words: Words): string | null {
  return words.kind === "sealed" ? null : textProblem(words.text);
}

/** Problems found while typing (the backend finds the rest, pattern safety included). */
export function localIssues(
  draft: Draft,
  limits: { minIntervalSec?: number; minRuleCooldownSec?: number },
): Issue[] {
  const issues: Issue[] = [];
  const general = (field: string, problem: string) =>
    issues.push({ place: "general", index: null, field, problem });
  const row = (place: "commands" | "rules", index: number, field: string, problem: string) =>
    issues.push({ place, index, field, problem });
  if (!NAME.test(draft.name.trim())) general("name", "format");
  const host = draft.host.trim();
  if (!host) general("server", "format");
  else if (/[\s/:@]/.test(host)) general("server", "format");
  const port = portOf(draft);
  if (Number.isNaN(port) || port > 65_535) general("port", "port");
  const { config } = draft;
  if (config.owner && !NAME.test(config.owner)) general("owner", "format");
  const anti = config.antiAfk;
  if (
    anti.delayMin < 10 ||
    anti.delayMax > 3600 ||
    anti.delayMin > anti.delayMax ||
    !Number.isInteger(anti.delayMin) ||
    !Number.isInteger(anti.delayMax)
  )
    general("antiAfk", "range");
  const minInterval = limits.minIntervalSec ?? 60;
  config.commands.forEach((command, index) => {
    if (command.on === "interval") {
      const every = command.every;
      if (!every || !Number.isInteger(every.min) || !Number.isInteger(every.max))
        row("commands", index, "every", "range");
      else if (every.min > every.max) row("commands", index, "every", "order");
      else if (every.min < minInterval || every.max > DAY) row("commands", index, "every", "range");
    } else if (command.on === "daily") {
      const at = command.at ?? [];
      if (!at.length) row("commands", index, "at", "empty");
      else if (at.length > 6) row("commands", index, "at", "many");
      else if (!at.every((time) => CLOCK.test(time))) row("commands", index, "at", "format");
      else if (new Set(at).size !== at.length) row("commands", index, "at", "duplicate");
    } else {
      const delay = command.delay;
      if (
        delay === null ||
        delay === undefined ||
        !Number.isInteger(delay) ||
        delay < 0 ||
        delay > 60
      )
        row("commands", index, "delay", "range");
    }
    const problem = wordsProblem(command.words);
    if (problem) row("commands", index, "text", problem);
  });
  const minCooldown = limits.minRuleCooldownSec ?? 10;
  config.rules.forEach((rule, index) => {
    const pattern = rule.pattern.trim();
    if (!pattern) row("rules", index, "match", "empty");
    else if (pattern.length > MAX_TEXT) row("rules", index, "match", "tooLong");
    if (rule.ownerOnly && !config.owner) row("rules", index, "ownerOnly", "owner");
    else if (rule.ownerOnly && rule.from === "server") row("rules", index, "ownerOnly", "server");
    if (!Number.isInteger(rule.cooldown) || rule.cooldown < minCooldown || rule.cooldown > DAY)
      row("rules", index, "cooldown", "range");
    const problem = wordsProblem(rule.reply);
    if (problem) row("rules", index, "reply", problem);
  });
  return issues;
}

/** The text key of an issue (`cloudAfk.settings.issues.<key>`). */
export function issueKey(issue: Issue): string {
  const { field, problem } = issue;
  switch (field) {
    case "name":
      return problem === "duplicate" ? "nameDuplicate" : "name";
    case "server":
      if (problem === "dot") return "hostDot";
      if (problem === "private" || problem === "self") return "hostPrivate";
      if (problem === "port") return "port";
      if (problem === "blocked" || problem === "full" || problem === "changeLimit") return problem;
      return "host";
    case "port":
      return "port";
    case "version":
      return "version";
    case "antiAfk":
      return "antiAfk";
    case "owner":
      return "name";
    case "count":
      return "limit";
    case "delay":
      return "delay";
    case "every":
      return problem === "order" ? "order" : "every";
    case "at":
      return (
        {
          format: "timesFormat",
          duplicate: "timesDuplicate",
          empty: "timesEmpty",
          many: "timesMany",
        }[problem] ?? "timesFormat"
      );
    case "cooldown":
      return "cooldown";
    case "ownerOnly":
      return problem === "server" ? "ownerServer" : "owner";
    case "match":
      return ["empty", "tooLong", "invalid", "tooComplex"].includes(problem) ? problem : "invalid";
    case "text":
    case "reply":
      return [
        "empty",
        "length",
        "unicode",
        "space",
        "control",
        "backslash",
        "variable",
        "secret",
        "reseal",
      ].includes(problem)
        ? problem
        : "other";
    default:
      return problem === "reseal" ? "reseal" : "other";
  }
}

/** The issue of a field of a row (or of the general part, `index` null). */
export function issueAt(
  issues: Issue[],
  place: string,
  index: number | null,
  field: string,
): Issue | undefined {
  return issues.find((i) => i.place === place && i.index === index && i.field === field);
}
