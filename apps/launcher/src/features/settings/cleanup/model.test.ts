import { describe, expect, it } from "vitest";
import type { Leftover, OldSet, SharedFiles } from "@/lib/api";
import {
  ageLabel,
  groupBytes,
  groupState,
  mayCheckReminder,
  planOf,
  REMIND_EVERY,
  rowsOf,
  SAME_GAIN_QUIET,
  selectedBytes,
  setRowId,
  sharedRowId,
  staleSets,
  toggleGroup,
  toggleRow,
  worthReminding,
} from "./model";

const DAY = 24 * 60 * 60 * 1000;
const MB = 1024 * 1024;
const GB = 1024 * MB;
const NOW = Date.parse("2026-10-16T12:00:00Z");

const set = (minecraft: string, daysAgo: number, sizeBytes: number): OldSet => ({
  key: `projects/${minecraft}`,
  instanceId: "projects",
  instanceName: "Projects",
  minecraft,
  lastUsedAt: new Date(NOW - daysAgo * DAY).toISOString(),
  sizeBytes,
});

const sets = [set("1.19.4", 200, 300), set("1.20.1", 70, 500), set("1.21.1", 35, 100)];
const shared: SharedFiles = {
  items: [
    { id: "version:1.21.1", kind: "gameVersion", name: "1.21.1", bytes: 40, files: 2 },
    {
      id: "version:fabric-loader-0.16.5-1.21.1",
      kind: "loaderProfile",
      name: "fabric-loader-0.16.5-1.21.1",
      bytes: 5,
      files: 2,
    },
    { id: "libraries", kind: "libraries", name: "", bytes: 70, files: 9 },
  ],
  // The game jar is hard-linked into both versions: freed only with both.
  groups: [{ items: ["version:1.21.1", "version:fabric-loader-0.16.5-1.21.1"], bytes: 1000 }],
  totalBytes: 1115,
};
const leftovers: Leftover[] = [
  { kind: "cache", bytes: 8, files: 3 },
  { kind: "logs", bytes: 2, files: 1 },
];

describe("selection", () => {
  it("filters sets by the chosen threshold", () => {
    expect(staleSets(sets, 30, NOW).map((s) => s.minecraft)).toEqual([
      "1.19.4",
      "1.20.1",
      "1.21.1",
    ]);
    expect(staleSets(sets, 60, NOW).map((s) => s.minecraft)).toEqual(["1.19.4", "1.20.1"]);
    expect(staleSets(sets, 90, NOW).map((s) => s.minecraft)).toEqual(["1.19.4"]);
    expect(staleSets([{ ...sets[0], lastUsedAt: undefined } as OldSet], 30, NOW)).toEqual([]);
  });

  it("starts with everything on and toggles groups as a whole", () => {
    const rows = rowsOf(sets, shared, leftovers);
    let off = new Set<string>();
    expect(groupState(rows, "sets", off)).toBe("all");
    off = toggleRow(off, setRowId(sets[1] as OldSet));
    expect(groupState(rows, "sets", off)).toBe("some");
    // A partly selected group turns fully on, then fully off.
    off = toggleGroup(rows, "sets", off);
    expect(groupState(rows, "sets", off)).toBe("all");
    off = toggleGroup(rows, "sets", off);
    expect(groupState(rows, "sets", off)).toBe("none");
    expect(groupState(rows, "shared", off)).toBe("all");
  });

  it("counts shared files once every holder is selected", () => {
    const rows = rowsOf(sets, shared, leftovers);
    const all = selectedBytes(rows, new Set(), shared.groups);
    expect(all).toBe(900 + 115 + 1000 + 10);
    const oneHolder = toggleRow(new Set(), sharedRowId("version:fabric-loader-0.16.5-1.21.1"));
    expect(selectedBytes(rows, oneHolder, shared.groups)).toBe(900 + 110 + 10);
    expect(groupBytes(rows, "shared", shared.groups)).toBe(1115);
    expect(groupBytes(rows, "leftovers")).toBe(10);
  });

  it("plans what each step cleans", () => {
    const off = new Set([setRowId(sets[0] as OldSet), "leftover:logs", sharedRowId("libraries")]);
    const plan = planOf(sets, shared, leftovers, off);
    expect(plan.sets.map((s) => s.minecraft)).toEqual(["1.20.1", "1.21.1"]);
    expect(plan.sharedIds).toEqual(["version:1.21.1", "version:fabric-loader-0.16.5-1.21.1"]);
    expect(plan.leftoverKinds).toEqual(["cache"]);
    expect(plan.setBytes).toBe(600);
    expect(plan.sharedBytes).toBe(1045);
    expect(plan.leftoverBytes).toBe(8);
    expect(plan.totalBytes).toBe(1653);
  });

  it("says how long ago in words", () => {
    expect(ageLabel(new Date(NOW - 1 * DAY).toISOString(), NOW)).toBe("dün");
    expect(ageLabel(new Date(NOW - 12 * DAY).toISOString(), NOW)).toBe("12 gün önce");
    expect(ageLabel(new Date(NOW - 38 * DAY).toISOString(), NOW)).toBe("geçen ay");
    expect(ageLabel(new Date(NOW - 143 * DAY).toISOString(), NOW)).toBe("5 ay önce");
    expect(ageLabel(new Date(NOW - 400 * DAY).toISOString(), NOW)).toBe("geçen yıl");
  });
});

describe("reminder", () => {
  const base = { now: NOW, lastShownAt: null, lastBytes: 0 };

  it("speaks up only for a real gain", () => {
    expect(worthReminding({ ...base, reclaimable: 1.5 * GB, freeBytes: 200 * GB })).toBe(false);
    expect(worthReminding({ ...base, reclaimable: 2.5 * GB, freeBytes: 200 * GB })).toBe(true);
    // An almost full disk makes a smaller gain worth it.
    expect(worthReminding({ ...base, reclaimable: 700 * MB, freeBytes: 6 * GB })).toBe(true);
    expect(worthReminding({ ...base, reclaimable: 300 * MB, freeBytes: 6 * GB })).toBe(false);
  });

  it("does not nag", () => {
    const shown = { now: NOW, lastBytes: 3 * GB, reclaimable: 3.2 * GB, freeBytes: 200 * GB };
    expect(worthReminding({ ...shown, lastShownAt: NOW - 3 * DAY })).toBe(false);
    // Two weeks later the same gain still waits...
    expect(worthReminding({ ...shown, lastShownAt: NOW - REMIND_EVERY - DAY })).toBe(false);
    // ...unless it grew a lot, or a long time passed.
    expect(
      worthReminding({ ...shown, reclaimable: 4.5 * GB, lastShownAt: NOW - REMIND_EVERY - DAY }),
    ).toBe(true);
    expect(worthReminding({ ...shown, lastShownAt: NOW - SAME_GAIN_QUIET - DAY })).toBe(true);
  });

  it("stays quiet in do-not-disturb, during games and before the welcome flow", () => {
    const ready = { now: NOW, lastShownAt: null, onboarded: true, quiet: false };
    expect(mayCheckReminder(ready)).toBe(true);
    expect(mayCheckReminder({ ...ready, quiet: true })).toBe(false);
    expect(mayCheckReminder({ ...ready, onboarded: false })).toBe(false);
    expect(mayCheckReminder({ ...ready, lastShownAt: NOW - 2 * DAY })).toBe(false);
  });
});
