import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { appliesTo, reasonKey, tally } from "./bulk";
import {
  type KindLine,
  kindTone,
  lineKind,
  matchesFilter,
  mentions,
  splitAtName,
  visibleLines,
} from "./console-kinds";
import { reduce, toCursor, toInvItem, WAITING } from "./inventory/model";

const kinds = JSON.parse(
  readFileSync(
    new URL(
      "../../../../../../crates/pl-cloud-afk/tests/vectors/console-kinds-v1.json",
      import.meta.url,
    ),
    "utf8",
  ),
) as {
  kinds: string[];
  cases: Array<{ name: string; expected: { c?: string; who?: string; log?: boolean } }>;
};

const line = (k: string, c: string | null, text: string, who: string | null = null): KindLine => ({
  at: 1,
  k,
  c,
  text,
  who,
  goto: null,
});

describe("console kinds", () => {
  it("files every logged kind of the contract under a chip", () => {
    for (const kind of kinds.kinds.filter((k) => k !== "bar")) {
      const shown = (["chat", "pm", "server", "link"] as const).some((filter) =>
        matchesFilter(line("n", kind, "x"), filter, "Ahmet_AFK"),
      );
      expect(shown, kind).toBe(true);
    }
    for (const { name, expected } of kinds.cases) {
      if (expected.log && expected.c)
        expect(lineKind(line("n", expected.c, "x")), name).not.toBe("console");
    }
  });

  it("keeps lines from before kinds under Hepsi only", () => {
    const old = line("s", null, "/is home");
    expect(lineKind(old)).toBe("console");
    expect(matchesFilter(old, "all", "a")).toBe(true);
    expect(matchesFilter(old, "mine", "a")).toBe(false);
    expect(kindTone(old)).toBe("text-gold-300/80");
    expect(lineKind(line("s", "sys", "/spawn"))).toBe("command");
    expect(lineKind(line("n", "later", "x"))).toBe("cli");
  });

  it("finds mentions by someone else, ASCII letter case aside", () => {
    expect(mentions(line("n", "chat", "kerem » AHMET_afk gel", "Kerem"), "Ahmet_AFK")).toBe(true);
    expect(mentions(line("n", "pm", "✉ «« Ahmet_AFK: selam", "Ahmet_AFK"), "Ahmet_AFK")).toBe(
      false,
    );
    expect(mentions(line("n", "sys", "Ahmet_AFK katıldı"), "Ahmet_AFK")).toBe(false);
  });

  it("filters by chip and search together", () => {
    const lines = [
      line("n", "chat", "Ayşe » selam", "Ayşe"),
      line("n", "pm", "✉ »» Mert: pvp?", "Mert"),
      line("n", "sys", "[Duyuru] Etkinlik"),
      line("e", "err", "Bağlantı yavaşladı"),
    ];
    expect(visibleLines(lines, "pm", "x", "")).toHaveLength(1);
    expect(visibleLines(lines, "all", "x", "ETKİNLİK")).toHaveLength(1);
    expect(visibleLines(lines, "link", "x", "")).toHaveLength(1);
    expect(splitAtName(lines[1] as KindLine)).toEqual(["✉ »» ", "Mert", ": pvp?"]);
  });
});

describe("bulk results", () => {
  it("say the first failure's reason in the contract's words", () => {
    expect(
      tally([{ ok: true }, { ok: false, code: "BULUT-SURE" }, { ok: false, code: "BULUT" }]),
    ).toEqual({ done: 1, failed: 2, reason: "expired" });
    expect(reasonKey("BULUT-ASKIDA")).toBe("staff");
    expect(reasonKey("here")).toBe("here");
    expect(reasonKey(null)).toBe("other");
  });

  it("act only on the accounts an action applies to", () => {
    const account = (desired: string, hold: unknown = null, expired = false) => ({
      desired,
      hold,
      expired,
    });
    expect(appliesTo("start", account("run"))).toBe(false);
    expect(appliesTo("start", account("stop"))).toBe(true);
    expect(appliesTo("start", account("run", null, true))).toBe(true);
    expect(appliesTo("stop", account("stop"))).toBe(false);
    expect(appliesTo("restart", account("run", { kind: "cause" }))).toBe(false);
    expect(appliesTo("restart", account("run"))).toBe(true);
  });
});

describe("inventory frames", () => {
  const item = {
    slot: 36,
    id: "minecraft:diamond_sword",
    count: 1,
    label: { tr: "Elmas Kılıç", en: "Diamond Sword" },
    name: null,
    lore: [],
    durability: { left: 1450, max: 1561 },
    cmd: null,
    model: null,
    ench: [],
    enchanted: true,
    inside: null,
    potion: null,
    color: null,
    partial: false,
  };

  it("become the view's own items, empty parts left out", () => {
    expect(toInvItem(item)).toEqual({
      slot: 36,
      id: "minecraft:diamond_sword",
      count: 1,
      label: { tr: "Elmas Kılıç", en: "Diamond Sword" },
      durability: { left: 1450, max: 1561 },
      enchanted: true,
    });
    expect(toCursor(null)).toBeNull();
    expect(toCursor(item)).not.toHaveProperty("slot");
  });

  it("follow the machine's state and windows", () => {
    let state = reduce(
      WAITING,
      { kind: "invState", acct: "a", state: "ok", reason: null, lock: "trade" },
      "a",
    );
    expect(state).toMatchObject({ status: "ok", locked: true });
    state = reduce(
      state,
      { kind: "inventory", acct: "a", held: 2, cursor: null, items: [item], snap: true },
      "a",
    );
    expect(state.inventory?.held).toBe(2);
    const open = { type: "generic_9x3", title: "Kitler", size: 27 };
    state = reduce(
      state,
      { kind: "window", acct: "a", wseq: 7, open, items: [], closed: false },
      "a",
    );
    expect(state.window?.wseq).toBe(7);
    // A close for an older opening leaves the newer one.
    state = reduce(
      state,
      { kind: "window", acct: "a", wseq: 6, open: null, items: [], closed: true },
      "a",
    );
    expect(state.window?.wseq).toBe(7);
    state = reduce(
      state,
      { kind: "window", acct: "a", wseq: 7, open: null, items: [], closed: true },
      "a",
    );
    expect(state.window).toBeNull();
    // Another account's frames change nothing.
    expect(
      reduce(
        state,
        { kind: "invState", acct: "b", state: "off", reason: "setting", lock: null },
        "a",
      ),
    ).toBe(state);
    expect(
      reduce(
        state,
        { kind: "invState", acct: "a", state: "off", reason: "restart", lock: null },
        "a",
      ),
    ).toMatchObject({ status: "off", reason: "restart", inventory: null });
  });
});
