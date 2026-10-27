import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import type { CloudLiveEvent, CloudMovementState } from "@/bindings";
import {
  coordinate,
  dimension,
  distance,
  doingKey,
  endedKey,
  facingKey,
  GOTO_MAX_BLOCKS,
  isGoing,
  offKey,
  reduceMovement,
  refusalKey,
  slotKey,
  WAITING,
} from "./model";

/** The contract's shared cases (projects-studio `5158bdb`), as pl-cloud-afk's tests read them. */
const vectors = JSON.parse(
  readFileSync(
    new URL(
      "../../../../../../../crates/pl-cloud-afk/tests/vectors/move-op-v1.json",
      import.meta.url,
    ),
    "utf8",
  ),
) as {
  gate: {
    name: string;
    state: { pos: { x: number; y: number; z: number } | null };
    op: { action: string; x?: number; y?: number; z?: number };
    expected: { ok?: boolean; error?: { code: string } };
  }[];
};

const state = (fields: Partial<CloudMovementState>): CloudMovementState => ({
  state: "ok",
  reason: null,
  doing: "idle",
  to: null,
  name: null,
  until: null,
  ended: null,
  sneak: false,
  lock: null,
  ...fields,
});

describe("movement", () => {
  it("says too far exactly where the machine does, from the block it stands in", () => {
    const walks = vectors.gate.filter(
      (c) =>
        c.op.action === "goto" &&
        c.state.pos &&
        (c.expected.ok || c.expected.error?.code === "too_far"),
    );
    expect(walks.length).toBeGreaterThanOrEqual(5);
    for (const walk of walks) {
      const pos = walk.state.pos as { x: number; y: number; z: number };
      // The `pos` frame names the block (rounded down), the gate the feet in it.
      const block = { x: Math.floor(pos.x), y: Math.floor(pos.y), z: Math.floor(pos.z) };
      const far = distance(block, walk.op as { x: number; y: number; z: number }) > GOTO_MAX_BLOCKS;
      expect(far, walk.name).toBe(walk.expected.error?.code === "too_far");
    }
  });

  it("reads typed coordinates in the contract's ranges", () => {
    expect(coordinate(" 120 ", "x")).toBe(120);
    expect(coordinate("−40", "z")).toBe(-40);
    expect(coordinate("2048", "y")).toBe(2048);
    expect(coordinate("2049", "y")).toBeNull();
    expect(coordinate("30000001", "x")).toBeNull();
    expect(coordinate("1.5", "x")).toBeNull();
    expect(coordinate("", "x")).toBeNull();
    expect(coordinate("abc", "z")).toBeNull();
  });

  it("follows only its own account's frames and forgets the place when it cannot move", () => {
    const pos: CloudLiveEvent = {
      kind: "pos",
      acct: "a_1",
      pos: { x: 1, y: 64, z: 2, dim: "minecraft:overworld", facing: "north" },
    };
    const ok: CloudLiveEvent = { kind: "movementState", acct: "a_1", state: state({}) };
    let current = reduceMovement(WAITING, ok, "a_1");
    current = reduceMovement(current, pos, "a_1");
    expect(current.pos?.facing).toBe("north");
    expect(reduceMovement(current, { ...pos, acct: "a_2" }, "a_1")).toBe(current);
    const off = reduceMovement(
      current,
      { kind: "movementState", acct: "a_1", state: state({ state: "off", reason: "not_running" }) },
      "a_1",
    );
    expect(off.pos).toBeNull();
    expect(off.move?.reason).toBe("not_running");
  });

  it("words states, ends and refusals as the contract's table", () => {
    expect(offKey("not_running")).toBe("notRunning");
    expect(offKey("setting")).toBe("off");
    expect(offKey("slots")).toBe("noSlot");
    expect(offKey("something_new")).toBe("serviceOff");
    expect(slotKey("host")).toEqual({ key: "noSlot" });
    expect(slotKey("player", 1)).toEqual({ key: "oneAccount" });
    expect(slotKey("player", 3)).toEqual({ key: "maxAccounts", values: { max: 3 } });
    expect(
      refusalKey({ code: "stale", reason: null, field: null, retryAfter: null }, null, null),
    ).toBeNull();
    expect(
      refusalKey({ code: "rate_limited", reason: null, field: null, retryAfter: 1 }, null, null),
    ).toEqual({ key: "slow" });
    expect(
      refusalKey(
        { code: "movement_off", reason: "restart", field: null, retryAfter: null },
        null,
        null,
      ),
    ).toEqual({ key: "restart" });
    // "Not nearby" names another own account, never the main one.
    const notVisible = { code: "not_visible", reason: null, field: null, retryAfter: null };
    expect(refusalKey(notVisible, "kaan_07", "Kaan_07")).toEqual({ key: "notNear" });
    expect(refusalKey(notVisible, "AfkBot_2", "Kaan_07")).toEqual({
      key: "notNearNamed",
      values: { name: "AfkBot_2" },
    });
    expect(endedKey("out_of_sight", "AfkBot_2", null)).toEqual({
      key: "notNearNamed",
      values: { name: "AfkBot_2" },
    });
    expect(endedKey("arrived", null, null)).toEqual({ key: "arrived" });
    expect(endedKey(null, null, null)).toBeNull();
    expect(
      refusalKey({ code: "host_offline", reason: null, field: null, retryAfter: null }, null, null),
    ).toEqual({
      key: "unreachable",
    });
  });

  it("says what it is doing and when Dur shows", () => {
    expect(doingKey(state({ doing: "goto", to: { x: 120, y: 64, z: -40 } }))).toEqual({
      key: "going",
      values: { x: 120, y: 64, z: -40 },
    });
    expect(doingKey(state({ doing: "follow", name: "Kaan_07" }))).toEqual({
      key: "following",
      values: { name: "Kaan_07" },
    });
    expect(doingKey(state({ doing: "follow", name: "Kaan_07", lock: "trade" }))).toEqual({
      key: "trade",
    });
    expect(doingKey(state({ doing: "idle" }))).toBeNull();
    expect(isGoing(state({ doing: "step" }))).toBe(true);
    expect(isGoing(state({ doing: "idle" }))).toBe(false);
    expect(isGoing(null)).toBe(false);
  });

  it("names dimensions and directions", () => {
    expect(dimension("minecraft:the_nether")).toEqual({ key: "nether" });
    expect(dimension("projects:spawn")).toEqual({ text: "spawn" });
    expect(facingKey("west")).toBe("west");
    expect(facingKey("up")).toBe("south");
  });
});
