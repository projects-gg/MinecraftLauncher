// SOC-NEXT §3.7 for voice chat (VOICE-CALL): an ignored person's ring stays quiet on the page, the
// launcher hears the list once per change, and their missed call lights nothing.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useSocNextFlag } from "@/features/social/next/flag";
import { useSocNextPrefs } from "@/features/social/next/prefs";
import { voiceApi } from "@/lib/api/voice-api";
import { HIDDEN_VOICE_STATE, type VoiceRing, type VoiceState } from "@/lib/api/voice-types";

vi.mock("./ring-toast", () => ({ showRingToast: vi.fn(), dismissRingToast: vi.fn() }));

const { onVoiceState } = await import("./host");
const { showRingToast, dismissRingToast } = await import("./ring-toast");
const { heardRings, syncIgnored, toldMissed } = await import("./quiet");
const { unseenMissed } = await import("./store");

const ring = (from: string, room: string): VoiceRing => ({ room, from, with: [], until: 2e9 });
const ringing = (...rings: VoiceRing[]): VoiceState => ({
  ...HIDDEN_VOICE_STATE,
  available: true,
  enabled: true,
  rings,
});

beforeEach(() => {
  useSocNextFlag.setState({ on: true });
  useSocNextPrefs.setState({ ignored: ["KorAtesi"] });
});

afterEach(() => {
  useSocNextFlag.setState({ on: false });
  useSocNextPrefs.setState({ ignored: [] });
  vi.clearAllMocks();
  vi.restoreAllMocks();
});

describe("an ignored person's ring", () => {
  it("is not heard, nor is their missed call counted", () => {
    const rings = [ring("KorAtesi", "f1"), ring("GeceKurdu_", "f2")];
    expect(heardRings(rings).map((r) => r.from)).toEqual(["GeceKurdu_"]);
    const missed = [
      { room: "f1", from: "KorAtesi", at: 1, joinable: true },
      { room: "f2", from: "GeceKurdu_", at: 1, joinable: true },
    ];
    expect(toldMissed(missed).map((m) => m.from)).toEqual(["GeceKurdu_"]);
    expect(unseenMissed({ state: { ...ringing(), missed }, seenMissed: [] })).toBe(1);
  });

  it("rings again with SOC-NEXT's switch off", () => {
    useSocNextFlag.setState({ on: false });
    expect(heardRings([ring("KorAtesi", "f1")])).toHaveLength(1);
  });

  it("gets no card while the launcher has not heard the list yet", () => {
    onVoiceState(ringing(), ringing(ring("KorAtesi", "f1"), ring("GeceKurdu_", "f2")));
    expect(showRingToast).toHaveBeenCalledTimes(1);
    expect(vi.mocked(showRingToast).mock.calls[0]?.[0].from).toBe("GeceKurdu_");
  });

  it("takes its card away when the player ignores them while it rings", () => {
    const before = ringing(ring("KorAtesi", "f1"));
    // Before the launcher has moved it to the missed list, and after.
    onVoiceState(before, before);
    expect(dismissRingToast).toHaveBeenCalledWith("f1");
    vi.mocked(dismissRingToast).mockClear();
    onVoiceState(before, ringing());
    expect(dismissRingToast).toHaveBeenCalledWith("f1");
    expect(showRingToast).not.toHaveBeenCalled();
  });
});

describe("the list the launcher hears", () => {
  it("goes once on start and once per change, empty with the switch off", () => {
    const sent = vi.spyOn(voiceApi, "setIgnored").mockResolvedValue();
    const stop = syncIgnored();
    expect(sent).toHaveBeenLastCalledWith(["KorAtesi"]);
    // A nickname or a pin changes nothing it says.
    useSocNextPrefs.setState({ pinnedChats: ["f9"] });
    expect(sent).toHaveBeenCalledTimes(1);
    useSocNextPrefs.setState({ ignored: ["KorAtesi", "ElmasKazma"] });
    expect(sent).toHaveBeenLastCalledWith(["KorAtesi", "ElmasKazma"]);
    useSocNextFlag.setState({ on: false });
    expect(sent).toHaveBeenLastCalledWith([]);
    expect(sent).toHaveBeenCalledTimes(3);
    stop();
    useSocNextFlag.setState({ on: true });
    expect(sent).toHaveBeenCalledTimes(3);
  });
});
