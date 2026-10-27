// Browser previews of movement (FEAT-L, README "Movement"), for mock.ts. Preview switch (read once,
// from the page URL), on top of `?cloud=accounts&feat=on`; without it the service is one from before
// movement (nothing of it shows):
// - `&move=on`: GeceBekcisi (play.projects.gg) is in game with movement on: "Hareket" shows beside
//   the inventory; its main account is EjderAvcisi, its other account there Tuccar_Ali (stopped);
//   Madenci07 may move but has it off (the settings' switch);
// - `&move=player|host`: GeceBekcisi has it off and no slot is free (the switch waits);
// - `&move=restart|slots|version|trade|notrunning`: what the machine says for GeceBekcisi;
// - `&move=goto|follow`: GeceBekcisi walks to coordinates or follows EjderAvcisi at first;
// - `&move=nomain`: no main account (only its other account);
// - `&move=away`: the main account is out of sight (walking to it is refused).
// A small stand-in machine answers the operations as the gate would (distance, who may be followed,
// the rate limit), moves the account block by block and ends walks by themselves.
import type {
  CloudLiveEvent,
  CloudMovementError,
  CloudMovementFeature,
  CloudMovementState,
  CloudPos,
} from "@/bindings";
import { distance, GOTO_MAX_BLOCKS, type MoveDir, type MoveOp } from "./movement/model";

function param(key: string): string | null {
  try {
    const fromSearch = new URLSearchParams(window.location.search).get(key);
    const fromHash = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get(key);
    return fromSearch ?? fromHash;
  } catch {
    return null;
  }
}

export const moveScenario = param("move");
export const GECE = "a_8XH4N2RT6WQ9PB3D";
const MADENCI = "a_7KQ2MZ3P9TW4XB1C";
const MAIN = "EjderAvcisi";
const OWN = ["Tuccar_Ali"];

/** GeceBekcisi is in game for the movement previews (it waits to reconnect otherwise). */
export const geceRunning = moveScenario !== null && moveScenario !== "notrunning";

/** `Account.features.movement` of a mock account; null: a service before movement. */
export function movementFeature(id: string): CloudMovementFeature | null {
  if (moveScenario === null) return null;
  if (id === GECE) {
    const waiting = moveScenario === "player" || moveScenario === "host";
    return {
      mode: "on",
      on: !waiting,
      slot: waiting ? moveScenario : null,
      follow: { main: moveScenario === "nomain" ? null : MAIN, own: OWN },
    };
  }
  if (id === MADENCI) return { mode: "on", on: false, slot: null, follow: { main: null, own: [] } };
  return { mode: "off", on: false, slot: null, follow: { main: null, own: [] } };
}

const OFF_REASONS = ["restart", "slots", "version"];

interface Machine {
  pos: CloudPos;
  move: CloudMovementState;
  timer: number | null;
  ops: number[];
}

const machines = new Map<string, Machine>();

function idle(sneak: boolean, ended: string | null = null): CloudMovementState {
  return {
    state: "ok",
    reason: null,
    doing: "idle",
    to: null,
    name: null,
    until: null,
    ended,
    sneak,
    lock: null,
  };
}

function machineFor(acct: string): Machine {
  let machine = machines.get(acct);
  if (!machine) {
    const now = Math.floor(Date.now() / 1000);
    const pos: CloudPos = { x: 128, y: 64, z: -41, dim: "minecraft:overworld", facing: "north" };
    const move = idle(false);
    if (moveScenario === "goto")
      Object.assign(move, { doing: "goto", to: { x: 210, y: 70, z: -96 }, until: now + 110 });
    if (moveScenario === "follow")
      Object.assign(move, { doing: "follow", name: MAIN, until: now + 28 * 60 });
    if (moveScenario === "trade") move.lock = "trade";
    if (OFF_REASONS.includes(moveScenario ?? ""))
      Object.assign(move, { state: "off", reason: moveScenario, doing: null, sneak: null });
    machine = { pos, move, timer: null, ops: [] };
    machines.set(acct, machine);
  }
  return machine;
}

let emit: (event: CloudLiveEvent) => void = () => {};
const watched = new Set<string>();

function send(acct: string) {
  if (!watched.has(acct)) return;
  const machine = machineFor(acct);
  emit({ kind: "movementState", acct, state: { ...machine.move } });
  if (machine.move.state === "ok") emit({ kind: "pos", acct, pos: { ...machine.pos } });
}

export function watchMovement(
  acct: string,
  running: boolean,
  sink: (event: CloudLiveEvent) => void,
) {
  emit = sink;
  watched.add(acct);
  if (!running) {
    sink({
      kind: "movementState",
      acct,
      state: { ...idle(false), state: "off", reason: "not_running", doing: null, sneak: null },
    });
    return;
  }
  send(acct);
}

export function unwatchMovement(acct: string) {
  watched.delete(acct);
}

const TURN: Record<string, { left: string; right: string; back: string }> = {
  north: { left: "west", right: "east", back: "south" },
  south: { left: "east", right: "west", back: "north" },
  east: { left: "north", right: "south", back: "west" },
  west: { left: "south", right: "north", back: "east" },
};
const DELTA: Record<string, [number, number]> = {
  north: [0, -1],
  south: [0, 1],
  east: [1, 0],
  west: [-1, 0],
};

function compass(facing: string, dir: MoveDir): string {
  const turn = TURN[facing] ?? TURN.north;
  if (dir === "forward") return facing;
  return turn?.[dir] ?? facing;
}

function stopTimer(machine: Machine) {
  if (machine.timer !== null) window.clearTimeout(machine.timer);
  machine.timer = null;
}

const refuse = (code: string, extra: Partial<CloudMovementError> = {}): CloudMovementError => ({
  code,
  reason: null,
  field: null,
  retryAfter: null,
  ...extra,
});

/** The stand-in machine's answer to one operation: null when it started, else the refusal. */
export function moveOperation(acct: string, op: MoveOp): CloudMovementError | null {
  const machine = machineFor(acct);
  const now = Date.now();
  if (op.action !== "stop") {
    if (machine.move.state !== "ok") return refuse("movement_off", { reason: machine.move.reason });
    if (machine.move.lock === "trade") return refuse("busy_trade");
    machine.ops = machine.ops.filter((at) => now - at < 2000);
    if (machine.ops.length >= 8) return refuse("rate_limited", { retryAfter: 1 });
    machine.ops.push(now);
  }
  const sneak = Boolean(machine.move.sneak);
  switch (op.action) {
    case "stop":
      stopTimer(machine);
      machine.move = idle(sneak);
      break;
    case "jump":
      break;
    case "sneak":
      machine.move = { ...machine.move, sneak: op.on };
      break;
    case "step": {
      stopTimer(machine);
      const toward = compass(machine.pos.facing, op.dir);
      const [dx, dz] = DELTA[toward] ?? [0, 0];
      let left = op.n;
      machine.move = { ...idle(sneak), doing: "step" };
      const walk = () => {
        machine.pos = {
          ...machine.pos,
          x: machine.pos.x + dx,
          z: machine.pos.z + dz,
        };
        left -= 1;
        if (left <= 0) {
          machine.timer = null;
          machine.move = idle(sneak);
          send(acct);
          return;
        }
        send(acct);
        machine.timer = window.setTimeout(walk, sneak ? 600 : 260);
      };
      machine.timer = window.setTimeout(walk, 120);
      break;
    }
    case "goto": {
      if (distance(machine.pos, op) > GOTO_MAX_BLOCKS) return refuse("too_far");
      stopTimer(machine);
      machine.move = {
        ...idle(sneak),
        doing: "goto",
        to: { x: op.x, y: op.y, z: op.z },
        until: Math.floor(now / 1000) + 120,
      };
      machine.timer = window.setTimeout(() => {
        machine.pos = { ...machine.pos, x: op.x, y: op.y, z: op.z };
        machine.move = idle(sneak, "arrived");
        machine.timer = null;
        send(acct);
      }, 3200);
      break;
    }
    case "approach":
    case "follow": {
      const feature = movementFeature(acct);
      const main = feature?.follow?.main ?? null;
      const own = feature?.follow?.own ?? [];
      const isMain = main !== null && op.name.toLowerCase() === main.toLowerCase();
      if (!isMain && !own.includes(op.name)) return refuse("not_allowed");
      // Tuccar_Ali is stopped in the previews: not in sight.
      if (!isMain || moveScenario === "away") return refuse("not_visible");
      stopTimer(machine);
      machine.move = {
        ...idle(sneak),
        doing: op.action,
        name: op.name,
        until: Math.floor(now / 1000) + (op.action === "follow" ? 1800 : 120),
      };
      if (op.action === "approach") {
        machine.timer = window.setTimeout(() => {
          machine.move = { ...idle(sneak, "arrived"), name: op.name };
          machine.timer = null;
          send(acct);
        }, 2400);
      }
      break;
    }
  }
  window.setTimeout(() => send(acct), 60);
  return null;
}
