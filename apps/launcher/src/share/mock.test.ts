import { describe, expect, it } from "vitest";
import { noticeText, refusalText } from "@/features/share/actions";
import { createVoiceMock } from "@/lib/api/voice-mock";
import { createShareMock } from "./mock";
import { shareViewFor } from "./mock-scenarios";
import type { ShareClosedWhy, ShareNotice, ShareRefusal } from "./types";

const voiceOf = (scenario: string) => createVoiceMock({ scenario, timers: false });

describe("screen sharing in the preview", () => {
  it("dresses voice scenarios with shares", () => {
    for (const scenario of [
      "shareOffer",
      "shareLive",
      "shareWatch",
      "shareTwo",
      "shareStill",
      "shareCall",
      "shareWatchOnly",
      "shareNotAllowed",
    ]) {
      const state = voiceOf(scenario).current();
      expect(state.share?.on, scenario).toBe(true);
      expect(state.room, scenario).not.toBeNull();
    }
    expect(shareViewFor("room")).toBeNull();
    expect(voiceOf("room").current().share).toBeUndefined();
  });

  it("dresses the player's own share in each of its states (SQOL)", () => {
    const mineOf = (scenario: string) => voiceOf(scenario).current().share?.mine[0];
    for (const scenario of [
      "shareEdit",
      "sharePaused",
      "shareMin",
      "shareLost",
      "shareLostGame",
      "shareSoftware",
      "shareCapped",
      "shareNoEdit",
      "shareSwitchFail",
      "shareLiveMuted",
    ]) {
      expect(voiceOf(scenario).current().share?.mine, scenario).toHaveLength(1);
    }
    expect(voiceOf("shareLiveMuted").current().room?.me.mutedByModerator).toBe(true);
    expect(voiceOf("shareLive").current().room?.me.mutedByModerator).toBe(false);
    expect(mineOf("sharePaused")).toMatchObject({ st: "hold", hold: "paused" });
    expect(mineOf("shareMin")).toMatchObject({ st: "min" });
    expect(mineOf("shareLost")).toMatchObject({ st: "hold", hold: "closed", kind: "window" });
    expect(mineOf("shareLostGame")).toMatchObject({ st: "hold", hold: "gameClosed", kind: "game" });
    expect(voiceOf("shareSoftware").current().share?.hwMax).toBe("normal");
    expect(voiceOf("shareCapped").current().share?.maxPreset).toBe("normal");
    expect(voiceOf("shareNoEdit").current().share?.edit).toBe(false);
    expect(voiceOf("shareLive").current().share?.edit).toBe(true);
  });

  it("starts, watches and stops as the launcher would", async () => {
    const voice = voiceOf("shareLiveRow");
    const share = createShareMock(voice, false);
    expect(await share.start({ kind: "game" }, "")).toBeNull();
    expect(voice.current().share?.mine[0]?.kind).toBe("game");
    expect(await share.watch("301", 1)).toBeNull();
    expect(voice.current().share?.watching).toHaveLength(1);
    expect(await share.watch("301", 9)).toBe("gone");
    await share.unwatch("301", 1);
    expect(voice.current().share?.watching).toHaveLength(0);
    await share.stop(1);
    expect(voice.current().share?.mine).toHaveLength(0);
    // Someone the room does not let share.
    const quiet = createShareMock(voiceOf("shareNotAllowed"));
    expect(await quiet.start({ kind: "game" }, "")).toBe("notAllowed");
    // Someone a moderator muted for everyone: no new share, nothing the room checks; the
    // launcher's own goes on.
    const muted = createShareMock(voiceOf("shareLiveMuted"), false);
    expect(await muted.start({ kind: "screen", id: "1" }, "")).toBe("notAllowed");
    expect(await muted.update(1, { preset: "low" })).toBe("notAllowed");
    expect(await muted.update(1, { audio: false })).toBe("notAllowed");
    expect(await muted.update(1, { cursor: false })).toBeNull();
  });

  it("switches, changes and pauses a live share in place as the launcher would", async () => {
    const voice = voiceOf("shareLive");
    const share = createShareMock(voice, false);
    const notices: ShareNotice[] = [];
    share.onNotice((notice) => notices.push(notice));
    const mine = () => voice.current().share?.mine[0];
    const window = { kind: "window" as const, id: "0x2a01" };
    expect(await share.switchSource(1, window, "Kale planı - Paint")).toBeNull();
    expect(mine()).toMatchObject({ kind: "window", label: "Kale planı - Paint", st: "live" });
    expect(notices).toEqual([
      { kind: "switched", n: 1, src: "window", label: "Kale planı - Paint" },
    ]);
    expect(await share.update(1, { preset: "high", fps: 60 })).toBeNull();
    expect(mine()).toMatchObject({ preset: "high", fps: 60 });
    // Down from "Yüksek", 60 frames come down with it.
    expect(await share.update(1, { preset: "normal" })).toBeNull();
    expect(mine()).toMatchObject({ preset: "normal", fps: 30 });
    expect(await share.hold(1, true)).toBeNull();
    expect(mine()).toMatchObject({ st: "hold", hold: "paused" });
    expect(await share.hold(1, false)).toBeNull();
    expect(mine()).toMatchObject({ st: "live", hold: null });
    expect(await share.hold(9, true)).toBe("gone");
    // The service's limit, this computer's ceiling, an older service.
    expect(await createShareMock(voiceOf("shareCapped"), false).update(1, { preset: "high" })).toBe(
      "budget",
    );
    const soft = voiceOf("shareSoftware");
    expect(await createShareMock(soft, false).update(1, { preset: "high" })).toBeNull();
    expect(soft.current().share?.mine[0]?.preset).toBe("normal");
    expect(await createShareMock(voiceOf("shareNoEdit"), false).update(1, { audio: false })).toBe(
      "off",
    );
    // A switch the publisher could not make: the old source goes on.
    const failing = voiceOf("shareLive");
    const said: ShareNotice[] = [];
    const fails = createShareMock(failing, false, { switchFails: true });
    fails.onNotice((notice) => said.push(notice));
    expect(await fails.switchSource(1, window, "Kale planı - Paint")).toBeNull();
    expect(failing.current().share?.mine[0]?.kind).toBe("game");
    expect(said).toEqual([{ kind: "switchFailed", n: 1 }]);
    // What the player's other share shows cannot be taken.
    expect(
      await createShareMock(voiceOf("shareTwoMine"), false).switchSource(
        1,
        { kind: "screen", id: "\\\\.\\DISPLAY2" },
        "Ekran 2",
      ),
    ).toBe("inUse");
    // Nothing to resume while the source is gone.
    expect(await createShareMock(voiceOf("shareLost"), false).hold(1, false)).toBe("gone");
    // Every raise refused: the rollback shows.
    const refusing = createShareMock(voiceOf("shareLive"), false, { refuseRaise: true });
    expect(await refusing.update(1, { audio: false })).toBeNull();
    expect(await refusing.update(1, { audio: true })).toBe("budget");
  });

  it("has words for every refusal and every ending", () => {
    const refusals: ShareRefusal[] = [
      "off",
      "unsupported",
      "notAllowed",
      "limited",
      "full",
      "roomFull",
      "watchFull",
      "watchMax",
      "budget",
      "gone",
      "failed",
      "inUse",
    ];
    for (const refusal of refusals) expect(refusalText(refusal).length, refusal).toBeGreaterThan(3);
    const endings: ShareClosedWhy[] = ["mod", "budget", "idle", "limit", "sourceGone", "error"];
    for (const why of endings)
      expect(noticeText({ kind: "closed", why, src: "window" }), why).toBeTruthy();
    // Leaving the room says nothing more.
    expect(noticeText({ kind: "closed", why: "left", src: "game" })).toBeNull();
    expect(noticeText({ kind: "watchStopped" })).toBeTruthy();
    // The game ending says so; a window closing says that.
    expect(noticeText({ kind: "closed", why: "sourceGone", src: "game" })).toBe(
      "Oyun kapandığı için paylaşım durdu.",
    );
    expect(noticeText({ kind: "closed", why: "sourceGone", src: "window" })).toBe(
      "Paylaşılan pencere kapandı.",
    );
    // SQOL: the own share's moments.
    expect(noticeText({ kind: "sourceClosed", n: 1, src: "window" })).toBe(
      "Paylaştığın pencere kapandı.",
    );
    expect(noticeText({ kind: "sourceClosed", n: 1, src: "screen" })).toBe(
      "Paylaştığın ekran kapandı.",
    );
    expect(noticeText({ kind: "sourceClosed", n: 1, src: "game" })).toBe("Oyun kapandı.");
    expect(noticeText({ kind: "switched", n: 1, src: "game", label: "" })).toBe(
      "Artık oyun paylaşılıyor.",
    );
    expect(noticeText({ kind: "switchFailed", n: 1 })).toBe("Bu pencere paylaşılamadı.");
  });
});
