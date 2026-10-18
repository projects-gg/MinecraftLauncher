// Selection, totals and reminder rules of the Temizlik card (pure, unit-tested).
import type {
  Leftover,
  LeftoverKind,
  OldSet,
  SharedFiles,
  SharedGroup,
  SharedItem,
} from "@/lib/api";
import { relativeTimeFormat } from "@/lib/format";

export type GroupId = "sets" | "shared" | "leftovers";

/** One selectable line of the card. */
export interface Row {
  /** `set:<key>`, `shared:<item id>` or `leftover:<kind>`. */
  id: string;
  group: GroupId;
  bytes: number;
}

export type Tri = "all" | "some" | "none";

const DAY = 24 * 60 * 60 * 1000;
const MIB = 1024 * 1024;
const GIB = 1024 * MIB;

export const setRowId = (set: OldSet) => `set:${set.key}`;
export const sharedRowId = (item: SharedItem | string) =>
  `shared:${typeof item === "string" ? item : item.id}`;
export const leftoverRowId = (kind: LeftoverKind) => `leftover:${kind}`;

/** Sets last used before the threshold (the list comes with the smallest threshold). */
export function staleSets(sets: OldSet[], days: number, now = Date.now()): OldSet[] {
  const cutoff = now - days * DAY;
  return sets.filter((set) => set.lastUsedAt !== undefined && Date.parse(set.lastUsedAt) < cutoff);
}

export function rowsOf(
  sets: OldSet[] | undefined,
  shared: SharedFiles | undefined,
  leftovers: Leftover[] | undefined,
): Row[] {
  return [
    ...(sets ?? []).map((set) => ({
      id: setRowId(set),
      group: "sets" as const,
      bytes: set.sizeBytes,
    })),
    ...(shared?.items ?? []).map((item) => ({
      id: sharedRowId(item),
      group: "shared" as const,
      bytes: item.bytes,
    })),
    ...(leftovers ?? []).map((item) => ({
      id: leftoverRowId(item.kind),
      group: "leftovers" as const,
      bytes: item.bytes,
    })),
  ];
}

/**
 * Selection is kept as the rows the player turned off, so rows that arrive later (a list that
 * loads, a longer threshold) start selected.
 */
export function groupState(rows: Row[], group: GroupId, off: ReadonlySet<string>): Tri {
  const mine = rows.filter((row) => row.group === group);
  const on = mine.filter((row) => !off.has(row.id)).length;
  if (on === 0) return "none";
  return on === mine.length ? "all" : "some";
}

/** A fully selected group turns off; otherwise every row of it turns on. */
export function toggleGroup(rows: Row[], group: GroupId, off: ReadonlySet<string>): Set<string> {
  const next = new Set(off);
  const mine = rows.filter((row) => row.group === group);
  const all = groupState(rows, group, off) === "all";
  for (const row of mine) {
    if (all) next.add(row.id);
    else next.delete(row.id);
  }
  return next;
}

export function toggleRow(off: ReadonlySet<string>, id: string): Set<string> {
  const next = new Set(off);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}

/**
 * Bytes the selection frees: every selected row, plus files shared by several unused game files
 * once all of their holders are selected.
 */
export function selectedBytes(
  rows: Row[],
  off: ReadonlySet<string>,
  groups: SharedGroup[] = [],
  only?: GroupId,
): number {
  const chosen = new Set(
    rows.filter((row) => !off.has(row.id) && (!only || row.group === only)).map((row) => row.id),
  );
  const items = rows.filter((row) => chosen.has(row.id)).reduce((sum, row) => sum + row.bytes, 0);
  if (only && only !== "shared") return items;
  const shared = groups
    .filter((group) => group.items.every((id) => chosen.has(sharedRowId(id))))
    .reduce((sum, group) => sum + group.bytes, 0);
  return items + shared;
}

/** Everything a group holds (all rows selected). */
export function groupBytes(rows: Row[], group: GroupId, groups: SharedGroup[] = []): number {
  return selectedBytes(rows, new Set(), groups, group);
}

/** What "Temizle" does with the current selection. */
export interface CleanupPlan {
  sets: OldSet[];
  sharedIds: string[];
  leftoverKinds: LeftoverKind[];
  setBytes: number;
  sharedBytes: number;
  leftoverBytes: number;
  totalBytes: number;
}

export function planOf(
  sets: OldSet[],
  shared: SharedFiles | undefined,
  leftovers: Leftover[],
  off: ReadonlySet<string>,
): CleanupPlan {
  const rows = rowsOf(sets, shared, leftovers);
  const chosenSets = sets.filter((set) => !off.has(setRowId(set)));
  const sharedIds = (shared?.items ?? [])
    .filter((item) => !off.has(sharedRowId(item)))
    .map((item) => item.id);
  const leftoverKinds = leftovers
    .filter((item) => !off.has(leftoverRowId(item.kind)))
    .map((item) => item.kind);
  const setBytes = selectedBytes(rows, off, [], "sets");
  const sharedBytes = selectedBytes(rows, off, shared?.groups ?? [], "shared");
  const leftoverBytes = selectedBytes(rows, off, [], "leftovers");
  return {
    sets: chosenSets,
    sharedIds,
    leftoverKinds,
    setBytes,
    sharedBytes,
    leftoverBytes,
    totalBytes: setBytes + sharedBytes + leftoverBytes,
  };
}

/**
 * Age of a date for a row, in the interface's language: "dün", "3 gün önce", "geçen ay", "4 ay
 * önce", "geçen yıl" / "yesterday", "3 days ago", "last month".
 */
export function ageLabel(iso: string, now = Date.now()): string {
  const relative = relativeTimeFormat();
  const days = Math.max(0, Math.round((now - Date.parse(iso)) / DAY));
  if (days < 30) return relative.format(-days, "day");
  if (days < 365) return relative.format(-Math.round(days / 30), "month");
  return relative.format(-Math.round(days / 365), "year");
}

// ---------------------------------------------------------------------------------------------
// The occasional reminder

/** Reminded about at most this often... */
export const REMIND_EVERY = 14 * DAY;
/** ...and the same gain again only after this long (unless it grew a lot). */
export const SAME_GAIN_QUIET = 45 * DAY;
const GROWTH = GIB;
/** A gain worth a reminder: 2 GB, or 500 MB when the disk is almost full (under 10 GB free). */
const WORTH = 2 * GIB;
const WORTH_LOW_DISK = 500 * MIB;
const LOW_DISK = 10 * GIB;

/** Whether to look at all (cheap checks before asking the backend). */
export function mayCheckReminder(input: {
  now: number;
  lastShownAt: number | null;
  onboarded: boolean;
  /** "Rahatsız etmeyin" or a game in progress. */
  quiet: boolean;
}): boolean {
  if (!input.onboarded || input.quiet) return false;
  return input.lastShownAt === null || input.now - input.lastShownAt >= REMIND_EVERY;
}

/** Whether this gain is worth interrupting the player for. */
export function worthReminding(input: {
  now: number;
  lastShownAt: number | null;
  lastBytes: number;
  reclaimable: number;
  freeBytes?: number;
}): boolean {
  const lowDisk = input.freeBytes !== undefined && input.freeBytes < LOW_DISK;
  if (input.reclaimable < (lowDisk ? WORTH_LOW_DISK : WORTH)) return false;
  if (input.lastShownAt === null) return true;
  const elapsed = input.now - input.lastShownAt;
  if (elapsed < REMIND_EVERY) return false;
  return elapsed >= SAME_GAIN_QUIET || input.reclaimable >= input.lastBytes + GROWTH;
}
