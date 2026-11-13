// What screen sharing says for the player's own shares (SQOL, sqol.md §1.2; sqol-r.md notices): a
// source that closed asks once what to do (whichever came first, the launcher's notice or its
// state), and its place then says how it ended: a switch, the share going on by itself, or the
// minute running out; a minimised window is only said to a screen reader; the other notices toast
// as before.
import { beforeEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { useVoiceUi } from "@/features/voice/store";
import { createVoiceMock } from "@/lib/api/voice-mock";
import type { VoiceMyShare } from "@/share/types";
import { resetOwnNotices, showShareNotice, watchOwnShares } from "./own-notices";
import { resetOwnUi, useOwnUi } from "./own-store";

const { toast, said } = vi.hoisted(() => ({
  toast: Object.assign(vi.fn(), { dismiss: vi.fn(), error: vi.fn(), success: vi.fn() }),
  said: vi.fn(),
}));
vi.mock("@pl/ui", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@pl/ui")>()),
  toast,
}));
vi.mock("./own-say", () => ({ sayShare: said }));

const base: VoiceMyShare = {
  n: 1,
  kind: "window",
  label: "Kale planı - Paint",
  phase: "live",
  audio: true,
  preset: "normal",
  fps: 30,
  watchers: ["301"],
  since: 0,
  st: "live",
  hold: null,
};

/** A plain sentence in the share's toast (the question's buttons gone). */
const plain = expect.objectContaining({ id: "share-own-1", action: undefined, cancel: undefined });

beforeEach(() => {
  toast.mockClear();
  toast.dismiss.mockClear();
  said.mockClear();
  resetOwnUi();
  resetOwnNotices();
  useVoiceUi
    .getState()
    .setState(createVoiceMock({ scenario: "shareLive", timers: false }).current());
});

describe("the own share's notices", () => {
  it("ask once what to do when the shared window closes, and say when the minute ran out", () => {
    const lost: VoiceMyShare = {
      ...base,
      st: "hold",
      hold: "closed",
      holdUntil: Math.floor(Date.now() / 1000) + 50,
    };
    watchOwnShares([lost]);
    watchOwnShares([lost]);
    expect(toast).toHaveBeenCalledTimes(1);
    expect(toast.mock.calls[0]?.[0]).toBe("Paylaştığın pencere kapandı.");
    expect(toast.mock.calls[0]?.[1]).toMatchObject({
      id: "share-own-1",
      cancel: { label: "Başka Ekran Seç" },
      action: { label: "Paylaşımı Durdur" },
    });
    // The minute ran out while the player stayed in the room: said in the question's place.
    watchOwnShares([]);
    expect(toast).toHaveBeenLastCalledWith("Paylaşım durdu.", plain);
    // The launcher's notice of the same ending says nothing more.
    showShareNotice({ kind: "closed", why: "sourceGone", src: "window" });
    expect(toast).toHaveBeenCalledTimes(2);
  });

  it("ask from the notice when it comes first, and say the minute's end in its place", () => {
    showShareNotice({ kind: "sourceClosed", n: 1, src: "screen" });
    expect(useOwnUi.getState().lost[1]?.why).toBe("closed");
    watchOwnShares([{ ...base, kind: "screen", label: "Ekran 2", st: "hold", hold: undefined }]);
    expect(toast).toHaveBeenLastCalledWith("Paylaştığın ekran kapandı.", expect.anything());
    // The ending's notice before the state let the share go.
    showShareNotice({ kind: "closed", why: "sourceGone", src: "screen" });
    expect(toast).toHaveBeenLastCalledWith("Paylaşım durdu.", plain);
    // The state still holding a moment asks nothing again.
    watchOwnShares([{ ...base, kind: "screen", label: "Ekran 2", st: "hold", hold: "closed" }]);
    expect(toast).toHaveBeenCalledTimes(2);
  });

  it("say in the question's place when the share went on by itself", () => {
    watchOwnShares([{ ...base, kind: "game", label: "", st: "hold", hold: "gameClosed" }]);
    expect(toast).toHaveBeenLastCalledWith(
      "Oyun kapandı.",
      expect.objectContaining({ id: "share-own-1" }),
    );
    // The game's window came back in a few seconds.
    watchOwnShares([{ ...base, kind: "game", label: "", st: "live", hold: null }]);
    expect(toast).toHaveBeenLastCalledWith("Paylaşım sürüyor.", plain);
    // Said once.
    watchOwnShares([{ ...base, kind: "game", label: "", st: "live", hold: null }]);
    expect(toast).toHaveBeenCalledTimes(2);
  });

  it("answer the question with the launcher's new game window", () => {
    watchOwnShares([{ ...base, kind: "game", label: "", st: "hold", hold: "gameClosed" }]);
    showShareNotice({ kind: "switched", n: 1, src: "game", label: "" });
    expect(toast).toHaveBeenLastCalledWith("Artık oyun paylaşılıyor.", plain);
    // The state going live then says nothing more.
    watchOwnShares([{ ...base, kind: "game", label: "", st: "live", hold: null }]);
    expect(toast).toHaveBeenCalledTimes(2);
    expect(toast.dismiss).not.toHaveBeenCalled();
    expect(said).not.toHaveBeenCalled();
  });

  it("say a minimised window and its coming back, without a toast", () => {
    watchOwnShares([base]);
    watchOwnShares([{ ...base, st: "min" }]);
    expect(said).toHaveBeenLastCalledWith("Paylaştığın pencere küçültüldü.");
    watchOwnShares([base]);
    expect(said).toHaveBeenLastCalledWith("Paylaşım sürüyor.");
    expect(toast).not.toHaveBeenCalled();
  });

  it("toast the other notices as before", () => {
    showShareNotice({ kind: "closed", why: "mod", src: "game" });
    expect(toast).toHaveBeenLastCalledWith("Bir moderatör paylaşımını durdurdu.");
    // A switch nobody here asked for (another page): its words anyway.
    showShareNotice({ kind: "switchFailed", n: 2 });
    expect(toast).toHaveBeenLastCalledWith(
      "Bu pencere paylaşılamadı.",
      expect.objectContaining({ id: "share-own-2" }),
    );
    // Leaving the room says nothing more.
    showShareNotice({ kind: "closed", why: "left", src: "game" });
    expect(toast).toHaveBeenCalledTimes(2);
  });
});
