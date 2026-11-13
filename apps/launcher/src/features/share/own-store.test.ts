// The player's own live share on the page (SQOL, sqol.md §1.2): a change shows at once and goes
// back when the launcher refuses (one toast says why), a taken change gives way to the state (what
// the room allowed), a switch waits for the launcher's answer, and the card's words follow the
// launcher's state, a notice's mark next.
import { beforeEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { useVoiceUi } from "@/features/voice/store";
import { createVoiceMock } from "@/lib/api/voice-mock";
import type { VoiceState } from "@/lib/api/voice-types";
import { shareApi } from "@/share/api";
import type { ShareChange, ShareRefusal, VoiceMyShare } from "@/share/types";
import { changeShare, holdShare, switchShare } from "./actions";
import { resetOwnNotices, showShareNotice } from "./own-notices";
import {
  endSwitch,
  followShares,
  lostUntil,
  markLost,
  openSwitch,
  ownStatus,
  resetOwnUi,
  SETTLED_MS,
  useOwnUi,
  withPending,
} from "./own-store";
import { useShareUi } from "./store";

// The toasts and the status messages, as the player would see and hear them.
const { toast, said } = vi.hoisted(() => ({
  toast: Object.assign(vi.fn(), { dismiss: vi.fn(), error: vi.fn(), success: vi.fn() }),
  said: vi.fn(),
}));
vi.mock("@pl/ui", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@pl/ui")>()),
  toast,
}));
vi.mock("./own-say", () => ({ sayShare: said }));

const scenario = (name: string): VoiceState =>
  createVoiceMock({ scenario: name, timers: false }).current();

const shown = (n: number): VoiceMyShare => {
  const mine = useVoiceUi.getState().state.share?.mine.find((m) => m.n === n);
  if (!mine) throw new Error(`no share ${n}`);
  return withPending(mine, useOwnUi.getState().pending);
};

/** The launcher's next state for share 1. */
function next(patch: Partial<VoiceMyShare>) {
  const state = useVoiceUi.getState().state;
  const share = state.share;
  if (!share) throw new Error("no share");
  useVoiceUi.getState().setState({
    ...state,
    share: { ...share, mine: share.mine.map((m) => (m.n === 1 ? { ...m, ...patch } : m)) },
  });
}

/** The launcher answers each next change when the test says (the room's notice may come first). */
function answerLater() {
  const answer = {
    resolve: (_refusal: ShareRefusal | null) => {},
    reject: (_error: Error) => {},
  };
  vi.spyOn(shareApi, "update").mockImplementation(
    () =>
      new Promise<ShareRefusal | null>((resolve, reject) => {
        answer.resolve = resolve;
        answer.reject = reject;
      }),
  );
  return answer;
}

const base: VoiceMyShare = {
  n: 1,
  kind: "game",
  label: "",
  phase: "live",
  audio: true,
  preset: "normal",
  fps: 30,
  watchers: [],
  since: 0,
};

/** In the share's own toast, a question there before losing its buttons. */
const own = expect.objectContaining({ id: "share-own-1", action: undefined, cancel: undefined });

beforeEach(() => {
  vi.restoreAllMocks();
  toast.mockClear();
  toast.dismiss.mockClear();
  said.mockClear();
  resetOwnUi();
  resetOwnNotices();
  useShareUi.setState({ picker: false });
  useVoiceUi.getState().setState(scenario("shareLive"));
});

describe("the card's words for a share", () => {
  it("follow the launcher's state first, a notice's mark next", () => {
    expect(ownStatus({ ...base, phase: "starting" })).toBe("starting");
    expect(ownStatus(base)).toBe("live");
    expect(ownStatus({ ...base, st: "hold", hold: "paused" })).toBe("paused");
    // Held without a reason (an older word for it): the player paused it.
    expect(ownStatus({ ...base, st: "hold" })).toBe("paused");
    expect(ownStatus({ ...base, st: "min" })).toBe("minimized");
    expect(ownStatus({ ...base, st: "hold", hold: "closed" })).toBe("closed");
    expect(ownStatus({ ...base, st: "hold", hold: "gameClosed" })).toBe("gameClosed");
    // The notice came before the state.
    const mark = { why: "gameClosed" as const, until: 900, sawHold: false };
    expect(ownStatus(base, mark)).toBe("gameClosed");
    expect(lostUntil(base, mark)).toBe(900);
    expect(lostUntil({ ...base, holdUntil: 950 }, mark)).toBe(950);
  });

  it("end a notice's mark when its share goes live again or ends", () => {
    markLost(1, "closed", null);
    expect(useOwnUi.getState().lost[1]?.sawHold).toBe(false);
    followShares([{ ...base, st: "live" }]);
    // Live before the hold showed: the notice is newer, it stays.
    expect(useOwnUi.getState().lost[1]).toBeDefined();
    followShares([{ ...base, st: "hold" }]);
    expect(useOwnUi.getState().lost[1]?.sawHold).toBe(true);
    followShares([{ ...base, st: "live" }]);
    expect(useOwnUi.getState().lost[1]).toBeUndefined();
    markLost(1, "gameClosed", 1234);
    expect(useOwnUi.getState().lost[1]?.until).toBe(1234);
    followShares([]);
    expect(useOwnUi.getState().lost[1]).toBeUndefined();
  });
});

describe("a live share's changes", () => {
  it("show at once and go back with one sentence when the launcher refuses", async () => {
    vi.spyOn(shareApi, "update").mockResolvedValue("budget");
    const done = changeShare(1, { preset: "high" });
    expect(shown(1).preset).toBe("high");
    expect(await done).toBe(false);
    expect(shown(1).preset).toBe("normal");
    expect(toast).toHaveBeenCalledWith("Bu kalite şu an kapalı.");
    expect(said).not.toHaveBeenCalled();
    vi.spyOn(shareApi, "update").mockResolvedValue("off");
    await changeShare(1, { audio: false });
    expect(shown(1).audio).toBe(true);
    expect(toast).toHaveBeenLastCalledWith("Bu ayar şu an değiştirilemiyor.");
    vi.spyOn(shareApi, "update").mockRejectedValue(new Error("gone"));
    await changeShare(1, { cursor: false });
    expect(toast).toHaveBeenLastCalledWith("Bu ayar değiştirilemedi.");
    expect(useOwnUi.getState().pending).toEqual([]);
  });

  it("keep a taken change until the state moves on it, and say then what the room took", async () => {
    vi.spyOn(shareApi, "update").mockResolvedValue(null);
    expect(await changeShare(1, { preset: "high" })).toBe(true);
    expect(shown(1).preset).toBe("high");
    // The room decides: nothing is said before the state shows what it took.
    expect(said).not.toHaveBeenCalled();
    expect(toast).not.toHaveBeenCalled();
    // Something else in the room changed meanwhile: the change still shows.
    next({ watchers: ["301"] });
    expect(shown(1).preset).toBe("high");
    expect(said).not.toHaveBeenCalled();
    // The room's answer: the state now says what it took (here less than asked).
    next({ preset: "low" });
    expect(shown(1).preset).toBe("low");
    expect(said).toHaveBeenCalledExactlyOnceWith("Kalite: Düşük, 480p.");
    expect(useOwnUi.getState().pending).toEqual([]);
    // The launcher's own (the cursor) is said as soon as it answers.
    await changeShare(1, { cursor: false });
    expect(said).toHaveBeenLastCalledWith("İmleç gizlendi.");
  });

  it("say what the room took when its answer comes before the launcher's", async () => {
    const answer = answerLater();
    const done = changeShare(1, { audio: false });
    next({ audio: false });
    expect(said).not.toHaveBeenCalled();
    answer.resolve(null);
    expect(await done).toBe(true);
    expect(said).toHaveBeenCalledExactlyOnceWith("Ses paylaşılmıyor.");
    expect(useOwnUi.getState().pending).toEqual([]);
  });

  it("let the state show what the room kept when it never moved, saying nothing", async () => {
    vi.useFakeTimers();
    try {
      vi.spyOn(shareApi, "update").mockResolvedValue(null);
      await changeShare(1, { preset: "high" });
      expect(shown(1).preset).toBe("high");
      vi.advanceTimersByTime(SETTLED_MS);
      expect(shown(1).preset).toBe("normal");
      next({ watchers: ["301"] });
      expect(said).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("take back only what the room refused later, on that share", async () => {
    useVoiceUi.getState().setState(scenario("shareTwoMine"));
    vi.spyOn(shareApi, "update").mockResolvedValue(null);
    // The other share's sound, this share's cursor (local), then its quality (the room's).
    await changeShare(2, { audio: true });
    await changeShare(1, { cursor: false });
    await changeShare(1, { preset: "low" });
    showShareNotice({ kind: "refused", refusal: "budget" });
    expect(shown(1)).toMatchObject({ preset: "high", cursor: false });
    expect(shown(2).audio).toBe(true);
    expect(toast).toHaveBeenLastCalledWith("Bu kalite şu an kapalı.");
    // The next refusal: the other share's sound, the room's too.
    showShareNotice({ kind: "refused", refusal: "budget" });
    expect(shown(2).audio).toBe(false);
    expect(toast).toHaveBeenLastCalledWith("Bu ayar şu an değiştirilemiyor.");
    // Nothing waits for the room any more: a refusal of a share, as before.
    showShareNotice({ kind: "refused", refusal: "budget" });
    expect(toast).toHaveBeenLastCalledWith("Ekran paylaşımı şu an kullanılamıyor.");
    expect(shown(1).cursor).toBe(false);
  });

  it("take back at once a change the room refused before the launcher answered, never as done", async () => {
    const answer = answerLater();
    const done = changeShare(1, { preset: "high" });
    expect(shown(1).preset).toBe("high");
    // The room refused fast: its notice comes before the launcher's answer.
    showShareNotice({ kind: "refused", refusal: "notAllowed" });
    expect(shown(1).preset).toBe("normal");
    expect(toast).toHaveBeenCalledExactlyOnceWith("Bu ayar şu an değiştirilemiyor.");
    answer.resolve(null);
    expect(await done).toBe(false);
    expect(shown(1).preset).toBe("normal");
    expect(said).not.toHaveBeenCalled();
    expect(useOwnUi.getState().pending).toEqual([]);
    // The launcher's answer says no as well, or fails: the notice's sentence stays the only one.
    const refusedToo = changeShare(1, { audio: false });
    showShareNotice({ kind: "refused", refusal: "notAllowed" });
    answer.resolve("notAllowed");
    expect(await refusedToo).toBe(false);
    const failed = changeShare(1, { fps: 15 });
    showShareNotice({ kind: "refused", refusal: "notAllowed" });
    answer.reject(new Error("gone"));
    expect(await failed).toBe(false);
    expect(toast).toHaveBeenCalledTimes(3);
    expect(said).not.toHaveBeenCalled();
    expect(shown(1)).toMatchObject({ preset: "normal", audio: true, fps: 30 });
  });

  it("take back a change the room refused after the launcher answered, never said as done", async () => {
    vi.spyOn(shareApi, "update").mockResolvedValue(null);
    expect(await changeShare(1, { audio: false })).toBe(true);
    expect(shown(1).audio).toBe(false);
    showShareNotice({ kind: "refused", refusal: "notAllowed" });
    expect(shown(1).audio).toBe(true);
    expect(toast).toHaveBeenCalledExactlyOnceWith("Bu ayar şu an değiştirilemiyor.");
    // The state moves later on something else: still nothing said of the refused change.
    next({ watchers: ["301"] });
    expect(said).not.toHaveBeenCalled();
    expect(useOwnUi.getState().pending).toEqual([]);
  });

  it("say why the room refused: a moderator's mute, the room's rule, else that it can't change now", async () => {
    vi.spyOn(shareApi, "update").mockResolvedValue(null);
    const refused = async (change: ShareChange) => {
      await changeShare(1, change);
      showShareNotice({ kind: "refused", refusal: "notAllowed" });
      return toast.mock.lastCall?.[0];
    };
    expect(await refused({ audio: false })).toBe("Bu ayar şu an değiştirilemiyor.");
    // "Kim Paylaşabilir" leaves the player out now.
    const before = useVoiceUi.getState().state;
    if (!before.share) throw new Error("no share");
    useVoiceUi
      .getState()
      .setState({ ...before, share: { ...before.share, allowed: false, shareBy: "mods" } });
    expect(await refused({ preset: "low" })).toBe(
      "Bu odada yalnız lider ve moderatörler paylaşabilir.",
    );
    // A moderator muted the player for everyone: that is said first.
    const ruled = useVoiceUi.getState().state;
    if (!ruled.room) throw new Error("no room");
    useVoiceUi.getState().setState({
      ...ruled,
      room: { ...ruled.room, me: { ...ruled.room.me, mutedByModerator: true } },
    });
    expect(await refused({ fps: 15 })).toBe(
      "Bir moderatör seni susturduğu için şu an değiştiremezsin.",
    );
    // The launcher's own answer says it the same way.
    vi.spyOn(shareApi, "update").mockResolvedValue("notAllowed");
    expect(await changeShare(1, { audio: false })).toBe(false);
    expect(toast).toHaveBeenLastCalledWith(
      "Bir moderatör seni susturduğu için şu an değiştiremezsin.",
    );
    expect(said).not.toHaveBeenCalled();
    expect(shown(1)).toMatchObject({ preset: "normal", audio: true, fps: 30 });
  });

  it("pause and resume, back as it was when refused", async () => {
    vi.spyOn(shareApi, "hold").mockResolvedValue(null);
    expect(await holdShare(1, true)).toBe(true);
    expect(ownStatus(shown(1))).toBe("paused");
    expect(said).toHaveBeenLastCalledWith("Paylaşım duraklatıldı.");
    next({ st: "hold", hold: "paused" });
    expect(await holdShare(1, false)).toBe(true);
    expect(ownStatus(shown(1))).toBe("live");
    expect(said).toHaveBeenLastCalledWith("Paylaşım sürüyor.");
    next({ st: "live", hold: null });
    vi.spyOn(shareApi, "hold").mockResolvedValue("off");
    expect(await holdShare(1, true)).toBe(false);
    expect(ownStatus(shown(1))).toBe("live");
    expect(toast).toHaveBeenLastCalledWith("Paylaşım duraklatılamadı.");
  });

  it("switch the source in place, saying how it went when the launcher answers", async () => {
    vi.spyOn(shareApi, "switchSource").mockResolvedValue(null);
    const window = { kind: "window" as const, id: "0x2a01" };
    expect(await switchShare(1, window, "Kale planı - Paint")).toBe(true);
    // Shown at once; said when the publisher took it.
    expect(shown(1)).toMatchObject({ kind: "window", label: "Kale planı - Paint" });
    expect(toast).not.toHaveBeenCalled();
    showShareNotice({ kind: "switched", n: 1, src: "window", label: "Kale planı - Paint" });
    expect(toast).toHaveBeenLastCalledWith('Artık "Kale planı - Paint" paylaşılıyor.', own);
    next({ kind: "window", label: "Kale planı - Paint", source: window });
    // The new source could not start: the old one goes on.
    await switchShare(1, { kind: "screen", id: "\\\\.\\DISPLAY2" }, "Ekran 2");
    expect(shown(1)).toMatchObject({ kind: "screen", label: "Ekran 2" });
    showShareNotice({ kind: "switchFailed", n: 1 });
    expect(shown(1)).toMatchObject({ kind: "window", label: "Kale planı - Paint" });
    expect(toast).toHaveBeenLastCalledWith("Bu ekran paylaşılamadı.", own);
    // What the other share shows: refused at once.
    vi.spyOn(shareApi, "switchSource").mockResolvedValue("inUse");
    expect(await switchShare(1, { kind: "game" }, "Minecraft 26.1.2")).toBe(false);
    expect(shown(1)).toMatchObject({ kind: "window" });
    expect(toast).toHaveBeenLastCalledWith("Bunu zaten paylaşıyorsun.", own);
  });

  it("keep asking when a switch fails while the source is gone", async () => {
    next({ st: "hold", hold: "closed", holdUntil: Math.floor(Date.now() / 1000) + 40 });
    vi.spyOn(shareApi, "switchSource").mockResolvedValue("failed");
    await switchShare(1, { kind: "window", id: "0x2a03" }, "Sunucu haritası");
    expect(toast).toHaveBeenLastCalledWith(
      "Bu pencere paylaşılamadı.",
      expect.objectContaining({
        id: "share-own-1",
        cancel: expect.objectContaining({ label: "Başka Ekran Seç" }),
        action: expect.objectContaining({ label: "Paylaşımı Durdur" }),
      }),
    );
  });

  it("open the picker for a switch, and forget it when it closes", () => {
    openSwitch(1);
    expect(useOwnUi.getState().switching).toBe(1);
    expect(useShareUi.getState().picker).toBe(true);
    endSwitch();
    expect(useOwnUi.getState().switching).toBeNull();
    // The switched share ending takes the switch with it.
    openSwitch(1);
    followShares([]);
    expect(useOwnUi.getState().switching).toBeNull();
  });
});
