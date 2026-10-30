// Etkinlik's rows (PLUS-ACTIVITY), as the site groups them: a player's day in one row, an event
// without a player alone, the row's words from the most telling step of its day, the filters, the
// search, who waits for an answer and today's trades.
import { describe, expect, it } from "vitest";
import {
  type ActivityGroup,
  activityGroups,
  inFilter,
  outcomeOf,
  rowFound,
  tradesToday,
  waitingThreads,
} from "./activity";
import type { PlusAuditEntry, PlusInboxThread } from "./types";

let counter = 0;
function entry(k: string, w?: string, t = 1_000, i?: string): PlusAuditEntry {
  counter += 1;
  return {
    s: 1_000_000 + counter,
    t,
    k,
    ...(w ? { w } : {}),
    ...(i ? { i } : {}),
  } as PlusAuditEntry;
}

const first = (rows: ActivityGroup[]) => rows[0] as ActivityGroup;

describe("Etkinlik's rows", () => {
  it("keep a player's day together and an event without a player alone, newest first", () => {
    // Newest first, as the log reads them; names as the game compares them.
    const done = entry("trade_done", "KaraKartal_07", 9);
    const gate = entry("gate", undefined, 8);
    const reply = entry("reply", "KaraKartal_07", 7);
    const other = entry("pm_in", "TasUsta", 6);
    const opened = entry("pm_in", "karakartal_07", 5);
    const rows = activityGroups([done, gate, reply, other, opened]);
    expect(rows.map((row) => row.who)).toEqual(["KaraKartal_07", null, "TasUsta"]);
    expect(rows[0]?.entries).toEqual([done, reply, opened]);
    expect(rows[1]?.entries).toEqual([gate]);
  });

  it("say how the day went by its most telling step, the newest of those", () => {
    const rows = activityGroups([
      entry("reply", "KaraKartal_07"),
      entry("agree", "KaraKartal_07"),
      entry("offer", "KaraKartal_07"),
      entry("pm_in", "KaraKartal_07"),
    ]);
    expect(outcomeOf(first(rows)).k).toBe("agree");
    // A conversation alone says its newest line.
    const talk = activityGroups([entry("reply", "TasUsta"), entry("pm_in", "TasUsta")]);
    expect(outcomeOf(first(talk)).k).toBe("reply");
    // A trade done outranks the agreement before it, and a refusal a find.
    const day = activityGroups([entry("agree", "Neon_Drift"), entry("trade_done", "Neon_Drift")]);
    expect(outcomeOf(first(day)).k).toBe("trade_done");
    const refused = activityGroups([entry("found", "Pixel_Rook7"), entry("refuse", "Pixel_Rook7")]);
    expect(outcomeOf(first(refused)).k).toBe("refuse");
    // The bargain's last word is one rank, whoever said it: ProBot AI's newer offer, not the ask.
    const bargain = activityGroups([entry("offer", "Neon_Drift"), entry("counter", "Neon_Drift")]);
    expect(outcomeOf(first(bargain)).k).toBe("offer");
    // A delivery to the order menu says the day as a trade does.
    const menu = activityGroups([
      entry("menu_done", "KaraKartal_07"),
      entry("found", "KaraKartal_07"),
    ]);
    expect(outcomeOf(first(menu)).k).toBe("menu_done");
  });

  it("filter the site's groups: Retler holds what was turned down, not the assistant's pauses", () => {
    expect(inFilter("owner_no", "warnings")).toBe(true);
    expect(inFilter("handoff_no", "warnings")).toBe(true);
    expect(inFilter("refuse", "warnings")).toBe(true);
    expect(inFilter("paused", "warnings")).toBe(false);
    expect(inFilter("quota", "warnings")).toBe(false);
    // Yazışmalar: the conversation's lines only.
    expect(inFilter("pm_in", "messages")).toBe(true);
    expect(inFilter("offer", "messages")).toBe(false);
    // An entry may be in two groups, and Tümü shows a kind no group names.
    expect(inFilter("trade_fail", "deals") && inFilter("trade_fail", "warnings")).toBe(true);
    expect(inFilter("something_new", "all")).toBe(true);
    // DONUT-ORDER: a delivery is a deal, a failed one a deal and a refusal.
    expect(inFilter("menu_done", "deals")).toBe(true);
    expect(inFilter("menu_done", "warnings")).toBe(false);
    expect(inFilter("menu_fail", "deals") && inFilter("menu_fail", "warnings")).toBe(true);
  });

  it("find a row by the player or an item in either language, folded as players type", () => {
    const [row] = activityGroups([
      entry("found", "Ember_Fox42", 5, "minecraft:elytra"),
      entry("pm_in", "Ember_Fox42", 4),
    ]);
    // The item's names in both languages and its aliases.
    const names = (ref: string) =>
      ref === "minecraft:elytra" ? ["Elitra", "Elytra", "kanat"] : [];
    expect(rowFound(row as ActivityGroup, "", names)).toBe(true);
    expect(rowFound(row as ActivityGroup, "ember", names)).toBe(true);
    expect(rowFound(row as ActivityGroup, "elytra", names)).toBe(true);
    expect(rowFound(row as ActivityGroup, "ELİTRA ", names)).toBe(true);
    expect(rowFound(row as ActivityGroup, "kanat", names)).toBe(true);
    expect(rowFound(row as ActivityGroup, "zumrut", names)).toBe(false);
  });
});

function thread(name: string, extra: Partial<PlusInboxThread>): PlusInboxThread {
  return {
    name,
    lastAt: 1_000,
    last: { from: "them", text: "selam" },
    unread: 1,
    state: "active",
    handoffUntil: null,
    ...extra,
  };
}

describe("what waits for the player's answer", () => {
  it("is a player who wrote last, unread and not blocked", () => {
    const list = [
      thread("Ember_Fox42", {}),
      thread("Pixel_Rook7", { unread: 0 }),
      thread("Neon_Drift", { last: { from: "bot", text: "8.000 olur mu?" } }),
      thread("Dolandirici_X", { state: "blocked" }),
      thread("TasUsta", { state: "handoff", unread: 2 }),
    ];
    expect(waitingThreads(list).map((th) => th.name)).toEqual(["Ember_Fox42", "TasUsta"]);
  });
});

describe("the slim line's trades", () => {
  it("count today's finished trades, the player's handoffs too", () => {
    const now = new Date(2026, 9, 30, 15, 0, 0).getTime();
    const today = Math.floor(new Date(2026, 9, 30, 9, 30).getTime());
    const yesterday = Math.floor(new Date(2026, 9, 29, 22, 0).getTime());
    const log = [
      entry("trade_done", "KaraKartal_07", today),
      entry("handoff_done", "TasUsta", today),
      entry("trade_fail", "Neon_Drift", today),
      entry("trade_done", "Pixel_Rook7", yesterday),
    ];
    expect(tradesToday(log, now)).toBe(2);
  });
});
