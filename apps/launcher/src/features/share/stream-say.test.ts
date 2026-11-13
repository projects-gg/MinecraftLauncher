// What screen readers hear of the room's streams (SQOL-U1, WCAG 4.1.3): someone went live or
// ended; a watched stream opened, paused, had its window minimized or came back. Nothing about the
// player's own shares, nothing for the shares already there when the player came.
import { describe, expect, it } from "vitest";
import { shareViewFor } from "@/share/mock-scenarios";
import type { VoiceLiveShare, VoiceShareView } from "@/share/types";
import { streamSayings } from "./stream-say";

const base = shareViewFor("shareOffer") as VoiceShareView;

const live = (person: string, st?: VoiceLiveShare["st"]): VoiceLiveShare => ({
  person,
  n: 1,
  kind: "game",
  audio: true,
  preset: "normal",
  fps: 30,
  watchers: 1,
  since: 1,
  ...(st ? { st } : {}),
});

const view = (patch: Partial<VoiceShareView>): VoiceShareView => ({ ...base, ...patch });

describe("what screen readers hear of the streams", () => {
  it("says who went live and whose stream ended, never the player's own", () => {
    const before = view({ live: [live("301")] });
    const after = view({ live: [live("909"), live("100")] });
    expect(streamSayings(before, after, "100", true)).toEqual([
      { kind: "live", person: "909", n: 1 },
      { kind: "end", person: "301", n: 1 },
    ]);
  });

  it("says nothing of what was there when the player came, or when sharing goes", () => {
    expect(streamSayings(null, view({ live: [live("301")] }), "100", true)).toEqual([]);
    expect(streamSayings(view({ live: [live("301")] }), null, "100", true)).toEqual([]);
  });

  it("says a watched stream paused, was minimized and came back", () => {
    const watching = [{ person: "301", n: 1, layer: "f" as const, phase: "live" as const }];
    const a = view({ live: [live("301")], watching });
    const b = view({ live: [live("301", "hold")], watching });
    const c = view({ live: [live("301", "min")], watching });
    const d = view({ live: [live("301", "live")], watching });
    expect(streamSayings(a, b, "100", true)).toEqual([{ kind: "hold", person: "301", n: 1 }]);
    expect(streamSayings(b, c, "100", true)).toEqual([{ kind: "min", person: "301", n: 1 }]);
    expect(streamSayings(c, d, "100", true)).toEqual([{ kind: "resume", person: "301", n: 1 }]);
    // Not watched, or watched in the other window: nothing.
    const away = view({ live: [live("301")] });
    expect(streamSayings(away, view({ live: [live("301", "hold")] }), "100", true)).toEqual([]);
    expect(streamSayings(a, b, "100", false)).toEqual([]);
  });

  it("says a watch opened once it connects", () => {
    const connecting = [{ person: "301", n: 1, layer: "f" as const, phase: "connecting" as const }];
    const open = [{ person: "301", n: 1, layer: "f" as const, phase: "live" as const }];
    const a = view({ live: [live("301")], watching: connecting });
    const b = view({ live: [live("301")], watching: open });
    expect(streamSayings(a, b, "100", true)).toEqual([{ kind: "open", person: "301", n: 1 }]);
    expect(streamSayings(b, b, "100", true)).toEqual([]);
  });
});
