import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import type { CloudFishingSetting, CloudLiveEvent, CloudMiningSetting } from "@/bindings";
import { enCloudAfkAuto } from "@/locales/en-cloud-afk-auto";
import { trCloudAfkAuto } from "@/locales/tr-cloud-afk-auto";
import type { CloudAccount } from "../api";
import {
  AIM_DIRECTIONS,
  AIM_TILTS,
  AUTO_WAITING,
  autoShown,
  canPause,
  chosenKinds,
  defaultFishing,
  defaultMining,
  type FishState,
  fishingOf,
  kindOn,
  limitKey,
  MINING_BLOCKS,
  MINING_KINDS,
  type MineState,
  miningOf,
  needsResume,
  ORE_KINDS,
  ORES,
  offKey,
  orderedBlocks,
  reduceAuto,
  refusalKey,
  STONE_KINDS,
  STONES,
  slotKey,
  stateLine,
  toggleKind,
  wear,
  withAuto,
} from "./model";

const fish = (over: Partial<FishState> = {}): FishState => ({
  state: "on",
  reason: null,
  since: 1,
  doing: "wait",
  rod: { item: "minecraft:fishing_rod", dur: 40, max: 64 },
  today: { catches: 3, fish: 2, treasure: 1, junk: 0, perHour: 9, items: [], last: [] },
  ...over,
});

const mine = (over: Partial<MineState> = {}): MineState => ({
  state: "on",
  reason: null,
  since: 1,
  target: null,
  tool: null,
  today: { blocks: 10, perMinute: 12, items: [] },
  ...over,
});

/** The contract's shared cases (pl-cloud-afk's copy of FISHMINE-W's auto-op-v1.json). */
const vectors = JSON.parse(
  readFileSync(
    new URL(
      "../../../../../../../crates/pl-cloud-afk/tests/vectors/auto-op-v1.json",
      import.meta.url,
    ),
    "utf8",
  ),
);

describe("the contract's shared cases", () => {
  it("keep the block list, the defaults and every reason the panels word", () => {
    expect([...MINING_BLOCKS]).toEqual(vectors.settings.fixed.miningBlocks);
    const defaults = vectors.settings.configDefaults;
    expect(defaultFishing()).toEqual({
      enabled: false,
      rod: defaults.tool,
      full: { ...defaults.full, command: null },
      look: defaults.look,
    });
    expect(defaultMining()).toEqual({
      enabled: false,
      blocks: defaults.blocks,
      tool: defaults.tool,
      full: { ...defaults.full, command: null },
      idle: defaults.idle,
    });
    for (const reason of vectors.settings.autores.autoOffReasons as string[]) {
      for (const mode of ["fishing", "mining"] as const) {
        expect(trCloudAfkAuto[mode], `${mode}/${reason}`).toHaveProperty(offKey(reason));
      }
    }
  });

  it("read every stored setting whole, as the Worker stores it", () => {
    for (const each of vectors.config as Array<{
      name: string;
      mode: "fishing" | "mining";
      value: unknown;
      expected: { stored?: Record<string, unknown> };
    }>) {
      if (!each.expected.stored) continue;
      const read =
        each.mode === "fishing"
          ? fishingOf(each.value as Parameters<typeof fishingOf>[0])
          : miningOf(each.value as Parameters<typeof miningOf>[0]);
      if (!read) throw new Error(each.name);
      const { paused: _paused, command, ...full } = read.full;
      expect({ ...read, full: command ? { ...full, command } : full }, each.name).toEqual(
        each.expected.stored,
      );
    }
  });
});

describe("auto model", () => {
  it("merges each mode of an autost and leaves the other as it was", () => {
    const first = reduceAuto(
      AUTO_WAITING,
      { kind: "autoState", acct: "a_1", fishing: fish(), mining: mine() } as CloudLiveEvent,
      "a_1",
    );
    expect(first.fishing?.today.catches).toBe(3);
    const next = reduceAuto(
      first,
      {
        kind: "autoState",
        acct: "a_1",
        fishing: null,
        mining: mine({ state: "paused", reason: "moved" }),
      } as CloudLiveEvent,
      "a_1",
    );
    expect(next.fishing).toBe(first.fishing);
    expect(next.mining?.reason).toBe("moved");
    // Another account's frame changes nothing.
    expect(
      reduceAuto(
        next,
        { kind: "autoState", acct: "a_2", fishing: fish(), mining: null } as CloudLiveEvent,
        "a_1",
      ),
    ).toBe(next);
  });

  it("words what a mode does, its pauses and its waits", () => {
    expect(stateLine("fishing", fish({ doing: "reel" }))).toEqual({
      key: "fishing.doing.reel",
      tone: "active",
    });
    expect(stateLine("fishing", fish({ doing: "later" }))?.key).toBe("fishing.doing.on");
    expect(stateLine("mining", mine())).toEqual({ key: "mining.doing.on", tone: "active" });
    expect(stateLine("mining", mine({ state: "waiting", reason: "no_target" }))?.key).toBe(
      "mining.noTarget",
    );
    expect(stateLine("fishing", fish({ state: "waiting", reason: "full" }))?.key).toBe(
      "inventoryFull",
    );
    expect(stateLine("fishing", fish({ state: "paused", reason: "moved" }))?.key).toBe(
      "paused.moved",
    );
    expect(stateLine("fishing", null)).toBeNull();
    // Every reason has its own words; one a newer machine sends reads plainly, never as the player's.
    expect(stateLine("fishing", fish({ state: "paused", reason: "player" }))?.key).toBe(
      "paused.player",
    );
    expect(stateLine("fishing", fish({ state: "paused", reason: "later" }))?.key).toBe(
      "pausedOther",
    );
    expect(stateLine("mining", mine({ state: "waiting", reason: "later" }))?.key).toBe(
      "waitingOther",
    );
    // A wait is the mode's own: a rod's never reads on mining.
    expect(stateLine("mining", mine({ state: "waiting", reason: "no_rod" }))?.key).toBe(
      "waitingOther",
    );
    expect(needsResume(fish({ state: "paused", reason: "player" }))).toBe(true);
    expect(needsResume(fish({ state: "paused", reason: "window" }))).toBe(false);
    expect(canPause(fish({ state: "waiting", reason: "no_water" }))).toBe(true);
    expect(canPause(fish({ state: "paused", reason: "moved" }))).toBe(false);
  });

  it("names why a mode is off, why its switch waits and a refusal", () => {
    expect(offKey("slots")).toBe("slots");
    expect(offKey("limited")).toBe("limited");
    // An auto_off reason this launcher does not know reads as Studio's switch (the contract's rule).
    expect(offKey("service")).toBe("off");
    expect(offKey("later")).toBe("off");
    expect(slotKey("host")).toEqual({ key: "noSlot" });
    expect(slotKey("player")).toEqual({ key: "elsewhere" });
    // A refused save names the player's own limit (none sent reads as one, as on the site).
    expect(limitKey("player", 3)).toEqual({ key: "maxAccounts", values: { max: 3 } });
    expect(limitKey("player", 1)).toEqual({ key: "oneAccount" });
    expect(limitKey("player", null)).toEqual({ key: "oneAccount" });
    expect(limitKey("host", 10)).toEqual({ key: "noSlot" });
    expect(refusalKey("fishing", { code: "busy_trade", reason: null, field: null }, true)).toBe(
      "refusal.turnTrade",
    );
    expect(refusalKey("fishing", { code: "busy_trade", reason: null, field: null })).toBe(
      "paused.trade",
    );
    expect(refusalKey("mining", { code: "not_running", reason: null, field: null })).toBe(
      "refusal.notRunning",
    );
    expect(refusalKey("mining", { code: "auto_off", reason: "version", field: null })).toBe(
      "mining.version",
    );
    expect(refusalKey("fishing", { code: "rate_limited", reason: null, field: null })).toBe(
      "refusal.slow",
    );
    expect(refusalKey("fishing", { code: "invalid", reason: "yaw", field: null })).toBeNull();
  });

  it("words every state, refusal and limit in both languages", () => {
    const at = (words: unknown, key: string) =>
      key
        .split(".")
        .reduce<unknown>(
          (node, part) => (node as Record<string, unknown> | undefined)?.[part],
          words,
        );
    const keys = new Set<string>();
    const add = (key: string | null | undefined) => {
      if (key) keys.add(key);
    };
    const reasons = {
      paused: ["player", "moved", "window", "trade", "later"],
      waiting: [
        "full",
        "no_rod",
        "rod_low",
        "no_water",
        "no_tool",
        "tool_low",
        "no_target",
        "other",
        "later",
      ],
    };
    for (const mode of ["fishing", "mining"] as const) {
      const make = (state: string, reason: string | null) =>
        mode === "fishing" ? fish({ state, reason }) : mine({ state, reason });
      add(stateLine(mode, make("on", null))?.key);
      for (const doing of ["cast", "wait", "reel", "later"]) {
        add(stateLine("fishing", fish({ doing }))?.key);
      }
      for (const [state, list] of Object.entries(reasons)) {
        for (const reason of list) add(stateLine(mode, make(state, reason))?.key);
      }
      for (const reason of ["not_running", "setting", "restart", "slots", "version", "limited"]) {
        add(`${mode}.${offKey(reason)}`);
      }
      add(`${mode}.${offKey("later")}`);
      for (const [scope, max] of [
        ["player", 3],
        ["player", 1],
        ["player", null],
        ["host", 10],
      ] as const) {
        add(`${mode}.${limitKey(scope, max).key}`);
      }
      for (const scope of ["player", "host"]) add(`${mode}.${slotKey(scope).key}`);
      for (const code of [
        "invalid",
        "rate_limited",
        "busy_trade",
        "not_running",
        "auto_off",
        "host_offline",
        "later",
      ]) {
        for (const aim of [false, true]) {
          add(refusalKey(mode, { code, reason: "later", field: null }, aim));
        }
      }
    }
    expect(keys.size).toBeGreaterThan(30);
    for (const key of keys) {
      expect(typeof at(trCloudAfkAuto, key), key).toBe("string");
      expect(typeof at(enCloudAfkAuto, key), key).toBe("string");
    }
  });

  it("counts the kinds the site counts: an ore and its deepslate twin are one", () => {
    expect(MINING_KINDS.flat()).toEqual([...MINING_BLOCKS]);
    expect(chosenKinds([...MINING_BLOCKS])).toBe(25);
    expect(chosenKinds(["minecraft:iron_ore", "minecraft:deepslate_iron_ore"])).toBe(1);
    expect(chosenKinds(["minecraft:cobblestone", "minecraft:stone"])).toBe(2);
    expect(chosenKinds([])).toBe(0);
  });

  it("toggles a kind as the site does: an ore and its twin go together", () => {
    const iron = ["minecraft:iron_ore", "minecraft:deepslate_iron_ore"];
    expect(STONE_KINDS).toHaveLength(14);
    expect(ORE_KINDS).toHaveLength(11);
    expect(toggleKind(["minecraft:diamond_ore"], iron)).toEqual([
      "minecraft:iron_ore",
      "minecraft:deepslate_iron_ore",
      "minecraft:diamond_ore",
    ]);
    // A stored half pair reads as chosen; pressing it takes the half away.
    expect(kindOn(["minecraft:deepslate_iron_ore"], iron)).toBe(true);
    expect(toggleKind(["minecraft:cobblestone", "minecraft:deepslate_iron_ore"], iron)).toEqual([
      "minecraft:cobblestone",
    ]);
    expect(kindOn(["minecraft:cobblestone"], iron)).toBe(false);
  });

  it("reads a stored setting with the contract's defaults", () => {
    expect(fishingOf({ enabled: true })).toEqual({ ...defaultFishing(), enabled: true });
    expect(miningOf({ enabled: true, idle: { jump: true } })?.idle).toEqual({
      ...defaultMining().idle,
      jump: true,
    });
    expect(fishingOf(null)).toBeNull();
  });

  it("keeps the contract's block list and order", () => {
    expect(MINING_BLOCKS).toHaveLength(33);
    expect(new Set(MINING_BLOCKS).size).toBe(33);
    expect([...STONES, ...ORES]).toEqual([...MINING_BLOCKS]);
    expect(
      ORES.every((block) => block.endsWith("_ore") || block === "minecraft:ancient_debris"),
    ).toBe(true);
    expect(orderedBlocks(["minecraft:diamond_ore", "minecraft:cobblestone"])).toEqual([
      "minecraft:cobblestone",
      "minecraft:diamond_ore",
    ]);
  });

  it("aims as the game counts and wears as the game draws", () => {
    expect(AIM_DIRECTIONS).toEqual({ north: 180, east: -90, south: 0, west: 90 });
    expect(AIM_TILTS).toEqual({ up: -45, level: 0, down: 45 });
    expect(wear(64, 64)).toEqual({ fill: 1, color: "hsl(120 100% 50%)" });
    expect(wear(0, 64).color).toBe("hsl(0 100% 50%)");
  });

  it("does one job at a time: turning a mode on turns the other off", () => {
    const fishing = { ...defaultFishing(), enabled: true };
    const start: { fishing: CloudFishingSetting | null; mining: CloudMiningSetting | null } = {
      fishing,
      mining: null,
    };
    const both = withAuto(start, "mining", { ...defaultMining(), enabled: true });
    expect(both.fishing?.enabled).toBe(false);
    expect(both.mining?.enabled).toBe(true);
    // Turning one off, or changing a part, leaves the other as it is.
    const off = withAuto(start, "mining", defaultMining());
    expect(off.fishing).toBe(fishing);
    expect(stateLine("mining", mine({ state: "waiting", reason: "other" }))?.key).toBe(
      "mining.otherFirst",
    );
  });

  it("shows a mode's tab only while it may run there", () => {
    const account = (fishing: object | null) =>
      ({ host: "kz1", features: { fishing } }) as unknown as CloudAccount;
    expect(autoShown(account({ mode: "on", on: true, slot: null, limited: null }), "fishing")).toBe(
      true,
    );
    expect(
      autoShown(
        account({ mode: "on", on: true, slot: null, limited: { tr: "x", en: "x" } }),
        "fishing",
      ),
    ).toBe(false);
    expect(autoShown(account({ mode: "off", on: true }), "fishing")).toBe(false);
    expect(autoShown(account(null), "fishing")).toBe(false);
  });
});
