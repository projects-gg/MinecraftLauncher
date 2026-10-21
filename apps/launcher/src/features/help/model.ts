// The troubleshooter's rows, headline, words and support summary (pure; model.test.ts). The checks
// and their rules are the backend's (`pl_data::troubleshoot`); the announcement row comes from the
// home content the launcher already has (Studio's maintenance notice or the status page's).
import type { TFunction } from "i18next";
import type {
  HelpCheck,
  HelpCode,
  HelpFacts,
  HelpFix,
  HelpInstance,
  HelpLevel,
  HelpSystem,
  HomeStatus,
  LoaderKind,
  Maintenance,
} from "@/lib/api";
import {
  dateTimeFormat,
  formatBytes,
  formatMemory,
  formatRelative,
  numberFormat,
} from "@/lib/format";

/** A row of the panel: a check, or the announcement. */
export type RowCheck = HelpCheck | "notice";

/** The announcement's codes (the summary writes them as `NOTICE-OUTAGE` and so on). */
export type NoticeCode = "noticeOutage" | "noticeDegraded" | "noticeInfo" | "noticeMaintenance";

export interface Row {
  check: RowCheck;
  level: HelpLevel;
  code: HelpCode | NoticeCode;
  fix?: HelpFix;
  facts: HelpFacts & {
    /** The announcement's own title (Turkish, as the team wrote it). */
    title?: string;
    url?: string;
  };
}

export type Group = "connection" | "game";

/** The rows in the order they are shown, by group (the server's status right under its ping). */
export const GROUPS: Record<Group, readonly RowCheck[]> = {
  connection: [
    "internet",
    "server",
    "notice",
    "content",
    "friends",
    "microsoft",
    "discord",
    "clock",
  ],
  // The disk first: what the game needs before the game's own rows.
  game: ["disk", "java", "memory", "mods", "crash"],
};

export const GROUP_ORDER: readonly Group[] = ["connection", "game"];

/** Rows shown only when they found something (they are not part of a run's plan). */
export const CONDITIONAL: readonly RowCheck[] = ["clock", "memory", "notice"];

/** Higher is worse; the headline and the summary lead with the worst. */
export function severity(level: HelpLevel): number {
  return { problem: 4, warning: 3, note: 2, ok: 1, skipped: 0 }[level];
}

/**
 * The announcement row: Studio's maintenance notice (information, nothing to fix: it never blocks
 * anything), else the status page's active notice (an outage is a problem, a slowdown a warning,
 * a notice information).
 */
export function noticeRow(
  maintenance: Maintenance | undefined,
  status: HomeStatus | undefined,
  now = Date.now(),
): Row | null {
  if (maintenance) {
    return {
      check: "notice",
      level: "note",
      code: "noticeMaintenance",
      facts: { title: maintenance.title },
    };
  }
  if (!status || !(Date.parse(status.expiresAt) > now)) return null;
  const [level, code]: [HelpLevel, NoticeCode] =
    status.level === "outage"
      ? ["problem", "noticeOutage"]
      : status.level === "degraded"
        ? ["warning", "noticeDegraded"]
        : ["note", "noticeInfo"];
  return {
    check: "notice",
    level,
    code,
    fix: status.url ? "statusPage" : undefined,
    facts: { title: status.title, url: status.url },
  };
}

/**
 * The fix a row's button offers. One button per fix: a wrong clock is why secure connections
 * fail, and the clock's own row offers its settings, so the internet row (which says so) does not
 * offer them a second time. Rows that are fine offer nothing.
 */
export function shownFix(row: Row, rows: ReadonlyMap<RowCheck, Row>): HelpFix | undefined {
  if (row.level === "ok") return undefined;
  if (
    row.check === "internet" &&
    row.fix === "clockSettings" &&
    rows.get("clock")?.fix === "clockSettings"
  ) {
    return undefined;
  }
  return row.fix;
}

/**
 * Which rows each group shows: the run's planned checks, the conditional rows that found
 * something, in the fixed order. A group with nothing to show is left out.
 */
export function layout(
  planned: readonly RowCheck[],
  found: ReadonlySet<RowCheck>,
): { group: Group; checks: RowCheck[] }[] {
  return GROUP_ORDER.map((group) => ({
    group,
    checks: GROUPS[group].filter(
      (check) => planned.includes(check) || (CONDITIONAL.includes(check) && found.has(check)),
    ),
  })).filter((entry) => entry.checks.length > 0);
}

export type Verdict =
  | { kind: "checking" }
  | { kind: "fine" }
  | { kind: "problems" | "warnings"; count: number; checks: RowCheck[] };

/** The headline: still checking, all fine, or how many problems (else warnings) and where. */
export function verdict(rows: readonly Row[], running: boolean): Verdict {
  if (running) return { kind: "checking" };
  const of = (level: HelpLevel) =>
    rows.filter((row) => row.level === level).map((row) => row.check);
  const problems = of("problem");
  if (problems.length > 0) return { kind: "problems", count: problems.length, checks: problems };
  const warnings = of("warning");
  if (warnings.length > 0) return { kind: "warnings", count: warnings.length, checks: warnings };
  return { kind: "fine" };
}

/** `serverUnreachable` → `SERVER-UNREACHABLE`, `javaX86` → `JAVA-X86` (the summary's codes). */
export function supportCode(code: string): string {
  return code.replace(/([a-z0-9])([A-Z])/g, "$1-$2").toUpperCase();
}

/** The first letter in upper case, the way the interface's language writes it ("dün" → "Dün"). */
function capitalize(text: string, language: string): string {
  return text.charAt(0).toLocaleUpperCase(language === "en" ? "en" : "tr") + text.slice(1);
}

/** "12 dakika", "3 saat", "2 gün" / "12 minutes": how far the clock is off. */
export function clockAmount(t: TFunction, minutes: number): string {
  const off = Math.abs(minutes);
  if (off < 120) return t("help.amount.minutes", { count: Math.max(1, Math.round(off)) });
  if (off < 48 * 60) return t("help.amount.hours", { count: Math.round(off / 60) });
  return t("help.amount.days", { count: Math.round(off / (24 * 60)) });
}

const NEEDS_INTERNET = /NeedsInternet$/;
const UNCHECKED: ReadonlySet<string> = new Set(["javaUnknown", "modsUnknown", "checkFailed"]);

/** The row's one line. */
export function reasonOf(t: TFunction, row: Row, language: string, now = Date.now()): string {
  const f = row.facts;
  const code = row.code;
  if (NEEDS_INTERNET.test(code)) return t("help.reasons.needsInternet");
  if (UNCHECKED.has(code)) return t("help.reasons.checkFailed");
  switch (code) {
    case "noticeOutage":
    case "noticeDegraded":
    case "noticeInfo":
    case "noticeMaintenance":
      return f.title ?? "";
    case "internetInsecure":
      return t(
        row.fix === "clockSettings"
          ? "help.reasons.internetInsecureClock"
          : "help.reasons.internetInsecure",
      );
    case "serverOk":
      return f.players !== undefined
        ? t("help.reasons.serverOk", {
            ms: f.latencyMs ?? 0,
            players: numberFormat().format(f.players),
          })
        : t("help.reasons.serverOkQuiet", { ms: f.latencyMs ?? 0 });
    case "serverSlow":
      return t("help.reasons.serverSlow", { ms: f.latencyMs ?? 0 });
    case "clockWrong": {
      const minutes = f.minutes ?? 0;
      return t(minutes > 0 ? "help.reasons.clockAhead" : "help.reasons.clockBehind", {
        amount: clockAmount(t, minutes),
      });
    }
    case "diskOk":
    case "diskLow":
    case "diskFull":
      return t(`help.reasons.${code}`, { free: formatBytes(f.freeBytes ?? 0) });
    case "javaReady":
    case "javaCustomOk":
    case "javaX86":
      return t(`help.reasons.${code}`, { major: f.major ?? "?" });
    case "javaTooOld":
    case "javaTooNew":
      return t(`help.reasons.${code}`, { major: f.major ?? "?", need: f.need ?? "?" });
    case "modsOk":
      return (f.count ?? 0) === 0
        ? t("help.reasons.modsNone")
        : t("help.reasons.modsOk", { count: f.count });
    case "modsWarning":
    case "modsBlocking":
      return t(`help.reasons.${code}`, { count: f.count ?? 0 });
    case "crashRecent": {
      const when = f.at ? capitalize(formatRelative(f.at, now), language) : "";
      return f.name
        ? t("help.reasons.crashRecentMod", { when, name: f.name })
        : t("help.reasons.crashRecent", {
            when,
            title: t(`modHealth.crash.titles.${f.crash ?? "unknown"}`),
          });
    }
    default:
      return t(`help.reasons.${code}`);
  }
}

/** A row's title. */
export function titleOf(t: TFunction, check: RowCheck): string {
  return t(`help.checks.${check}`);
}

/** The fix's words: the fixes that have a name elsewhere keep it (one name per action). */
export function fixLabel(t: TFunction, fix: HelpFix): string {
  switch (fix) {
    case "makeSpace":
      return t("disk.makeSpace");
    case "installJava":
      return t("javaPrompt.install");
    case "instanceSettings":
      return t("modHealth.crash.settings");
    case "repair":
      return t("modHealth.crash.repair");
    default:
      return t(`help.fixes.${fix}`);
  }
}

/** The rows the headline names: "Disk alanı ve Modlar" / "Disk space and Mods". */
export function verdictLine(t: TFunction, checks: readonly RowCheck[], language: string): string {
  const names = checks.map((check) => titleOf(t, check));
  try {
    return new Intl.ListFormat(language === "en" ? "en-GB" : "tr-TR", {
      type: "conjunction",
    }).format(names);
  } catch {
    return names.join(", ");
  }
}

const LOADERS: Record<LoaderKind, string> = {
  vanilla: "Vanilla",
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoForge: "NeoForge",
};

export interface SummaryInput {
  t: TFunction;
  language: string;
  system?: HelpSystem;
  instance?: HelpInstance;
  /** In the order they are shown. */
  rows: readonly Row[];
  /** The code of the launch that failed before the troubleshooter opened. */
  launchCode?: string;
  now: Date;
}

/**
 * The text "Destek için özeti kopyala" copies: the launcher, the system, the instance by its kind
 * and versions (never its name), when the check ran, then every row, worst first, with its code in
 * brackets. Nothing else: the rows hold no account names, ids, paths, addresses or tokens.
 */
export function summaryText(input: SummaryInput): string {
  const { t, language, system, instance, rows, now } = input;
  const lines = [t("help.summary.title")];
  if (system) {
    lines.push(
      `${t("help.summary.launcher")}: ${system.launcher}${system.debug ? " (debug)" : ""}`,
      `${t("help.summary.system")}: ${system.os}, ${system.arch}, ${t("help.summary.memory")} ${formatMemory(system.memoryMb)}`,
    );
  }
  if (instance) {
    const kind =
      instance.kind === "official"
        ? [t("help.summary.official"), instance.packVersion].filter(Boolean).join(" ")
        : t("help.summary.custom");
    const loader = [LOADERS[instance.loader], instance.loaderVersion].filter(Boolean).join(" ");
    lines.push(
      `${t("help.summary.instance")}: ${kind}, Minecraft ${instance.minecraft}${instance.loader === "vanilla" ? "" : `, ${loader}`}`,
    );
  }
  lines.push(
    `${t("help.summary.checked")}: ${dateTimeFormat({ dateStyle: "short", timeStyle: "short" }).format(now)}`,
  );
  if (input.launchCode) lines.push(`${t("help.summary.lastLaunch")}: ${input.launchCode}`);
  lines.push("");
  const ordered = rows
    .map((row, index) => ({ row, index }))
    .sort((a, b) => severity(b.row.level) - severity(a.row.level) || a.index - b.index)
    .map(({ row }) => row);
  for (const row of ordered) {
    const reason = reasonOf(t, row, language, now.getTime());
    lines.push(
      `${t(`help.states.${row.level}`)}: ${titleOf(t, row.check)}. ${reason} [${supportCode(row.code)}]`,
    );
  }
  return lines.join("\n");
}
