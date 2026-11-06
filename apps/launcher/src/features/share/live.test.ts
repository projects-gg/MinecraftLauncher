// @vitest-environment happy-dom
// "YAYINDA" and its card (SHARE-LIVE): whom a tag stands for and what its card offers, in the room,
// on the crew page and in the friends list; and the card's picture, asked only while a card is
// open, again when the room's picture time moves, never on its own.
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createVoiceMock } from "@/lib/api/voice-mock";
import type { VoiceState } from "@/lib/api/voice-types";
import { shareApi } from "@/share/api";
import type { ShareThumb } from "@/share/types";
import {
  asksPicture,
  crewRoomTargets,
  forgetThumbs,
  friendTarget,
  type LiveTarget,
  liveTargetIn,
  ownTarget,
  THUMB_KEEP_MS,
  useShareThumb,
} from "./live";

const scenario = (name: string): VoiceState =>
  createVoiceMock({ scenario: name, timers: false }).current();
const STONE = "c00000000000000b2";
const roomName = (room: { name: string | null }) => room.name ?? "";

function stone(state: VoiceState) {
  const entry = state.crews[0];
  const room = entry?.rooms.find((r) => r.id === STONE);
  if (!entry || !room) throw new Error("no Taş Ocağı");
  return { entry, room };
}

describe("whom a tag stands for", () => {
  it("in the room: someone's share with what it shows and who watches, then the player's own", () => {
    const state = scenario("shareLiveRow");
    expect(liveTargetIn(state, "301")).toMatchObject({
      person: "301",
      room: "f00000000000000a1",
      n: 1,
      kind: "game",
      watchers: 2,
      offer: "watch",
    });
    expect(liveTargetIn(state, "301")?.th).toBeTypeOf("number");
    expect(liveTargetIn(state, "303")).toBeNull();
    expect(liveTargetIn(scenario("shareWatch"), "301")?.offer).toBe("watching");
    expect(ownTarget(scenario("shareLive"))).toMatchObject({ offer: "own", watchers: 2 });
    // A share on its way is not live yet; nothing of sharing in a room that does not share.
    expect(ownTarget(scenario("shareStarting"))).toBeNull();
    expect(liveTargetIn(scenario("room"), "301")).toBeNull();
  });

  it("on the crew page: everyone live in a room, joinable or full, nothing where nobody is", () => {
    const open = scenario("shareCrewLive");
    const { entry, room } = stone(open);
    const targets = crewRoomTargets(open, entry.crew, room, entry.crewMax, "Taş Ocağı");
    expect(targets).toEqual([
      expect.objectContaining({ person: "302", room: STONE, n: null, offer: "join" }),
    ]);
    expect(targets[0]?.roomName).toBe("Taş Ocağı");
    const full = scenario("shareCrewLiveFull");
    const busy = stone(full);
    expect(
      crewRoomTargets(full, busy.entry.crew, busy.room, busy.entry.crewMax, "Taş Ocağı")[0]?.offer,
    ).toBe("full");
    const general = open.crews[0]?.rooms[0];
    if (!general) throw new Error("no Genel");
    expect(crewRoomTargets(open, entry.crew, general, entry.crewMax, "Genel")).toEqual([]);
  });

  it("in the friends list: a crew room the player may join, else someone's own call (locked)", () => {
    const state = scenario("shareFriendsLive");
    expect(friendTarget(state, "301", roomName)).toMatchObject({
      room: STONE,
      offer: "join",
      roomName: "Taş Ocağı",
    });
    expect(friendTarget(state, "303", roomName)).toMatchObject({ room: null, offer: "private" });
    // In the player's own room it is the room's share itself.
    expect(friendTarget(scenario("shareLiveRow"), "301", roomName).offer).toBe("watch");
    // In the player's own room before its share is in the state: the waiting card, never a lock.
    expect(friendTarget(scenario("room"), "301", roomName)).toMatchObject({
      room: "f00000000000000a1",
      n: null,
      offer: "watch",
    });
  });

  it("in the player's own crew room: their own share too, and the list's word until the room's", () => {
    const outside = scenario("shareCrewLive");
    const { entry, room } = stone(outside);
    const inside = scenario("shareLive");
    const roomView = inside.room;
    if (!roomView) throw new Error("no room");
    const here = {
      ...inside,
      room: { ...roomView, id: STONE, kind: "crew" as const, crew: entry.crew },
    };
    const targets = crewRoomTargets(here, entry.crew, room, entry.crewMax, "Taş Ocağı");
    expect(targets[0]).toMatchObject({ offer: "own", room: STONE });
    // Just in, the room's share state not there yet: who the list says is live, waiting.
    const justIn = { ...here, share: undefined };
    expect(crewRoomTargets(justIn, entry.crew, room, entry.crewMax, "Taş Ocağı")).toEqual([
      expect.objectContaining({ person: "302", offer: "watch", n: null }),
    ]);
  });

  it("never asks for a picture where the player may not go", () => {
    const base: LiveTarget = {
      person: "302",
      room: STONE,
      crew: "c",
      general: false,
      roomName: null,
      n: null,
      kind: null,
      watchers: null,
      th: null,
      offer: "join",
    };
    expect(asksPicture(base)).toBe(true);
    expect(asksPicture({ ...base, offer: "full" })).toBe(false);
    expect(asksPicture({ ...base, offer: "private", room: null })).toBe(false);
  });
});

describe("the card's picture", () => {
  const target: LiveTarget = {
    person: "301",
    room: "f00000000000000a1",
    crew: null,
    general: false,
    roomName: null,
    n: 1,
    kind: "game",
    watchers: 2,
    th: 1000,
    offer: "watch",
  };
  const image = (n: number): ShareThumb => ({
    kind: "image",
    dataUrl: `data:image/jpeg;base64,${n}`,
  });
  let answers: Array<(thumb: ShareThumb) => void>;

  beforeEach(() => {
    vi.useFakeTimers();
    forgetThumbs();
    answers = [];
    vi.stubGlobal(
      "Image",
      class {
        src = "";
        decode() {
          return Promise.resolve();
        }
      },
    );
    vi.spyOn(shareApi, "thumb").mockImplementation(
      () => new Promise<ShareThumb>((resolve) => answers.push(resolve)),
    );
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  const answer = async (i: number, thumb: ShareThumb) => {
    await act(async () => {
      answers[i]?.(thumb);
      await Promise.resolve();
    });
  };

  it("asks once when the card opens and shows the answer", async () => {
    const view = renderHook(({ open }) => useShareThumb(target, open), {
      initialProps: { open: false },
    });
    expect(shareApi.thumb).not.toHaveBeenCalled();
    view.rerender({ open: true });
    expect(shareApi.thumb).toHaveBeenCalledWith("f00000000000000a1", "301", 1);
    expect(view.result.current.thumb).toBeNull();
    await answer(0, image(1));
    expect(view.result.current.thumb).toEqual(image(1));
    // Nothing more while it stays open: no polling.
    await act(async () => {
      vi.advanceTimersByTime(5 * 60_000);
    });
    expect(shareApi.thumb).toHaveBeenCalledTimes(1);
  });

  it("drops an answer that comes after the card closed, and keeps it for the next opening", async () => {
    const view = renderHook(({ open }) => useShareThumb(target, open), {
      initialProps: { open: true },
    });
    view.rerender({ open: false });
    await answer(0, image(1));
    expect(view.result.current.thumb).toBeNull();
    view.rerender({ open: true });
    expect(shareApi.thumb).toHaveBeenCalledTimes(1);
    expect(view.result.current.thumb).toEqual(image(1));
    // Past the launcher's 20 s it asks again.
    view.rerender({ open: false });
    vi.setSystemTime(Date.now() + THUMB_KEEP_MS + 1);
    view.rerender({ open: true });
    expect(shareApi.thumb).toHaveBeenCalledTimes(2);
  });

  it("asks again when the room's picture time moves, keeping the last picture meanwhile", async () => {
    const view = renderHook(({ th }) => useShareThumb({ ...target, th }, true), {
      initialProps: { th: 1000 },
    });
    await answer(0, image(1));
    view.rerender({ th: 1060 });
    expect(shareApi.thumb).toHaveBeenCalledTimes(2);
    expect(view.result.current.thumb).toEqual(image(1));
    await answer(1, image(2));
    expect(view.result.current.thumb).toEqual(image(2));
    // A newer picture that did not come leaves the last one on screen.
    view.rerender({ th: 1120 });
    await answer(2, { kind: "none" });
    expect(view.result.current.thumb).toEqual(image(2));
  });

  it("asks nothing for a locked card", () => {
    renderHook(() => useShareThumb({ ...target, offer: "private", room: null }, true));
    expect(shareApi.thumb).not.toHaveBeenCalled();
  });
});
