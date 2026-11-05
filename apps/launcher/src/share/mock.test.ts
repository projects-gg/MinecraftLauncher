import { describe, expect, it } from "vitest";
import { noticeText, refusalText } from "@/features/share/actions";
import { createVoiceMock } from "@/lib/api/voice-mock";
import { createShareMock } from "./mock";
import { shareViewFor } from "./mock-scenarios";
import type { ShareClosedWhy, ShareRefusal } from "./types";

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
      const state = createVoiceMock({ scenario, timers: false }).current();
      expect(state.share?.on, scenario).toBe(true);
      expect(state.room, scenario).not.toBeNull();
    }
    expect(shareViewFor("room")).toBeNull();
    expect(createVoiceMock({ scenario: "room", timers: false }).current().share).toBeUndefined();
  });

  it("starts, watches and stops as the launcher would", async () => {
    const voice = createVoiceMock({ scenario: "shareLiveRow", timers: false });
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
    const quiet = createShareMock(createVoiceMock({ scenario: "shareNotAllowed", timers: false }));
    expect(await quiet.start({ kind: "game" }, "")).toBe("notAllowed");
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
  });
});
