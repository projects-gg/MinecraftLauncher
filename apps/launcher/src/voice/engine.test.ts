import { describe, expect, it } from "vitest";
import type { EngineCommand, EnginePerson, PageReport } from "./boundary";
import { createEngine, DEFAULT_SETTINGS, QUALITY_GOOD_MS, QUALITY_POOR_MS } from "./engine";
import type { RawStats } from "./quality";
import { FakeCall, FakeGraph, FakeTimers } from "./testing";

function rig(options: { graphFails?: boolean } = {}) {
  const reports: PageReport[] = [];
  const timers = new FakeTimers();
  const graphs: FakeGraph[] = [];
  const calls: FakeCall[] = [];
  const engine = createEngine((report) => reports.push(report), {
    openGraph: async (_settings, events) => {
      if (options.graphFails) throw new Error("no audio");
      const graph = new FakeGraph(events);
      graphs.push(graph);
      return graph;
    },
    openCall: (send, callOptions, events) => {
      const call = new FakeCall(send, callOptions, events);
      calls.push(call);
      return call;
    },
    timers,
  });
  const send = (...commands: EngineCommand[]) => {
    for (const command of commands) engine.handle(command);
  };
  const of = <T extends PageReport["t"]>(t: T) =>
    reports.filter((report): report is Extract<PageReport, { t: T }> => report.t === t);
  const graph = () => {
    const graph = graphs.at(-1);
    if (!graph) throw new Error("no graph");
    return graph;
  };
  const call = () => {
    const call = calls.at(-1);
    if (!call) throw new Error("no call");
    return call;
  };
  return { reports, timers, graphs, calls, send, of, graph, call };
}

/** Lets queued steps (promises) run. */
async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

const person = (id: string, volume = 100, silent = false): EnginePerson => ({ id, volume, silent });

/** In a room: joined, answered, receiving `u2` on mid 1. */
async function inRoom() {
  const r = rig();
  r.send({ t: "configure", settings: DEFAULT_SETTINGS }, { t: "join" });
  await settle();
  r.send({ t: "answer", sdp: "room-answer" });
  await settle();
  r.call().nextTracks = ["1"];
  r.send(
    { t: "people", people: [person("u2")] },
    { t: "offer", sdp: "sfu-1", mids: { "1": "u2" } },
  );
  await settle();
  return r;
}

describe("the voice engine", () => {
  it("opens the sound and offers one send-only track on join", async () => {
    const r = rig();
    r.send({ t: "configure", settings: DEFAULT_SETTINGS }, { t: "join" });
    await settle();
    expect(r.graphs).toHaveLength(1);
    expect(r.calls).toHaveLength(1);
    expect(r.call().send).toBe(r.graph().sendTrack);
    expect(r.call().options.ice).toBeNull();
    expect(r.of("offer")).toEqual([{ t: "offer", sdp: "join-offer", mid: "0" }]);
  });

  it("applies the room's answer, answers the SFU's offers and plays each receiver's person", async () => {
    const r = await inRoom();
    expect(r.call().answers).toEqual(["room-answer"]);
    expect(r.call().offers).toEqual(["sfu-1"]);
    expect(r.of("answer")).toEqual([{ t: "answer", sdp: "answer-to-sfu-1" }]);
    expect(r.graph().played.get("1")).toBe(100);
    r.send({ t: "people", people: [person("u2", 180)] });
    expect(r.graph().played.get("1")).toBe(180);
  });

  it("stops a person who left at once, and plays one who came back on the new receiver only", async () => {
    const r = await inRoom();
    r.send({ t: "people", people: [] });
    expect(r.graph().played.get("1")).toBeNull();
    r.call().nextTracks = ["3"];
    r.send(
      { t: "people", people: [person("u2")] },
      { t: "offer", sdp: "sfu-2", mids: { "3": "u2" } },
    );
    await settle();
    expect(r.graph().played.get("3")).toBe(100);
    expect(r.graph().played.get("1")).toBeNull();
  });

  it("neither plays nor lights up a silenced person", async () => {
    const r = await inRoom();
    r.send({ t: "people", people: [person("u2", 100, true)] });
    expect(r.graph().played.get("1")).toBeNull();
    r.graph().events.speaking("1", true);
    r.timers.advance(200);
    expect(r.of("speaking").every((s) => s.ids.length === 0)).toBe(true);
  });

  it("reports who speaks and the player's own speech, changes only", async () => {
    const r = await inRoom();
    r.graph().events.speaking("1", true);
    expect(r.of("speaking").at(-1)).toEqual({ t: "speaking", ids: ["u2"], me: false });
    r.graph().events.me(true);
    r.graph().events.speaking("1", true);
    expect(r.of("speaking")).toHaveLength(1);
    r.timers.advance(100);
    expect(r.of("speaking").at(-1)).toEqual({ t: "speaking", ids: ["u2"], me: true });
    // A receiver of nobody in the room never shows.
    r.graph().events.speaking("9", true);
    r.timers.advance(100);
    expect(r.of("speaking")).toHaveLength(2);
  });

  it("leaves an SFU offer that comes while a join waits for its answer unanswered", async () => {
    const r = rig();
    r.send({ t: "join" });
    await settle();
    r.call().nextTracks = ["1"];
    r.send({ t: "offer", sdp: "old-session", mids: { "1": "u2" } });
    await settle();
    expect(r.of("answer")).toEqual([]);
    expect(r.graph().attached.size).toBe(0);
  });

  it("reports a lost connection and makes a new one on the next join", async () => {
    const r = await inRoom();
    const first = r.call();
    first.events.lost();
    expect(r.of("lost")).toHaveLength(1);
    expect(first.closed).toBe(true);
    expect(r.graph().attached.size).toBe(0);
    r.send({ t: "join" });
    await settle();
    expect(r.calls).toHaveLength(2);
    expect(r.of("offer")).toHaveLength(2);
    // The old connection's late words change nothing.
    first.events.connected();
    expect(r.of("connected")).toEqual([]);
  });

  it("asks for TURN when ICE never connected, then joins again through it", async () => {
    const r = rig();
    r.send({ t: "join" });
    await settle();
    r.send({ t: "answer", sdp: "room-answer" });
    await settle();
    r.call().events.needTurn();
    expect(r.of("needTurn")).toHaveLength(1);
    const ice = {
      urls: ["turns:turn.cloudflare.com:443?transport=tcp"],
      username: "u",
      credential: "c",
    };
    r.send({ t: "ice", ice });
    await settle();
    expect(r.calls).toHaveLength(2);
    expect(r.call().options.ice).toEqual(ice);
    expect(r.of("offer")).toHaveLength(2);
    expect(r.graphs).toHaveLength(1);
  });

  it("joins to listen when the sound cannot open", async () => {
    const r = rig({ graphFails: true });
    r.send({ t: "join" });
    await settle();
    expect(r.of("failed")).toContainEqual({ t: "failed", code: "audio" });
    expect(r.of("mic")).toEqual([{ t: "mic", mic: "missing", changed: false }]);
    expect(r.of("offer")).toEqual([{ t: "offer", sdp: null, mid: null }]);
    expect(r.call().send).toBeNull();
    r.send({ t: "offer", sdp: "sfu-1", mids: { "1": "u2" } });
    await settle();
    expect(r.of("answer")).toHaveLength(1);
  });

  it("leaves nothing running after the stay: no call, no sound, no timer", async () => {
    const r = await inRoom();
    r.call().up();
    r.graph().events.speaking("1", true);
    r.send({ t: "leave" });
    await settle();
    expect(r.call().closed).toBe(true);
    expect(r.graph().closed).toBe(true);
    expect(r.timers.pending).toBe(0);
    expect(r.of("speaking").at(-1)).toEqual({ t: "speaking", ids: [], me: false });
    const reported = r.reports.length;
    r.send({ t: "offer", sdp: "late", mids: { "2": "u3" } }, { t: "answer", sdp: "late" });
    await settle();
    expect(r.reports).toHaveLength(reported);
  });

  it("opens the sound for the settings page and closes it with the page", async () => {
    const r = rig();
    r.send({ t: "setup", on: true });
    await settle();
    const graph = r.graph();
    expect(graph.levelsOn).toBe(true);
    graph.events.level(0.4, true);
    graph.events.devices({ inputs: [{ id: "a1", label: "USB" }], outputs: [] });
    graph.events.mic("ok", false);
    expect(r.of("level")).toEqual([{ t: "level", level: 0.4, open: true }]);
    expect(r.of("devices")).toHaveLength(1);
    expect(r.of("mic")).toEqual([{ t: "mic", mic: "ok", changed: false }]);
    r.send({ t: "selfTest", on: true });
    await settle();
    expect(graph.testOn).toBe(true);
    r.send({ t: "selfTest", on: false });
    await settle();
    expect(graph.testOn).toBe(false);
    expect(graph.closed).toBe(false);
    r.send({ t: "setup", on: false });
    await settle();
    expect(graph.closed).toBe(true);
    graph.events.level(0.9, true);
    expect(r.of("level")).toHaveLength(1);
  });

  it("keeps the sound open while a stay still needs it", async () => {
    const r = await inRoom();
    r.send({ t: "setup", on: true });
    await settle();
    r.send({ t: "setup", on: false });
    await settle();
    expect(r.graph().closed).toBe(false);
    expect(r.graph().levelsOn).toBe(false);
    expect(r.graphs).toHaveLength(1);
  });

  it("applies mute and deafen at once, and before the sound opens", async () => {
    const r = rig();
    r.send({ t: "talk", open: true }, { t: "deaf", on: true }, { t: "join" });
    await settle();
    expect(r.graph().talkOpen).toBe(true);
    expect(r.graph().deafOn).toBe(true);
    r.send({ t: "talk", open: false });
    expect(r.graph().talkOpen).toBe(false);
  });

  it("plays only the shortcut's tick while deafened", async () => {
    const r = await inRoom();
    r.send({ t: "sound", sound: "waiting" }, { t: "deaf", on: true });
    r.send({ t: "sound", sound: "join" }, { t: "sound", sound: "tick" });
    r.send(
      { t: "sound", sound: "leave" },
      { t: "deaf", on: false },
      { t: "sound", sound: "leave" },
    );
    expect(r.graph().sounds).toEqual(["waiting", "stop", "tick", "leave"]);
  });

  it("passes settings to the sound and the bit rate to the call", async () => {
    const r = await inRoom();
    const settings = { ...DEFAULT_SETTINGS, kbps: 32, sensitivity: 40 };
    r.send({ t: "configure", settings });
    await settle();
    expect(r.graph().configured.at(-1)).toEqual(settings);
    expect(r.call().kbps).toBe(32);
  });

  it("loses the call when the room's answer cannot apply", async () => {
    const r = rig();
    r.send({ t: "join" });
    await settle();
    r.send({ t: "answer", sdp: "broken" });
    await settle();
    expect(r.of("failed")).toContainEqual({ t: "failed", code: "answer" });
    expect(r.of("lost")).toHaveLength(1);
    expect(r.call().closed).toBe(true);
  });

  it("checks quality only in a connected call with others, rarely while good", async () => {
    const r = await inRoom();
    const call = r.call();
    expect(r.timers.pending).toBe(0);
    call.up();
    expect(r.of("connected")).toHaveLength(1);
    expect(r.timers.pending).toBe(1);
    const stats = (upSent: number, upLost: number): RawStats => ({
      rttMs: 40,
      upSent,
      upLost,
      upJitter: 2,
      down: new Map(),
    });
    call.stats_ = stats(500, 0);
    r.timers.advance(QUALITY_GOOD_MS);
    await settle();
    expect(call.statsCalls).toBe(1);
    expect(r.of("quality")).toEqual([]);
    // 20 % of the next 500 packets lost: bad at once, then checked often.
    call.stats_ = stats(1000, 100);
    r.timers.advance(QUALITY_GOOD_MS);
    await settle();
    expect(r.of("quality")).toEqual([{ t: "quality", q: 2 }]);
    call.stats_ = stats(1500, 100);
    r.timers.advance(QUALITY_POOR_MS);
    await settle();
    call.stats_ = stats(2000, 100);
    r.timers.advance(QUALITY_POOR_MS);
    await settle();
    expect(r.of("quality")).toEqual([
      { t: "quality", q: 2 },
      { t: "quality", q: 1 },
    ]);
    // Alone in the room: no more checks.
    r.send({ t: "people", people: [] });
    r.timers.advance(QUALITY_POOR_MS);
    await settle();
    const calls = call.statsCalls;
    r.timers.advance(60_000);
    await settle();
    expect(call.statsCalls).toBe(calls);
    expect(r.timers.pending).toBe(0);
  });

  it("marks an interruption bad at once and good when it is back", async () => {
    const r = await inRoom();
    r.call().up();
    r.call().events.interrupted();
    r.call().events.resumed();
    expect(r.of("quality")).toEqual([
      { t: "quality", q: 2 },
      { t: "quality", q: 0 },
    ]);
  });

  it("answers a stats request once, with nothing to say outside a call", async () => {
    const r = rig();
    r.send({ t: "stats" });
    await settle();
    expect(r.of("stats")).toEqual([{ t: "stats", rttMs: null, loss: null, jitterMs: null }]);
    const room = await inRoom();
    room.send({ t: "stats" });
    await settle();
    expect(room.of("stats")).toEqual([{ t: "stats", rttMs: 40, loss: null, jitterMs: null }]);
  });
});
