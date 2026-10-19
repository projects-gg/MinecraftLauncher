// What the import screens show and send, without React (IMPORT1): the rows of each launcher
// found, the ticks a screen starts with, the selected size against the free space, the request,
// and the summary's lines. The words are the components' (locales/tr-import.ts).
import type {
  ImportProfile,
  ImportReport,
  ImportRequest,
  ImportScanView,
  ImportShared,
  ImportSource,
  ItemOutcome,
  ProfileMeasure,
  SourceKind,
  SourceMeasure,
} from "@/lib/api";

/** Rows bigger than this start unticked: the player decides about gigabytes. */
export const LARGE_BYTES = 4 * 1024 ** 3;
/** Per launcher, this many of the most recently used profiles start ticked. */
export const FIRST_PROFILES = 5;

export const SHARED_KINDS: ImportShared[] = [
  "options",
  "servers",
  "worlds",
  "resourcePacks",
  "shaderPacks",
  "screenshots",
];

export type RowKey = string;
export const sharedKey = (sourceId: string, kind: ImportShared): RowKey =>
  `${sourceId}|shared|${kind}`;
export const profileKey = (sourceId: string, id: string): RowKey => `${sourceId}|profile|${id}`;

/**
 * - `ready`: can be ticked.
 * - `kept`: game settings, while the Projects instance has its own (they stay).
 * - `imported`: a profile an earlier import already made into an instance.
 * - `unsupported`: a profile whose loader or version cannot be installed here.
 */
export type RowState = "ready" | "kept" | "imported" | "unsupported";

export interface SharedRow {
  type: "shared";
  key: RowKey;
  sourceId: string;
  kind: ImportShared;
  count: number;
  /** Unknown until the source is measured. */
  bytes?: number;
  state: RowState;
}

export interface ProfileRow {
  type: "profile";
  key: RowKey;
  sourceId: string;
  profile: ImportProfile;
  measure?: ProfileMeasure;
  bytes?: number;
  state: RowState;
}

export type ImportRow = SharedRow | ProfileRow;

export interface SourceRows {
  source: ImportSource;
  shared: SharedRow[];
  profiles: ProfileRow[];
}

function sharedCount(source: ImportSource, kind: ImportShared): number {
  const shared = source.shared;
  if (!shared) return 0;
  switch (kind) {
    case "options":
      return shared.options ? 1 : 0;
    case "servers":
      return shared.servers;
    case "worlds":
      return shared.worlds;
    case "resourcePacks":
      return shared.resourcePacks;
    case "shaderPacks":
      return shared.shaderPacks;
    case "screenshots":
      return shared.screenshots;
  }
}

function sharedBytes(kind: ImportShared, measure: SourceMeasure | undefined): number | undefined {
  // Settings and the server list are a few kilobytes.
  if (kind === "options" || kind === "servers") return 0;
  const sizes = measure?.shared;
  if (!sizes) return undefined;
  return sizes[kind];
}

/**
 * What a source's measurement covers: its profiles and how many shared things it has. When a
 * profile or a world turns up (the player made one in the other launcher meanwhile), the key
 * changes and the source is measured again.
 */
export function rowsKey(source: ImportSource): string {
  const shared = source.shared
    ? SHARED_KINDS.map((kind) => sharedCount(source, kind)).join("/")
    : "";
  return [shared, ...source.profiles.map((p) => p.id).sort()].join("\n");
}

/**
 * The rows each source (by id) is measured for: a source seen for the first time at once, a
 * source whose rows changed for the rows it had until the new ones `settled` (a debounce: changes
 * in a burst make one measurement, none is dropped).
 */
export function measureKeys(
  current: Record<string, string>,
  settled: Record<string, string>,
): Record<string, string> {
  return Object.fromEntries(Object.entries(current).map(([id, key]) => [id, settled[id] ?? key]));
}

/**
 * The rows of every launcher found, in the scan's order (the `.minecraft` folders first).
 * `upToDate` names the sources whose measurement covers the rows they have now: a profile such a
 * measurement does not know is gone from the other launcher and weighs nothing. Under an older
 * measurement it waits for its size.
 */
export function rowsOf(
  scan: ImportScanView,
  measures: Record<string, SourceMeasure | undefined>,
  upToDate?: ReadonlySet<string>,
): SourceRows[] {
  return scan.sources.map((source) => {
    const measure = measures[source.id];
    const current = measure !== undefined && upToDate?.has(source.id) === true;
    const shared: SharedRow[] = SHARED_KINDS.filter((kind) => sharedCount(source, kind) > 0).map(
      (kind) => ({
        type: "shared",
        key: sharedKey(source.id, kind),
        sourceId: source.id,
        kind,
        count: sharedCount(source, kind),
        bytes: sharedBytes(kind, measure),
        state: kind === "options" && scan.projectsHasOptions ? "kept" : "ready",
      }),
    );
    const profiles: ProfileRow[] = source.profiles.map((profile) => {
      const measured = measure?.profiles.find((p) => p.id === profile.id);
      const state: RowState =
        profile.unsupported || !profile.minecraft
          ? "unsupported"
          : profile.importedAs
            ? "imported"
            : "ready";
      return {
        type: "profile",
        key: profileKey(source.id, profile.id),
        sourceId: source.id,
        profile,
        measure: measured,
        bytes: state === "ready" ? (measured?.bytes ?? (current ? 0 : undefined)) : 0,
        state,
      };
    });
    return { source, shared, profiles };
  });
}

export const allRows = (rows: SourceRows[]): ImportRow[] =>
  rows.flatMap((r) => [...r.shared, ...r.profiles]);

/** Whether `kind` can be dropped to make room (settings and servers weigh nothing). */
const weighty = (row: ImportRow) =>
  row.type === "profile" || (row.kind !== "options" && row.kind !== "servers");

/**
 * The ticks a screen starts with: everything that can come over, except rows over
 * [`LARGE_BYTES`] and profiles past the first [`FIRST_PROFILES`] of each launcher. When the
 * free space (less the reserve) is known and the ticks do not fit, the biggest rows go first.
 */
export function defaultTicks(
  rows: SourceRows[],
  freeBytes: number | null | undefined,
  reserveBytes: number,
): Set<RowKey> {
  const ticks = new Set<RowKey>();
  for (const source of rows) {
    for (const row of source.shared) {
      if (row.state === "ready" && (row.bytes ?? 0) <= LARGE_BYTES) ticks.add(row.key);
    }
    source.profiles
      .filter((row) => row.state === "ready")
      .slice(0, FIRST_PROFILES)
      .filter((row) => (row.bytes ?? 0) <= LARGE_BYTES)
      .forEach((row) => {
        ticks.add(row.key);
      });
  }
  if (freeBytes === null || freeBytes === undefined) return ticks;
  const room = Math.max(0, freeBytes - reserveBytes);
  const byWeight = allRows(rows)
    .filter((row) => ticks.has(row.key) && weighty(row))
    .sort((a, b) => (b.bytes ?? 0) - (a.bytes ?? 0));
  let total = selectedBytes(rows, ticks);
  for (const row of byWeight) {
    if (total <= room) break;
    ticks.delete(row.key);
    total -= row.bytes ?? 0;
  }
  return ticks;
}

/**
 * The ticks shown: the player's own choices, and the defaults for rows they have not touched
 * (those follow the sizes as they arrive). Only rows that can come over are ever ticked.
 */
export function ticksOf(
  rows: SourceRows[],
  defaults: Set<RowKey>,
  choices: ReadonlyMap<RowKey, boolean>,
): Set<RowKey> {
  const ticks = new Set<RowKey>();
  for (const row of allRows(rows)) {
    const on = choices.get(row.key) ?? defaults.has(row.key);
    if (on && row.state === "ready") ticks.add(row.key);
  }
  return ticks;
}

/** Bytes of the ticked rows (rows not measured yet count as nothing). */
export function selectedBytes(rows: SourceRows[], ticks: Set<RowKey>): number {
  return allRows(rows)
    .filter((row) => ticks.has(row.key))
    .reduce((sum, row) => sum + (row.bytes ?? 0), 0);
}

/** A ticked row's size is not known yet. */
export function measuring(rows: SourceRows[], ticks: Set<RowKey>): boolean {
  return allRows(rows).some((row) => ticks.has(row.key) && row.bytes === undefined);
}

/** The ticked rows fit next to the reserve (unknown free space: assumed to fit). */
export function fits(total: number, freeBytes: number | null | undefined, reserveBytes: number) {
  if (freeBytes === null || freeBytes === undefined || total === 0) return true;
  return total + reserveBytes <= freeBytes;
}

export function requestOf(rows: SourceRows[], ticks: Set<RowKey>): ImportRequest {
  const sources = rows
    .map((source) => ({
      sourceId: source.source.id,
      shared: source.shared.filter((row) => ticks.has(row.key)).map((row) => row.kind),
      profiles: source.profiles.filter((row) => ticks.has(row.key)).map((row) => row.profile.id),
    }))
    .filter((source) => source.shared.length > 0 || source.profiles.length > 0);
  return { sources };
}

/** Launchers by their own names (brand names: never translated). */
export const LAUNCHER_NAMES: Record<SourceKind, string> = {
  minecraft: "Minecraft Launcher",
  tLauncher: "TLauncher",
  skLauncher: "SKlauncher",
  curseForge: "CurseForge",
  modrinth: "Modrinth App",
  prism: "Prism Launcher",
  polyMc: "PolyMC",
  multiMc: "MultiMC",
};

/** A piece of a row's second line: a key under `import.` and its values. */
export interface MetaPart {
  key: string;
  params?: Record<string, string | number>;
}

/**
 * A profile row's second line: what it runs (when its name does not say), its mods, the mods
 * that stay behind, its worlds, "without OptiFine"; or why it cannot come over.
 */
export function profileMeta(row: ProfileRow): MetaPart[] {
  const p = row.profile;
  if (row.state === "imported") return [{ key: "profile.imported" }];
  if (row.state === "unsupported") {
    if (p.unsupported?.kind === "loader") {
      return [
        p.unsupported.name
          ? { key: "profile.loader", params: { name: p.unsupported.name } }
          : { key: "profile.unknownLoader" },
      ];
    }
    return [{ key: "profile.unknownVersion" }];
  }
  const parts: MetaPart[] = [];
  if (p.minecraft && !p.name.includes(p.minecraft)) {
    parts.push({ key: "profile.version", params: { loader: p.loader, version: p.minecraft } });
  }
  if (p.loader !== "vanilla") {
    const mods = row.measure?.mods ?? p.modFiles;
    const left = row.measure?.leftMods ?? 0;
    if (mods > 0) parts.push({ key: "profile.mods", params: { count: mods } });
    if (left > 0) {
      parts.push(
        mods > 0
          ? { key: "profile.leftMods", params: { count: left } }
          : { key: "profile.noModsFit" },
      );
    }
  }
  const worlds = row.measure?.worlds ?? 0;
  if (worlds > 0) parts.push({ key: "profile.worlds", params: { count: worlds } });
  if (p.withoutOptifine) parts.push({ key: "profile.noOptifine" });
  return parts;
}

/** The offline player name a launcher remembers (TLauncher), for the account step. */
export function suggestedName(scan: ImportScanView | undefined): string | undefined {
  return scan?.sources.find((s) => s.offlineName)?.offlineName ?? undefined;
}

// --- the summary -------------------------------------------------------------------------------

export type Tone = "done" | "partial" | "skipped" | "failed";

/** One line of the summary: a locale key under `import.`, its values and the item it is about. */
export interface SummaryLine {
  /** Its item (a source's shared kind or profile), unique in the summary. */
  id: string;
  tone: Tone;
  key: string;
  params: Record<string, string | number>;
  /** The shared item the line names (its words: `import.what.<kind>`). */
  what?: ImportShared;
  /** Where the line's things came from (words: `import.from.<kind>`, `import.at.<kind>`). */
  source?: SourceKind;
  /** Follow-up sentences in the same line. */
  more: Array<{ key: string; params: Record<string, string | number>; what?: ImportShared }>;
}

/** Follow-ups that say where worlds went (one list, not sentences). */
export const PLACE_KEYS = ["outcome.worldsIn", "outcome.worldsInOne"];

/** Why some of an item's things stayed behind (`import.outcome.leftBecause.<key>`). */
function leftReason(reason: ItemOutcome["reason"]): string {
  switch (reason) {
    case "noSpace":
    case "inUse":
    case "unreadable":
    case "noTarget":
    case "cancelled":
    case "needsInstance":
      return reason;
    default:
      return "failed";
  }
}

const lineId = (outcome: ItemOutcome) =>
  `${outcome.sourceId}|${outcome.item.type === "shared" ? outcome.item.kind : `profile|${outcome.item.id}`}`;

function sharedLine(outcome: ItemOutcome, kind: ImportShared): SummaryLine | null {
  const line = (
    tone: Tone,
    key: string,
    params: Record<string, string | number> = {},
  ): SummaryLine => ({
    id: lineId(outcome),
    tone,
    key,
    params,
    what: kind,
    source: outcome.sourceKind,
    more: [],
  });
  const reasonLine = (reason: ItemOutcome["reason"]) => {
    switch (reason) {
      case "noSpace":
      case "noTarget":
      case "unreadable":
      case "inUse":
      case "needsInstance":
        return `outcome.${reason}`;
      default:
        return "outcome.failed";
    }
  };
  switch (outcome.status) {
    case "done":
    case "partial": {
      const done =
        kind === "options"
          ? line("done", "outcome.options")
          : line(outcome.status, `outcome.${kind}`, { count: outcome.copied });
      // Worlds go into the instances that play them: the line says which.
      if (kind === "worlds") {
        const one = outcome.destinations.length === 1;
        for (const d of outcome.destinations) {
          done.more.push({
            key: one ? "outcome.worldsInOne" : "outcome.worldsIn",
            params: { name: d.instanceName, count: d.count },
          });
        }
      }
      if (outcome.status === "partial" && outcome.left > 0) {
        done.more.push({
          key: `outcome.leftBecause.${leftReason(outcome.reason)}`,
          params: { count: outcome.left },
        });
      }
      return done;
    }
    case "skipped":
      if (outcome.reason === "keptExisting") return line("skipped", "outcome.optionsKept");
      if (outcome.reason === "alreadyThere") return line("skipped", "outcome.already");
      return null;
    case "failed":
      return line("failed", reasonLine(outcome.reason));
  }
}

function profileLine(outcome: ItemOutcome): SummaryLine | null {
  const name = outcome.label;
  const line = (
    tone: Tone,
    key: string,
    params: Record<string, string | number> = {},
  ): SummaryLine => ({
    id: lineId(outcome),
    tone,
    key,
    params: { name, ...params },
    more: [],
  });
  switch (outcome.status) {
    case "done":
    case "partial": {
      const mods = outcome.copied;
      // Mods made for another version stay behind by design: that is no failure.
      const tone: Tone = missingPart(outcome) ? "partial" : "done";
      const added =
        mods > 0
          ? line(tone, "outcome.profileMods", { count: mods })
          : line(tone, "outcome.profile");
      if (outcome.status === "partial") {
        added.more.push(
          outcome.reason === "incompatibleMods"
            ? { key: "outcome.profileLeftMods", params: { count: outcome.left } }
            : { key: "outcome.profileLeftFiles", params: {} },
        );
      }
      return added;
    }
    case "skipped":
      if (outcome.reason === "alreadyThere") return line("skipped", "outcome.profileAlready");
      if (outcome.reason === "unsupported") return line("failed", "outcome.profileUnsupported");
      return null;
    case "failed":
      return line(
        "failed",
        outcome.reason === "noSpace" ? "outcome.profileNoSpace" : "outcome.profileFailed",
      );
  }
}

/** The summary's lines in the order the items ran; cancelled items are one line at the end. */
export function summaryLines(report: ImportReport): SummaryLine[] {
  const lines: SummaryLine[] = [];
  let cancelled = false;
  for (const outcome of report.outcomes) {
    if (outcome.reason === "cancelled" && outcome.status !== "partial") {
      cancelled = true;
      continue;
    }
    const line =
      outcome.item.type === "shared"
        ? sharedLine(outcome, outcome.item.kind)
        : profileLine(outcome);
    if (line) lines.push(line);
  }
  if (cancelled || report.cancelled) {
    lines.push({
      id: "cancelled",
      tone: "skipped",
      key: "outcome.cancelled",
      params: {},
      more: [],
    });
  }
  return lines;
}

/** Something the player picked did not come over (mods made for another version do not count). */
function missingPart(outcome: ItemOutcome): boolean {
  return (
    outcome.status === "failed" ||
    (outcome.status === "partial" && outcome.reason !== "incompatibleMods")
  );
}

/** The summary's title (`import.done.<key>`). */
export function summaryTitle(
  report: ImportReport,
): "title" | "titleSome" | "titleCancelled" | "titleFailed" {
  if (report.cancelled) return "titleCancelled";
  const worked = report.outcomes.filter((o) => o.status === "done" || o.status === "partial");
  const missing = report.outcomes.some(missingPart);
  if (worked.length === 0 && missing) return "titleFailed";
  return missing ? "titleSome" : "title";
}

/** Technical details of what failed, for "Ayrıntıları kopyala". */
export function failureDetails(report: ImportReport): string {
  return report.outcomes
    .filter((o) => o.detail)
    .map((o) => {
      const item = o.item.type === "shared" ? o.item.kind : `${o.label} (${o.item.id})`;
      return `${o.sourceId} ${item}: ${o.status} ${o.reason ?? ""}\n${o.detail}`;
    })
    .join("\n\n");
}
