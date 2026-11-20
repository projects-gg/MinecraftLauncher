// Browser previews of auto-fishing and auto-mining (FISHMINE-L, README "Auto-fishing and
// auto-mining"), for mock.ts. Preview switch (read once, from the page URL), on top of
// `?cloud=accounts&feat=on`; without it the service is one from before the modes (nothing of them
// shows):
// - `&auto=on` (or `fishing`): Madenci07 (mc.madenciler.net) is in game and fishes ("Balık" shows; a
//   small stand-in machine casts and catches every few seconds while the tab is on screen), mining
//   offered and off (an account does one job at a time). GeceBekcisi and TuccarBaykus
//   (play.projects.gg) may mine (off) and their fishing is limited (the contract's seed);
// - `&auto=mining`: Madenci07 mines instead ("Kazma"; a block a second);
// - `&auto=player|host`: Madenci07 has both off and no slot is free (the switches wait);
// - `&auto=paused|moved|trade|norod|nowater` (fishing), `window|full|notarget|lowtool` (mining):
//   what the machine says;
// - `&auto=restart|slots|version|limited`: fishing is off on the machine for that reason;
// - `&auto=settings`: Madenci07 fishes with every choice made, its mining choices kept and off;
//   `&auto=settings-mining` the other way round; `&auto=reviewed`: as `settings`, the fishing
//   command paused by a moderation case.
import type {
  CloudAutoError,
  CloudAutoFeature,
  CloudFishingSetting,
  CloudLiveEvent,
  CloudMiningSetting,
} from "@/bindings";
import type { AutoMode, AutoOp, FishState, MineState } from "./auto/model";

function param(key: string): string | null {
  try {
    const fromSearch = new URLSearchParams(window.location.search).get(key);
    const fromHash = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get(key);
    return fromSearch ?? fromHash;
  } catch {
    return null;
  }
}

export const autoScenario = param("auto");
const MADENCI = "a_7KQ2MZ3P9TW4XB1C";
const GECE = "a_8XH4N2RT6WQ9PB3D";
const TUCCAR = "a_3MCV7Y1KD5HS8FQ2";
const PROJECTS = [GECE, TUCCAR];
/** The contract's seed for Projects' own servers. */
const PROJECTS_REASON = {
  tr: "Projects sunucularında otomatik balık tutma kapalı.",
  en: "Auto-fishing is off on Projects servers.",
};

const OFF_REASONS = ["restart", "slots", "version", "limited"];

/** The scenarios where Madenci07 mines; in the others it fishes (one job at a time). */
const MINING = ["mining", "settings-mining", "window", "full", "notarget", "lowtool"];
const active: AutoMode = MINING.includes(autoScenario ?? "") ? "mining" : "fishing";

function modeOn(mode: AutoMode): boolean {
  if (autoScenario === "player" || autoScenario === "host") return false;
  return mode === active;
}

/** `Account.features.<mode>` of a mock account; null: a service before the modes. */
export function autoFeature(id: string, mode: AutoMode): CloudAutoFeature | null {
  if (autoScenario === null) return null;
  if (id === MADENCI) {
    const waiting = autoScenario === "player" || autoScenario === "host";
    return {
      mode: "on",
      on: modeOn(mode),
      slot: waiting ? autoScenario : null,
      limited: null,
    };
  }
  if (PROJECTS.includes(id)) {
    return {
      mode: "on",
      on: false,
      slot: null,
      limited: mode === "fishing" ? PROJECTS_REASON : null,
    };
  }
  return { mode: "off", on: false, slot: null, limited: null };
}

/** The stored settings of a mock account (only Madenci07 has any). */
export function autoConfig(id: string): {
  fishing: CloudFishingSetting | null;
  mining: CloudMiningSetting | null;
} {
  if (autoScenario === null || id !== MADENCI) return { fishing: null, mining: null };
  if (
    autoScenario === "settings" ||
    autoScenario === "reviewed" ||
    autoScenario === "settings-mining"
  ) {
    return {
      fishing: {
        enabled: active === "fishing",
        rod: { switch: false, keep: 10 },
        full: {
          do: "command",
          command: "/sell all",
          paused:
            autoScenario === "reviewed"
              ? { case: "mc_01JBQ7XK2M9RPV4TNW8HD3CFGZ", category: "advertising" }
              : null,
        },
        look: true,
      },
      mining: {
        enabled: active === "mining",
        blocks: [
          "minecraft:cobblestone",
          "minecraft:stone",
          "minecraft:coal_ore",
          "minecraft:iron_ore",
          "minecraft:diamond_ore",
          "minecraft:deepslate_diamond_ore",
        ],
        tool: { switch: true, keep: 8 },
        full: { do: "keep", command: null },
        idle: { look: true, sneak: false, jump: true, step: false, every: { min: 45, max: 120 } },
      },
    };
  }
  const care = { switch: true, keep: 5 };
  const full = { do: "wait", command: null };
  return {
    fishing: modeOn("fishing") ? { enabled: true, rod: care, full, look: false } : null,
    mining: modeOn("mining")
      ? {
          enabled: true,
          blocks: null,
          tool: care,
          full,
          idle: {
            look: false,
            sneak: false,
            jump: false,
            step: false,
            every: { min: 60, max: 180 },
          },
        }
      : null,
  };
}

interface Machine {
  fishing: FishState | null;
  mining: MineState | null;
  timer: number | null;
  tick: number;
}

const machines = new Map<string, Machine>();
let emit: (event: CloudLiveEvent) => void = () => {};
const watched = new Set<string>();

const minutesAgo = (now: number, minutes: number) => now - minutes * 60;

function fishFor(now: number): FishState | null {
  if (!modeOn("fishing")) return null;
  const state: FishState = {
    state: "on",
    reason: null,
    since: minutesAgo(now, 41),
    doing: "wait",
    rod: { item: "minecraft:fishing_rod", dur: 41, max: 64 },
    today: {
      catches: 31,
      fish: 24,
      treasure: 2,
      junk: 5,
      perHour: 11.4,
      items: [
        { id: "minecraft:cod", n: 15 },
        { id: "minecraft:salmon", n: 7 },
        { id: "minecraft:bowl", n: 2 },
        { id: "minecraft:string", n: 2 },
        { id: "minecraft:tropical_fish", n: 1 },
        { id: "minecraft:pufferfish", n: 1 },
        { id: "minecraft:name_tag", n: 1 },
        { id: "minecraft:enchanted_book", n: 1 },
        { id: "minecraft:leather_boots", n: 1 },
      ],
      last: [
        { id: "minecraft:name_tag", cat: "treasure", ench: null, at: minutesAgo(now, 3) },
        { id: "minecraft:cod", cat: "fish", ench: null, at: minutesAgo(now, 5) },
        { id: "minecraft:salmon", cat: "fish", ench: null, at: minutesAgo(now, 9) },
        { id: "minecraft:enchanted_book", cat: "treasure", ench: true, at: minutesAgo(now, 12) },
        { id: "minecraft:bowl", cat: "junk", ench: null, at: minutesAgo(now, 16) },
        { id: "minecraft:cod", cat: "fish", ench: null, at: minutesAgo(now, 19) },
      ],
    },
  };
  switch (autoScenario) {
    case "paused":
      return { ...state, state: "paused", reason: "player", doing: null };
    case "moved":
      return { ...state, state: "paused", reason: "moved", doing: null };
    case "trade":
      return { ...state, state: "paused", reason: "trade", doing: null };
    case "norod":
      return { ...state, state: "waiting", reason: "no_rod", doing: null, rod: null };
    case "nowater":
      return { ...state, state: "waiting", reason: "no_water", doing: null };
    default:
      if (OFF_REASONS.includes(autoScenario ?? "")) {
        return { ...state, state: "off", reason: autoScenario, doing: null };
      }
      return state;
  }
}

function mineFor(now: number): MineState | null {
  if (!modeOn("mining")) return null;
  const state: MineState = {
    state: "on",
    reason: null,
    since: minutesAgo(now, 63),
    target: { id: "minecraft:cobblestone", x: 128, y: 64, z: -40 },
    tool: { item: "minecraft:iron_pickaxe", dur: 187, max: 250 },
    today: {
      blocks: 1284,
      perMinute: 14,
      items: [
        { id: "minecraft:cobblestone", n: 1102 },
        { id: "minecraft:stone", n: 140 },
        { id: "minecraft:coal_ore", n: 31 },
        { id: "minecraft:iron_ore", n: 11 },
      ],
    },
  };
  switch (autoScenario) {
    case "window":
      return { ...state, state: "paused", reason: "window" };
    case "trade":
      return { ...state, state: "paused", reason: "trade" };
    case "full":
      return { ...state, state: "waiting", reason: "full" };
    case "notarget":
      return { ...state, state: "waiting", reason: "no_target", target: null };
    case "lowtool":
      return {
        ...state,
        state: "waiting",
        reason: "tool_low",
        tool: state.tool ? { ...state.tool, dur: 5 } : null,
      };
    default:
      if (OFF_REASONS.includes(autoScenario ?? "") && autoScenario !== "limited") {
        return { ...state, state: "off", reason: autoScenario, target: null };
      }
      return state;
  }
}

function machineFor(acct: string): Machine {
  let machine = machines.get(acct);
  if (!machine) {
    const now = Math.floor(Date.now() / 1000);
    machine = { fishing: fishFor(now), mining: mineFor(now), timer: null, tick: 0 };
    machines.set(acct, machine);
  }
  return machine;
}

function send(acct: string) {
  if (!watched.has(acct)) return;
  const machine = machineFor(acct);
  emit({ kind: "autoState", acct, fishing: machine.fishing, mining: machine.mining });
}

const CATCHES: Array<[string, "fish" | "treasure" | "junk", number]> = [
  ["minecraft:cod", "fish", 10],
  ["minecraft:salmon", "fish", 5],
  ["minecraft:tropical_fish", "fish", 1],
  ["minecraft:pufferfish", "fish", 1],
  ["minecraft:bowl", "junk", 1],
  ["minecraft:string", "junk", 1],
  ["minecraft:name_tag", "treasure", 1],
];

/** A catch by the table's weights, the same for the same tick (previews stay repeatable). */
function pick(tick: number): [string, "fish" | "treasure" | "junk"] {
  const weights = CATCHES.reduce((sum, [, , w]) => sum + w, 0);
  let roll = (tick * 7919) % weights;
  for (const [id, cat, weight] of CATCHES) {
    if (roll < weight) return [id, cat];
    roll -= weight;
  }
  return ["minecraft:cod", "fish"];
}

function counted(items: { id: string; n: number }[], id: string) {
  const next = items.map((item) => (item.id === id ? { ...item, n: item.n + 1 } : item));
  if (!next.some((item) => item.id === id)) next.push({ id, n: 1 });
  return next.sort((a, b) => b.n - a.n).slice(0, 12);
}

/** One second of the stand-in machine: a block, and now and then a cast, a bite and a catch. */
function step(acct: string) {
  const machine = machineFor(acct);
  machine.tick += 1;
  const now = Math.floor(Date.now() / 1000);
  const mining = machine.mining;
  if (mining?.state === "on") {
    const block = machine.tick % 9 === 0 ? "minecraft:stone" : "minecraft:cobblestone";
    const tool = mining.tool;
    machine.mining = {
      ...mining,
      target: mining.target ? { ...mining.target, id: block } : null,
      tool: tool && machine.tick % 4 === 0 ? { ...tool, dur: Math.max(1, tool.dur - 1) } : tool,
      today: {
        ...mining.today,
        blocks: mining.today.blocks + 1,
        items: counted(mining.today.items, block),
      },
    };
  }
  const fishing = machine.fishing;
  if (fishing?.state === "on") {
    const phase = machine.tick % 6;
    if (phase === 0) machine.fishing = { ...fishing, doing: "cast" };
    else if (phase === 1) machine.fishing = { ...fishing, doing: "wait" };
    else if (phase === 4) machine.fishing = { ...fishing, doing: "reel" };
    else if (phase === 5) {
      const [id, cat] = pick(machine.tick);
      const rod = fishing.rod;
      machine.fishing = {
        ...fishing,
        doing: "wait",
        rod: rod ? { ...rod, dur: Math.max(1, rod.dur - 1) } : rod,
        today: {
          ...fishing.today,
          catches: fishing.today.catches + 1,
          [cat]: fishing.today[cat] + 1,
          items: counted(fishing.today.items, id),
          last: [{ id, cat, ench: null, at: now }, ...fishing.today.last].slice(0, 8),
        },
      };
    }
  }
  send(acct);
}

export function watchAuto(acct: string, running: boolean, sink: (event: CloudLiveEvent) => void) {
  emit = sink;
  watched.add(acct);
  const machine = machineFor(acct);
  if (!running) {
    const off = (state: FishState | MineState | null) =>
      state ? { ...state, state: "off", reason: "not_running" } : null;
    sink({
      kind: "autoState",
      acct,
      fishing: off(machine.fishing) as FishState | null,
      mining: off(machine.mining) as MineState | null,
    });
    return;
  }
  send(acct);
  if (machine.timer === null) machine.timer = window.setInterval(() => step(acct), 1000);
}

export function unwatchAuto(acct: string) {
  watched.delete(acct);
  const machine = machines.get(acct);
  if (machine?.timer != null) {
    window.clearInterval(machine.timer);
    machine.timer = null;
  }
}

const refuse = (code: string, reason: string | null = null): CloudAutoError => ({
  code,
  reason,
  field: null,
});

/** The stand-in machine's answer to one operation: null when it took it, else the refusal. */
export function autoOperation(acct: string, op: AutoOp): CloudAutoError | null {
  const machine = machineFor(acct);
  if (op.action === "aim") {
    const any = machine.fishing ?? machine.mining;
    if (!any || any.state === "off") return refuse("auto_off", any?.reason ?? "setting");
    return null;
  }
  const state = machine[op.mode];
  if (!state || state.state === "off") return refuse("auto_off", state?.reason ?? "setting");
  if (state.state === "paused" && state.reason === "trade") return refuse("busy_trade");
  const now = Math.floor(Date.now() / 1000);
  const next =
    op.action === "pause"
      ? { ...state, state: "paused", reason: "player", since: now }
      : { ...state, state: "on", reason: null, since: now };
  if (op.mode === "fishing") machine.fishing = next as FishState;
  else machine.mining = next as MineState;
  window.setTimeout(() => send(acct), 60);
  return null;
}
