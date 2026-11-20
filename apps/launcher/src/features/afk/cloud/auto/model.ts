// Auto-fishing and auto-mining of a cloud account (FISHMINE, services/cloud-afk README "Auto-fishing and
// auto-mining"): the settings' defaults and blocks, what the "Balık" and "Kazma" panels hold from the
// live frames (`autost`), and how they word the contract's states and refusals. Pure, so the panels,
// the settings and their tests share it.
import type {
  CloudAccount_Serialize,
  CloudAutoError,
  CloudFishingSetting,
  CloudFishState_Serialize,
  CloudLiveEvent,
  CloudMineState_Serialize,
  CloudMiningSetting,
  CloudModPaused,
} from "@/bindings";

export type AutoMode = "fishing" | "mining";
export const AUTO_MODES: readonly AutoMode[] = ["fishing", "mining"];
export type FishState = CloudFishState_Serialize;
export type MineState = CloudMineState_Serialize;

/** The contract's `AutoOp`, as the panels send it (Rust checks its shape again). */
export type AutoOp =
  | { mode: AutoMode; action: "pause" | "resume" }
  | { action: "aim"; yaw: number; pitch: number };

export type AutoResult = { ok: true } | { ok: false; error: CloudAutoError };

/**
 * The blocks mining may break (`fixed.miningBlocks`), in the contract's order: the stone kinds
 * first, then the ores. The same list as pl-cloud-afk's `MINING_BLOCKS`.
 */
export const MINING_BLOCKS = [
  "minecraft:cobblestone",
  "minecraft:stone",
  "minecraft:cobbled_deepslate",
  "minecraft:deepslate",
  "minecraft:andesite",
  "minecraft:diorite",
  "minecraft:granite",
  "minecraft:tuff",
  "minecraft:basalt",
  "minecraft:smooth_basalt",
  "minecraft:blackstone",
  "minecraft:netherrack",
  "minecraft:end_stone",
  "minecraft:obsidian",
  "minecraft:coal_ore",
  "minecraft:deepslate_coal_ore",
  "minecraft:copper_ore",
  "minecraft:deepslate_copper_ore",
  "minecraft:iron_ore",
  "minecraft:deepslate_iron_ore",
  "minecraft:gold_ore",
  "minecraft:deepslate_gold_ore",
  "minecraft:redstone_ore",
  "minecraft:deepslate_redstone_ore",
  "minecraft:lapis_ore",
  "minecraft:deepslate_lapis_ore",
  "minecraft:diamond_ore",
  "minecraft:deepslate_diamond_ore",
  "minecraft:emerald_ore",
  "minecraft:deepslate_emerald_ore",
  "minecraft:nether_gold_ore",
  "minecraft:nether_quartz_ore",
  "minecraft:ancient_debris",
] as const;
/** The picker's two groups: the list's stone kinds, then its ores. */
export const STONES: readonly string[] = MINING_BLOCKS.slice(0, 14);
export const ORES: readonly string[] = MINING_BLOCKS.slice(14);

/**
 * The kinds the mining answer counts, as the site's editor offers them (afk-web `MINING_CHOICES`):
 * each stone kind alone, each ore with its deepslate twin, the Nether's ores alone. The same stored
 * blocks read the same sentence on both ("Önüne gelen {n} tür bloğu kırar").
 */
export const MINING_KINDS: readonly (readonly string[])[] = [
  ...STONES.map((id) => [id]),
  ...["coal", "copper", "iron", "gold", "redstone", "lapis", "diamond", "emerald"].map((ore) => [
    `minecraft:${ore}_ore`,
    `minecraft:deepslate_${ore}_ore`,
  ]),
  ...["minecraft:nether_gold_ore", "minecraft:nether_quartz_ore", "minecraft:ancient_debris"].map(
    (id) => [id],
  ),
];

/** How many kinds the chosen blocks make (an ore and its twin are one), as the site counts. */
export function chosenKinds(blocks: readonly string[]): number {
  return MINING_KINDS.filter((ids) => ids.some((id) => blocks.includes(id))).length;
}

export const KEEP_MIN = 1;
export const KEEP_MAX = 100;
export const IDLE_MIN_SECS = 20;
export const IDLE_MAX_SECS = 3600;

/** A rod's or a pickaxe's care: put away at `keep` durability left; `switch` takes a spare. */
export interface ToolCare {
  switch: boolean;
  keep: number;
}

/**
 * What a full inventory does: `wait`, `keep` or `command` (with its words); `paused`: the view's mark
 * that a moderation case paused the command (it waits meanwhile; never sent back).
 */
export interface WhenFull {
  do: string;
  command: string | null;
  paused?: CloudModPaused | null;
}

export interface IdleMoves {
  look: boolean;
  sneak: boolean;
  jump: boolean;
  step: boolean;
  every: { min: number; max: number };
}

/** `config.fishing` with every part (the binding's fields are optional: serde fills them). */
export interface FishingSetting {
  enabled: boolean;
  rod: ToolCare;
  full: WhenFull;
  look: boolean;
}

export interface MiningSetting {
  enabled: boolean;
  /** null: every block of the list. */
  blocks: string[] | null;
  tool: ToolCare;
  full: WhenFull;
  idle: IdleMoves;
}

/** The contract's defaults (`CloudAccountConfig` "Auto settings"): what a first switch-on stores. */
export function defaultFishing(): FishingSetting {
  return {
    enabled: false,
    rod: { switch: true, keep: 5 },
    full: { do: "wait", command: null },
    look: false,
  };
}

export function defaultMining(): MiningSetting {
  return {
    enabled: false,
    blocks: null,
    tool: { switch: true, keep: 5 },
    full: { do: "wait", command: null },
    idle: { look: false, sneak: false, jump: false, step: false, every: { min: 60, max: 180 } },
  };
}

/** A stored or drafted fishing setting with every part (the defaults for what is missing). */
export function fishingOf(setting: CloudFishingSetting | null | undefined): FishingSetting | null {
  if (!setting) return null;
  const base = defaultFishing();
  return {
    enabled: setting.enabled ?? false,
    rod: { ...base.rod, ...setting.rod },
    full: { ...base.full, ...setting.full },
    look: setting.look ?? false,
  };
}

export function miningOf(setting: CloudMiningSetting | null | undefined): MiningSetting | null {
  if (!setting) return null;
  const base = defaultMining();
  return {
    enabled: setting.enabled ?? false,
    blocks: setting.blocks ?? null,
    tool: { ...base.tool, ...setting.tool },
    full: { ...base.full, ...setting.full },
    idle: {
      ...base.idle,
      ...setting.idle,
      every: { ...base.idle.every, ...setting.idle?.every },
    },
  };
}

/**
 * A config with one mode's setting written: turning a mode on turns the other off in the same draft
 * (an account does one job at a time; the Worker refuses the second, `invalid_config` "other").
 */
export function withAuto<
  C extends { fishing?: CloudFishingSetting | null; mining?: CloudMiningSetting | null },
>(config: C, mode: AutoMode, next: FishingSetting | MiningSetting): C {
  const other: AutoMode = mode === "fishing" ? "mining" : "fishing";
  const otherSetting = other === "fishing" ? fishingOf(config.fishing) : miningOf(config.mining);
  return {
    ...config,
    [mode]: next,
    ...(next.enabled && otherSetting?.enabled
      ? { [other]: { ...otherSetting, enabled: false } }
      : {}),
  };
}

/** A chosen subset in the list's order (the picker's own order, whatever the clicks). */
export function orderedBlocks(chosen: readonly string[]): string[] {
  return MINING_BLOCKS.filter((block) => chosen.includes(block));
}

/** Studio offers the mode for the account (its settings block may show). */
export function autoOffered(account: CloudAccount_Serialize | undefined, mode: AutoMode): boolean {
  return account?.features?.[mode]?.mode === "on";
}

/**
 * Whether the account shows the mode's tab and card item: offered, switched on, with a machine, and
 * its server not limited for the mode (the settings say why there).
 */
export function autoShown(account: CloudAccount_Serialize | undefined, mode: AutoMode): boolean {
  const feature = account?.features?.[mode];
  return Boolean(account?.host && feature?.mode === "on" && feature.on && !feature.limited);
}

export interface AutoState {
  fishing: FishState | null;
  mining: MineState | null;
}

export const AUTO_WAITING: AutoState = { fishing: null, mining: null };

/**
 * The panels' state after one live event of account `acct`: an `autost` replaces the modes it
 * names and leaves the other as it was (others leave it all as it is).
 */
export function reduceAuto(state: AutoState, event: CloudLiveEvent, acct: string): AutoState {
  if (event.kind !== "autoState" || event.acct !== acct) return state;
  // Rust always writes the whole state: the serialized shape.
  const fishing = (event.fishing ?? null) as FishState | null;
  const mining = (event.mining ?? null) as MineState | null;
  if (!fishing && !mining) return state;
  return { fishing: fishing ?? state.fishing, mining: mining ?? state.mining };
}

export type WordKey = { key: string; values?: Record<string, string | number> };

/** Why a mode does not run (`state` `off`, or `auto_off`'s reason); `cloudAfkAuto.<mode>.<key>`. */
export function offKey(reason: string | null | undefined): string {
  switch (reason) {
    case "not_running":
      return "notRunning";
    case "setting":
      return "setting";
    case "restart":
      return "restart";
    case "slots":
      return "slots";
    case "version":
      return "version";
    case "limited":
      return "limited";
    default:
      return "off";
  }
}

/**
 * Why the switch cannot be turned on now (`features.<mode>.slot`): the player's other accounts hold
 * it, or the machine has no room. Keys under `cloudAfkAuto.<mode>`, the site's words.
 */
export function slotKey(scope: string | null | undefined): WordKey {
  return scope === "player" ? { key: "elsewhere" } : { key: "noSlot" };
}

/**
 * A save refused for want of a slot (`limit_<mode>`): the player's own limit names its number (none
 * sent reads as one, as on the site), the machine's has none. Keys under `cloudAfkAuto.<mode>`.
 */
export function limitKey(
  scope: string | null | undefined,
  max: number | null | undefined,
): WordKey {
  if (scope !== "player") return { key: "noSlot" };
  const n = max ?? 1;
  return n > 1 ? { key: "maxAccounts", values: { max: n } } : { key: "oneAccount" };
}

type Tone = "active" | "quiet";

/** The pauses every mode has (keys under `cloudAfkAuto`). */
const PAUSES = new Map([
  ["player", "paused.player"],
  ["moved", "paused.moved"],
  ["window", "paused.window"],
  ["trade", "paused.trade"],
]);

/** Each mode's own waits: a rod's and the water's on fishing, a pickaxe's and the target's on mining. */
const WAITS: Record<AutoMode, Map<string, string>> = {
  fishing: new Map([
    ["no_rod", "fishing.noRod"],
    ["rod_low", "fishing.rodLow"],
    ["no_water", "fishing.noWater"],
  ]),
  mining: new Map([
    ["no_tool", "mining.noTool"],
    ["tool_low", "mining.toolLow"],
    ["no_target", "mining.noTarget"],
  ]),
};

/** What a mode does now, in one line (keys under `cloudAfkAuto`); null before the first word. */
export function stateLine(
  mode: AutoMode,
  state: FishState | MineState | null,
): (WordKey & { tone: Tone }) | null {
  if (!state) return null;
  switch (state.state) {
    case "on": {
      if (mode === "mining") return { key: "mining.doing.on", tone: "active" };
      const doing = (state as FishState).doing;
      const known = doing === "cast" || doing === "wait" || doing === "reel";
      return { key: `fishing.doing.${known ? doing : "on"}`, tone: "active" };
    }
    // A reason a newer machine sends reads plainly (the site's "Durakladı."), never as the player's.
    case "paused":
      return { key: PAUSES.get(state.reason ?? "") ?? "pausedOther", tone: "quiet" };
    case "waiting": {
      const reason = state.reason ?? "";
      if (reason === "full") return { key: "inventoryFull", tone: "quiet" };
      // One job at a time: the other mode holds the account (a fallback; the Worker refuses it).
      if (reason === "other") return { key: `${mode}.otherFirst`, tone: "quiet" };
      return { key: WAITS[mode].get(reason) ?? "waitingOther", tone: "quiet" };
    }
    default:
      return null;
  }
}

/** The mode waits for the player's "Devam Et" (`paused` `player` or `moved`). */
export function needsResume(state: FishState | MineState | null): boolean {
  return state?.state === "paused" && (state.reason === "player" || state.reason === "moved");
}

/** The mode may be paused now (it runs, or waits for something that comes by itself). */
export function canPause(state: FishState | MineState | null): boolean {
  return (
    state?.state === "on" ||
    state?.state === "waiting" ||
    (state?.state === "paused" && (state.reason === "window" || state.reason === "trade"))
  );
}

/** The mode is granted (`aim` goes): it runs, waits or is paused, never off. */
export function granted(state: FishState | MineState | null): boolean {
  return state !== null && state.state !== "off" && state.state !== "";
}

/**
 * A refused operation in one line; null when there is nothing to say (`invalid`: the next frame
 * redraws). Keys under `cloudAfkAuto` (an `auto_off` under the mode, as its off state); `aim`: the
 * refused one turned the account. The site's words (afk-web auto-panel `errorLine`).
 */
export function refusalKey(mode: AutoMode, error: CloudAutoError, aim = false): string | null {
  switch (error.code) {
    case "invalid":
      return null;
    case "rate_limited":
      return "refusal.slow";
    case "busy_trade":
      return aim ? "refusal.turnTrade" : "paused.trade";
    case "not_running":
      return "refusal.notRunning";
    case "auto_off":
      return `${mode}.${offKey(error.reason)}`;
    default:
      return "refusal.unreachable";
  }
}

/**
 * Where the account looks, as the game counts it (yaw 0 south, 90 west, 180 north, -90 east), and
 * how high (pitch -45 up, 0 ahead at eye height, 45 down at the block in front of its feet): the
 * site's presets. No frame says where it looks now, so a direction comes first (FM-WEB's note).
 */
export const AIM_DIRECTIONS = { north: 180, east: -90, south: 0, west: 90 } as const;
export type AimDirection = keyof typeof AIM_DIRECTIONS;
export const AIM_TILTS = { up: -45, level: 0, down: 45 } as const;
export type AimTilt = keyof typeof AIM_TILTS;

/** A rate as the panel shows it: whole from 10 up, one decimal below. */
export function rateText(rate: number, locale: string): string {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: rate >= 10 ? 0 : 1 }).format(rate);
}

/**
 * The game's wear bar: how much of it is filled (0-1) and its colour, from green through yellow to
 * red as the item wears (the game's own hue, HSV(dur/max/3, 1, 1)).
 */
export function wear(dur: number, max: number): { fill: number; color: string } {
  const fill = max > 0 ? Math.min(1, Math.max(0, dur / max)) : 0;
  return { fill, color: `hsl(${Math.round(fill * 120)} 100% 50%)` };
}

let next = 0;

/** An operation's own id (`[A-Za-z0-9_-]`, unique among this launcher's pending ones). */
export function autoOpId(): string {
  next = (next + 1) % 1_000_000;
  return `a${Date.now().toString(36)}${next.toString(36)}`;
}
