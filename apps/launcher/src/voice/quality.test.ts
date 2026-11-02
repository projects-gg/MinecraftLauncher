import { describe, expect, it } from "vitest";
import { grade, QualityTracker, type RawStats, readStats, windowOf } from "./quality";

/** A stats report as Chromium lays it out (only what is read). */
function report(o: {
  rtt?: number;
  sent: number;
  upLost: number;
  receivers: [string, number, number, number][];
}) {
  return [
    {
      type: "candidate-pair",
      nominated: true,
      state: "succeeded",
      currentRoundTripTime: o.rtt ?? 0.04,
    },
    { type: "candidate-pair", nominated: false, state: "failed", currentRoundTripTime: 9 },
    { type: "outbound-rtp", kind: "audio", packetsSent: o.sent },
    { type: "remote-inbound-rtp", kind: "audio", packetsLost: o.upLost, jitter: 0.004 },
    { type: "outbound-rtp", kind: "video", packetsSent: 100000 },
    ...o.receivers.map(([id, received, lost, jitter]) => ({
      type: "inbound-rtp",
      kind: "audio",
      id,
      packetsReceived: received,
      packetsLost: lost,
      jitter,
    })),
  ];
}

describe("readStats", () => {
  it("takes the nominated pair, our audio and every receiver", () => {
    const raw = readStats(
      report({ rtt: 0.08, sent: 500, upLost: 5, receivers: [["r1", 400, 4, 0.01]] }),
    );
    expect(raw.rttMs).toBeCloseTo(80);
    expect(raw.upSent).toBe(500);
    expect(raw.upLost).toBe(5);
    expect(raw.upJitter).toBeCloseTo(4);
    expect(raw.down.get("r1")).toEqual({ received: 400, lost: 4, jitter: 10 });
  });
});

describe("windowOf and grade", () => {
  const at = (
    sent: number,
    upLost: number,
    receivers: [string, number, number, number][],
    rtt = 0.04,
  ) => readStats(report({ rtt, sent, upLost, receivers }));

  it("measures loss on the stretch, both ways, and calls a clean line good", () => {
    const a = at(1000, 0, [["r1", 1000, 0, 0.005]]);
    const b = at(1500, 1, [["r1", 1500, 2, 0.005]]);
    const window = windowOf(a, b);
    expect(window.loss).toBeCloseTo(2 / 502);
    expect(grade(window)).toBe(0);
  });

  it("calls 6 % loss weak and 20 % bad; a slow round trip too", () => {
    const a = at(1000, 0, [["r1", 1000, 0, 0.005]]);
    expect(grade(windowOf(a, at(1500, 30, [["r1", 1500, 0, 0.005]])))).toBe(1);
    expect(grade(windowOf(a, at(1500, 100, [["r1", 1500, 0, 0.005]])))).toBe(2);
    expect(grade(windowOf(a, at(1500, 0, [["r1", 1500, 0, 0.005]], 0.4)))).toBe(1);
    expect(grade(windowOf(a, at(1500, 0, [["r1", 1500, 0, 0.005]], 0.9)))).toBe(2);
  });

  it("says nothing of loss over a quiet stretch (DTX sends little)", () => {
    const a = at(1000, 0, [["r1", 1000, 0, 0.005]]);
    const window = windowOf(a, at(1010, 5, [["r1", 1010, 5, 0.005]]));
    expect(window.loss).toBeNull();
    expect(grade(window)).toBe(0);
  });

  it("ignores receivers that went or started over, and jitter of silent ones", () => {
    const a = at(1000, 0, [
      ["r1", 1000, 0, 0.005],
      ["gone", 900, 0, 0.005],
    ]);
    const b: RawStats = at(1500, 0, [
      ["r1", 1500, 0, 0.005],
      ["new", 100, 0, 0.2],
      ["idle", 0, 0, 0.5],
    ]);
    const window = windowOf(a, b);
    expect(window.loss).toBe(0);
    // "new" received its packets in this stretch: its jitter counts; "idle" has none.
    expect(window.jitterMs).toBeCloseTo(200);
    expect(grade(window)).toBe(2);
  });
});

describe("QualityTracker", () => {
  it("shows worse at once and better one step per two windows", () => {
    const tracker = new QualityTracker();
    expect(tracker.push(0)).toBe(false);
    expect(tracker.push(2)).toBe(true);
    expect(tracker.quality).toBe(2);
    expect(tracker.push(0)).toBe(false);
    expect(tracker.push(0)).toBe(true);
    expect(tracker.quality).toBe(1);
    // A bad window in between starts the count over.
    expect(tracker.push(0)).toBe(false);
    expect(tracker.push(1)).toBe(false);
    expect(tracker.push(0)).toBe(false);
    expect(tracker.push(0)).toBe(true);
    expect(tracker.quality).toBe(0);
    tracker.push(2);
    tracker.reset();
    expect(tracker.quality).toBe(0);
  });
});
