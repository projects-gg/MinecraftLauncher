// Editing model of the AFK settings sheet: the page from the backend plus row keys and
// in-progress password replacements, the dirty check, and the inline validation. The backend runs
// the same rules again on save (crates/pl-afk/src/form.rs); the ranges here mirror its clamps.
import type {
  AfkSettingsView,
  AutoRespondRule,
  LoginCommandLine,
  PatternProblem,
  ScheduledTask,
  SettingsPage,
  ValidationIssue,
} from "@/lib/api";

export const SECTIONS = [
  "server",
  "login",
  "scheduler",
  "antiAfk",
  "relog",
  "eat",
  "respond",
  "chatLog",
  "alerts",
] as const;

export type SectionId = (typeof SECTIONS)[number];

/**
 * The panel's rail (SHEET-UX): the nine features in five sections, by what the player asks (where
 * it connects, what it types, what it does in the game, how it answers, what it records).
 */
export const GROUPS = ["connection", "commands", "game", "replies", "log"] as const;

export type GroupId = (typeof GROUPS)[number];

export const GROUP_OF: Record<SectionId, GroupId> = {
  server: "connection",
  relog: "connection",
  login: "commands",
  scheduler: "commands",
  antiAfk: "game",
  eat: "game",
  respond: "replies",
  alerts: "log",
  chatLog: "log",
};

/** The page's fields each section edits (every field belongs to one). */
const GROUP_FIELDS: Record<GroupId, Array<keyof SettingsPage>> = {
  connection: [
    "serverHost",
    "serverPort",
    "minecraftVersion",
    "autoStart",
    "allowResourcePacks",
    "autoRelog",
  ],
  commands: ["loginCommands", "scheduler"],
  game: ["antiAfk", "autoEat"],
  replies: ["autoRespond"],
  log: ["alerts", "chatLog"],
};

/** The sections holding edits that saving would change. */
export function unsavedGroups(draft: Draft, baseline: Draft): Set<GroupId> {
  const now = JSON.parse(comparable(draft)) as Record<string, unknown>;
  const saved = JSON.parse(comparable(baseline)) as Record<string, unknown>;
  const differs = (field: string) => JSON.stringify(now[field]) !== JSON.stringify(saved[field]);
  return new Set(GROUPS.filter((group) => GROUP_FIELDS[group].some(differs)));
}

/** Number ranges (form.rs): the backend clamps into these, the sheet asks for them. */
export const LIMITS = {
  port: { min: 0, max: 65535, integer: true },
  loginDelay: { min: 0, max: 60, integer: false },
  antiAfkDelay: { min: 1, max: 3600, integer: false },
  walkRange: { min: 1, max: 32, integer: true },
  walkRetries: { min: 1, max: 100, integer: true },
  relogDelay: { min: 5, max: 3600, integer: false },
  relogRetries: { min: 1, max: 100, integer: true },
  threshold: { min: 0, max: 20, integer: true },
  cooldown: { min: 0, max: 86_400, integer: true },
  taskSeconds: { min: 1, max: 604_800, integer: false },
} as const;

export type Limit = (typeof LIMITS)[keyof typeof LIMITS];

/** Retry count shown when "unlimited" is switched off (legacy default). */
export const DEFAULT_RETRIES = 10;

/** Longest auto-respond pattern (`pattern::MAX_PATTERN_CHARS`). */
export const MAX_PATTERN_CHARS = 256;

let nextKey = 0;
const key = () => `r${++nextKey}`;

export type LoginRow =
  | { key: string; kind: "text"; text: string }
  | { key: string; kind: "stored"; index: number; command: string }
  /** A stored secret being replaced: the command name stays, new arguments are typed. */
  | { key: string; kind: "replace"; index: number; command: string; args: string };

export type TaskRow = ScheduledTask & { key: string };
export type RuleRow = AutoRespondRule & { key: string };

export interface Draft {
  page: SettingsPage;
  login: LoginRow[];
  tasks: TaskRow[];
  rules: RuleRow[];
}

export function toDraft(page: SettingsPage): Draft {
  return {
    page,
    login: page.loginCommands.commands.map(
      (line): LoginRow =>
        line.kind === "stored"
          ? { key: key(), kind: "stored", index: line.index, command: line.command }
          : { key: key(), kind: "text", text: line.text },
    ),
    tasks: page.scheduler.tasks.map((task) => ({ ...task, key: key() })),
    rules: page.autoRespond.rules.map((rule) => ({ ...rule, key: key() })),
  };
}

const strip = <T extends { key: string }>({ key: _key, ...rest }: T) => rest;

/** The page to send: rows without their keys, replacements as typed lines. */
export function fromDraft(draft: Draft): SettingsPage {
  return {
    ...draft.page,
    loginCommands: {
      ...draft.page.loginCommands,
      commands: draft.login.map((row): LoginCommandLine => {
        if (row.kind === "stored")
          return { kind: "stored", index: row.index, command: row.command };
        if (row.kind === "replace")
          return { kind: "text", text: `${row.command} ${row.args.trim()}` };
        return { kind: "text", text: row.text };
      }),
    },
    scheduler: { ...draft.page.scheduler, tasks: draft.tasks.map(strip) },
    autoRespond: { ...draft.page.autoRespond, rules: draft.rules.map(strip) },
  };
}

export const newTask = (): TaskRow => ({
  key: key(),
  name: "",
  action: "",
  onLogin: false,
  onFirstLogin: false,
  intervalEnabled: true,
  minSeconds: 300,
  maxSeconds: 600,
});

export const newRule = (): RuleRow => ({
  key: key(),
  pattern: "",
  reply: "",
  privateReply: "",
  otherReply: "",
  ownersOnly: false,
  cooldownSeconds: 0,
  isRegex: false,
});

export const newLoginRow = (): LoginRow => ({ key: key(), kind: "text", text: "" });

/** Line lists compare by their saved form (trimmed, blank lines dropped). */
const lines = (items: string[]) => items.map((s) => s.trim()).filter(Boolean);

/**
 * Comparable form of a draft: what saving would actually change. A stored secret opened with
 * "Değiştir" and nothing typed yet is still the stored one: the press alone brought up the change
 * bar, whose save would have asked for a password nobody had typed (AFK-CLEAR look 2).
 */
function comparable(draft: Draft): string {
  const page = fromDraft({
    ...draft,
    login: draft.login.map(
      (row): LoginRow =>
        row.kind === "replace" && !row.args.trim()
          ? { key: row.key, kind: "stored", index: row.index, command: row.command }
          : row,
    ),
  });
  return JSON.stringify({
    ...page,
    serverHost: page.serverHost.trim(),
    antiAfk: { ...page.antiAfk, command: page.antiAfk.command.trim() },
    autoRelog: { ...page.autoRelog, kickMessages: lines(page.autoRelog.kickMessages) },
    alerts: {
      ...page.alerts,
      matches: lines(page.alerts.matches),
      excludes: lines(page.alerts.excludes),
    },
    loginCommands: {
      ...page.loginCommands,
      commands: page.loginCommands.commands.filter((c) => c.kind !== "text" || c.text.trim()),
    },
  });
}

export function isDirty(draft: Draft, baseline: Draft): boolean {
  return comparable(draft) !== comparable(baseline);
}

// --- Validation -----------------------------------------------------------------------------

export type IssueCode =
  | "hostInvalid"
  | "hostPort"
  | "number"
  | "integer"
  | "range"
  | "loginEmpty"
  | "secretEmpty"
  | "schedulerEmpty"
  | "taskAction"
  | "respondEmpty"
  | "rulePattern"
  | "ruleReply"
  | "patternInvalid"
  | "patternComplex"
  | "patternLong"
  /** A stop for a password the server refused, on the line that types it (never blocking). */
  | "wrongPassword"
  /** The same on a kept line: "Değiştir" first. */
  | "wrongPasswordKept";

export interface Issue {
  section: SectionId;
  /** Field id: `serverHost`, `task.<key>.action`, ... */
  field: string;
  code: IssueCode;
  /** Shown only after a save attempt (empty lists, unfinished rows). */
  structural?: boolean;
  limit?: Limit;
}

const HOST = /^[A-Za-z0-9_][A-Za-z0-9._-]*$/;

/** `form::is_valid_server_host`. */
export function isValidHost(host: string): boolean {
  const h = host.trim();
  return h === "" || (h.length <= 253 && HOST.test(h));
}

/** `form::split_host_port`: "host:25566" → ["host", 25566]. */
export function splitHostPort(address: string): [string, number] | null {
  const match = /^([^:]+):(\d{1,5})$/.exec(address.trim());
  if (!match?.[1]?.trim() || !match[2]) return null;
  const port = Number(match[2]);
  return port >= 1 && port <= 65535 ? [match[1].trim(), port] : null;
}

function checkNumber(
  value: number,
  limit: Limit,
  section: SectionId,
  field: string,
  out: Issue[],
): void {
  if (!Number.isFinite(value)) out.push({ section, field, code: "number" });
  else if (limit.integer && !Number.isInteger(value)) out.push({ section, field, code: "integer" });
  else if (value < limit.min || value > limit.max)
    out.push({ section, field, code: "range", limit });
}

export function validate(draft: Draft): Issue[] {
  const p = draft.page;
  const out: Issue[] = [];

  // Server
  const host = p.serverHost.trim();
  if (!isValidHost(host)) {
    const code = /:\d+$/.test(host) ? "hostPort" : "hostInvalid";
    out.push({ section: "server", field: "serverHost", code });
  }
  checkNumber(p.serverPort, LIMITS.port, "server", "serverPort", out);

  // Login commands
  const login = p.loginCommands;
  checkNumber(login.delaySeconds, LIMITS.loginDelay, "login", "login.delay", out);
  for (const row of draft.login) {
    if (row.kind === "replace" && !row.args.trim())
      // Shown once a save is tried: "Değiştir" alone is not a mistake yet.
      out.push({
        section: "login",
        field: `login.${row.key}`,
        code: "secretEmpty",
        structural: true,
      });
  }
  const hasCommand = draft.login.some((row) => row.kind !== "text" || row.text.trim());
  if (login.enabled && !hasCommand)
    out.push({ section: "login", field: "login.list", code: "loginEmpty", structural: true });

  // Scheduler (rows are only editable, so only checked, while it is on). Unfinished rows get
  // their own message, so the list-level one only shows for an empty list.
  if (p.scheduler.enabled) {
    if (!draft.tasks.length)
      out.push({
        section: "scheduler",
        field: "scheduler.list",
        code: "schedulerEmpty",
        structural: true,
      });
    for (const task of draft.tasks) {
      if (!task.action.trim())
        out.push({
          section: "scheduler",
          field: `task.${task.key}.action`,
          code: "taskAction",
          structural: true,
        });
      if (task.intervalEnabled) {
        checkNumber(task.minSeconds, LIMITS.taskSeconds, "scheduler", `task.${task.key}.min`, out);
        checkNumber(task.maxSeconds, LIMITS.taskSeconds, "scheduler", `task.${task.key}.max`, out);
      }
    }
  }

  // AFK prevention
  const anti = p.antiAfk;
  checkNumber(anti.delayMin, LIMITS.antiAfkDelay, "antiAfk", "antiAfk.min", out);
  checkNumber(anti.delayMax, LIMITS.antiAfkDelay, "antiAfk", "antiAfk.max", out);
  checkNumber(anti.walkRange, LIMITS.walkRange, "antiAfk", "antiAfk.walkRange", out);
  checkNumber(anti.walkRetries, LIMITS.walkRetries, "antiAfk", "antiAfk.walkRetries", out);

  // Reconnect
  const relog = p.autoRelog;
  checkNumber(relog.delayMin, LIMITS.relogDelay, "relog", "relog.min", out);
  checkNumber(relog.delayMax, LIMITS.relogDelay, "relog", "relog.max", out);
  if (relog.retries !== -1)
    checkNumber(relog.retries, LIMITS.relogRetries, "relog", "relog.retries", out);

  // Auto respond
  if (p.autoRespond.enabled) {
    if (!draft.rules.length)
      out.push({
        section: "respond",
        field: "respond.list",
        code: "respondEmpty",
        structural: true,
      });
    for (const rule of draft.rules) {
      if (!rule.pattern.trim())
        out.push({
          section: "respond",
          field: `rule.${rule.key}.pattern`,
          code: "rulePattern",
          structural: true,
        });
      if (!rule.reply.trim() && !rule.privateReply.trim() && !rule.otherReply.trim())
        out.push({
          section: "respond",
          field: `rule.${rule.key}.reply`,
          code: "ruleReply",
          structural: true,
        });
      checkNumber(
        rule.cooldownSeconds,
        LIMITS.cooldown,
        "respond",
        `rule.${rule.key}.cooldown`,
        out,
      );
    }
  }

  return out;
}

const SERVER_ISSUES: Record<Extract<ValidationIssue, string>, Issue> = {
  invalidServerHost: { section: "server", field: "serverHost", code: "hostInvalid" },
  loginCommandsEmpty: { section: "login", field: "login.list", code: "loginEmpty" },
  autoRespondNoRules: { section: "respond", field: "respond.list", code: "respondEmpty" },
  schedulerNoTasks: { section: "scheduler", field: "scheduler.list", code: "schedulerEmpty" },
};

const PATTERN_CODES: Record<PatternProblem, IssueCode> = {
  invalid: "patternInvalid",
  tooComplex: "patternComplex",
  tooLong: "patternLong",
};

/** A rule pattern the AFK client can not use, shown at the rule's pattern field. */
export function patternIssue(ruleKey: string, problem: PatternProblem): Issue {
  return { section: "respond", field: `rule.${ruleKey}.pattern`, code: PATTERN_CODES[problem] };
}

/** Problems the backend reported, placed like the sheet's own (rule numbers become rows). */
export function fromServerIssues(issues: ValidationIssue[], draft: Draft): Issue[] {
  return issues.map((issue): Issue => {
    if (typeof issue === "string") return { ...SERVER_ISSUES[issue] };
    const { rule, problem } = issue.autoRespondPattern;
    const row = draft.rules[rule];
    return row
      ? patternIssue(row.key, problem)
      : { section: "respond", field: "respond.list", code: PATTERN_CODES[problem] };
  });
}

/** True when `text` starts with a command whose arguments are a password (`/login hunter2`). */
export function isSecretLine(
  text: string,
  sensitive: AfkSettingsView["sensitiveCommands"],
): boolean {
  const trimmed = text.trim();
  const [name = "", ...rest] = trimmed.split(/[ \t]+/);
  const upper = name.toUpperCase();
  return rest.join("").length > 0 && sensitive.some((s) => s.toUpperCase() === upper);
}

/**
 * The line a stop for a password the server refused points at (the client names none): the first
 * login command holding a password (kept or typed), else the first auto-reply that types one, while
 * it still has its saved words. Shown with the rest, never blocking; a line being replaced is being
 * fixed already (AFK-CLEAR).
 */
export function passwordIssue(
  draft: Draft,
  baseline: Draft,
  sensitive: AfkSettingsView["sensitiveCommands"],
): Issue | null {
  if (draft.page.loginCommands.enabled) {
    for (const row of draft.login) {
      if (row.kind === "replace") return null;
      if (row.kind === "stored") {
        return { section: "login", field: `login.${row.key}`, code: "wrongPasswordKept" };
      }
      if (isSecretLine(row.text, sensitive)) {
        const saved = baseline.login.find((r) => r.key === row.key);
        return saved?.kind === "text" && saved.text === row.text
          ? { section: "login", field: `login.${row.key}`, code: "wrongPassword" }
          : null;
      }
    }
  }
  if (draft.page.autoRespond.enabled) {
    for (const rule of draft.rules) {
      const field = (["reply", "privateReply", "otherReply"] as const).find((f) =>
        isSecretLine(rule[f], sensitive),
      );
      if (!field) continue;
      const saved = baseline.rules.find((r) => r.key === rule.key);
      return saved && saved[field] === rule[field]
        ? { section: "respond", field: `rule.${rule.key}.reply`, code: "wrongPassword" }
        : null;
    }
  }
  return null;
}
