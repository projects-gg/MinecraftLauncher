// @vitest-environment happy-dom
// "YAYINDA" and its card (SHARE-LIVE): resting on the tag opens the card after 350 ms with the
// stream's picture, what it shows and "İzle"; a room the player may not join shows a lock and one
// line and asks for no picture; "Katıl ve izle" joins the crew room and watches once it is there; a
// press from the keyboard takes the focus into the card.
import { setAppReducedMotion, TooltipProvider } from "@pl/ui";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { resetBadgeIntent } from "@/features/social/badge-intent";
import { useVoiceUi } from "@/features/voice/store";
import { voiceApi } from "@/lib/api/voice-api";
import { createVoiceMock } from "@/lib/api/voice-mock";
import { HIDDEN_VOICE_STATE, VOICE_OK, type VoiceState } from "@/lib/api/voice-types";
import { shareApi } from "@/share/api";
import { forgetThumbs, type LiveTarget, liveTargetIn, ownTarget } from "./live";
import { LiveTag, LiveTagButton, useLivePreview } from "./live-card";

// The card fades. happy-dom leaves a cancelled animation's `finished` rejection unhandled (a browser
// marks it handled), so a fade cut short failed the run; here it is handled (rule-assist.test.tsx).
const animate = Element.prototype.animate;
beforeAll(() => {
  setAppReducedMotion(true);
  Element.prototype.animate = function handledAnimate(
    this: Element,
    keyframes: Keyframe[] | PropertyIndexedKeyframes | null,
    options?: number | KeyframeAnimationOptions,
  ) {
    const animation = animate.call(this, keyframes, options);
    const cancel = animation.cancel;
    animation.cancel = function handledCancel(this: Animation) {
      this.finished.catch(() => undefined);
      cancel.call(this);
    };
    return animation;
  };
});
afterAll(() => {
  setAppReducedMotion(false);
  Element.prototype.animate = animate;
});
beforeEach(() => {
  vi.useFakeTimers();
  resetBadgeIntent();
  forgetThumbs();
  vi.stubGlobal(
    "Image",
    class {
      src = "";
      decode() {
        return Promise.resolve();
      }
    },
  );
});
afterEach(() => {
  cleanup();
  resetBadgeIntent();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
  useVoiceUi.setState({ state: HIDDEN_VOICE_STATE });
});

const PROFILES = [
  { id: "301", name: "GeceKurdu_", avatar: "preset:fox" },
  { id: "302", name: "KorAtesi", avatar: "preset:knight" },
];

function scenario(name: string): VoiceState {
  const state = createVoiceMock({ scenario: name, timers: false }).current();
  return { ...state, profiles: [...(state.profiles ?? []), ...PROFILES] };
}

function show(state: VoiceState, node: ReactNode) {
  act(() => useVoiceUi.getState().setState(state));
  return render(<TooltipProvider>{node}</TooltipProvider>);
}

const PLACE = { side: "bottom" } as const;

/** The mouse comes to rest on the tag; its card opens after 350 ms. */
async function rest(tag: HTMLElement) {
  fireEvent.pointerEnter(tag, { pointerType: "mouse", clientX: 10, clientY: 10 });
  await act(async () => {
    vi.advanceTimersByTime(360);
  });
  await act(async () => {
    await Promise.resolve();
  });
}

function crewTarget(state: VoiceState): LiveTarget {
  const entry = state.crews[0];
  const room = entry?.rooms.find((r) => r.id === "c00000000000000b2");
  if (!entry || !room) throw new Error("no room");
  const full = room.people.length >= (room.max ?? entry.crewMax);
  return {
    person: "302",
    room: room.id,
    crew: entry.crew,
    general: false,
    roomName: "Taş Ocağı",
    n: null,
    kind: null,
    watchers: null,
    th: null,
    offer: full ? "full" : "join",
  };
}

describe("the live tag's card", () => {
  it("opens on a resting pointer with the picture, what is shared and İzle", async () => {
    const thumb = vi
      .spyOn(shareApi, "thumb")
      .mockResolvedValue({ kind: "image", dataUrl: "data:image/jpeg;base64,AA" });
    const watch = vi.spyOn(shareApi, "watch").mockResolvedValue(null);
    const state = scenario("shareLiveRow");
    const target = liveTargetIn(state, "301");
    if (!target) throw new Error("no target");
    show(state, <LiveTagButton targets={[target]} placement={PLACE} />);
    const tag = screen.getByRole("button", { name: "GeceKurdu_ yayında" });
    expect(tag.textContent).toBe("YAYINDA");
    fireEvent.pointerEnter(tag, { pointerType: "mouse" });
    await act(async () => {
      vi.advanceTimersByTime(200);
    });
    // Passing over it opens nothing.
    expect(screen.queryByRole("dialog")).toBeNull();
    await rest(tag);
    expect(screen.getByRole("dialog", { name: "GeceKurdu_ yayında" })).toBeTruthy();
    expect(thumb).toHaveBeenCalledWith("f00000000000000a1", "301", 1);
    expect(screen.getByRole("img", { name: "GeceKurdu_ yayınından son görüntü" })).toBeTruthy();
    expect(screen.getByText("Oyun, 2 kişi izliyor")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "GeceKurdu_ yayınını izle" }));
    expect(watch).toHaveBeenCalledWith("301", 1);
    await act(async () => {
      vi.advanceTimersByTime(300);
    });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows a lock and one line, never a picture, where the player may not go", async () => {
    const thumb = vi.spyOn(shareApi, "thumb");
    const state = scenario("shareCrewLiveFull");
    show(state, <LiveTagButton targets={[crewTarget(state)]} placement={PLACE} />);
    await rest(screen.getByRole("button", { name: "KorAtesi yayında" }));
    expect(screen.getByText("Oda dolu.")).toBeTruthy();
    expect(screen.queryByRole("img", { name: /son görüntü/ })).toBeNull();
    expect(screen.queryByRole("button", { name: "Katıl ve izle" })).toBeNull();
    expect(thumb).not.toHaveBeenCalled();
    cleanup();
    const call: LiveTarget = { ...crewTarget(state), room: null, crew: null, offer: "private" };
    show(state, <LiveTagButton targets={[call]} placement={PLACE} />);
    await rest(screen.getByRole("button", { name: "KorAtesi yayında" }));
    expect(screen.getByText("Yalnız odadakiler izleyebilir.")).toBeTruthy();
    expect(thumb).not.toHaveBeenCalled();
  });

  it("locks the card when the friends service says the player may not join", async () => {
    vi.spyOn(shareApi, "thumb").mockResolvedValue({ kind: "locked" });
    const state = scenario("shareCrewLive");
    show(state, <LiveTagButton targets={[crewTarget(state)]} placement={PLACE} />);
    await rest(screen.getByRole("button", { name: "KorAtesi yayında" }));
    expect(screen.getByText("Bu odaya katılamazsın.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Katıl ve izle" })).toBeNull();
  });

  it("joins the crew room and watches the share once the room lists it", async () => {
    vi.spyOn(shareApi, "thumb").mockResolvedValue({ kind: "none" });
    const watch = vi.spyOn(shareApi, "watch").mockResolvedValue(null);
    const join = vi.spyOn(voiceApi, "joinCrew").mockResolvedValue(VOICE_OK);
    const state = scenario("shareCrewLive");
    const target = crewTarget(state);
    show(state, <LiveTagButton targets={[target]} placement={PLACE} />);
    await rest(screen.getByRole("button", { name: "KorAtesi yayında" }));
    // No picture yet: the card waits calmly, its action is there.
    expect(screen.getByText("Taş Ocağı")).toBeTruthy();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Katıl ve izle" }));
      await Promise.resolve();
    });
    expect(join).toHaveBeenCalledWith(target.crew, "c00000000000000b2");
    expect(watch).not.toHaveBeenCalled();
    // The room's state comes with KorAtesi's share: watched once.
    const inside = scenario("shareLiveRow");
    const room = inside.room;
    const share = inside.share;
    const first = share?.live[0];
    if (!room || !share || !first) throw new Error("no room");
    act(() =>
      useVoiceUi.getState().setState({
        ...inside,
        room: { ...room, id: "c00000000000000b2", kind: "crew", crew: target.crew },
        share: { ...share, live: [{ ...first, person: "302", n: 2 }] },
      }),
    );
    expect(watch).toHaveBeenCalledWith("302", 2);
    act(() => useVoiceUi.getState().setState({ ...useVoiceUi.getState().state }));
    expect(watch).toHaveBeenCalledTimes(1);
  });

  it("says what is shared in one line, lower case after the comma", async () => {
    vi.spyOn(shareApi, "thumb").mockResolvedValue({ kind: "none" });
    const watching = scenario("shareWatch");
    const watched = liveTargetIn(watching, "301");
    if (!watched) throw new Error("no target");
    show(watching, <LiveTagButton targets={[watched]} placement={PLACE} />);
    await rest(screen.getByRole("button", { name: "GeceKurdu_ yayında" }));
    expect(screen.getByText("Oyun, izliyorsun")).toBeTruthy();
    // Already watched: no "İzle".
    expect(screen.queryByRole("button", { name: "GeceKurdu_ yayınını izle" })).toBeNull();
    cleanup();
    resetBadgeIntent();
    const resting = scenario("shareRest");
    const own = ownTarget(resting);
    if (!own) throw new Error("no own share");
    show(resting, <LiveTagButton targets={[own]} placement={PLACE} />);
    await rest(screen.getByRole("button", { name: /yayında/ }));
    expect(screen.getByText("Pencere, kimse izlemiyor")).toBeTruthy();
  });

  it("opens and closes with a press on its own tag, and a press on a row's tag is the tag's", async () => {
    vi.spyOn(shareApi, "thumb").mockResolvedValue({ kind: "none" });
    const state = scenario("shareLiveRow");
    const target = liveTargetIn(state, "301");
    if (!target) throw new Error("no target");
    show(state, <LiveTagButton targets={[target]} placement={PLACE} />);
    const tag = screen.getByRole("button", { name: "GeceKurdu_ yayında" });
    await act(async () => {
      fireEvent.pointerDown(tag, { pointerType: "mouse", button: 0 });
      fireEvent.click(tag, { detail: 1 });
      await Promise.resolve();
    });
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(tag.getAttribute("aria-expanded")).toBe("true");
    await act(async () => {
      fireEvent.pointerDown(tag, { pointerType: "mouse", button: 0 });
      fireEvent.click(tag, { detail: 1 });
    });
    await act(async () => {
      vi.advanceTimersByTime(300);
    });
    expect(screen.queryByRole("dialog")).toBeNull();
    cleanup();
    resetBadgeIntent();
    // In a row: the row's own press (its menu) never hears a press on the tag.
    const rowPress = vi.fn();
    function Row() {
      const preview = useLivePreview([target as LiveTarget], PLACE);
      return (
        <div>
          <button type="button" aria-label="GeceKurdu_" onPointerDown={rowPress} onClick={rowPress}>
            <span {...preview.tagProps} data-testid="tag">
              <LiveTag />
            </span>
          </button>
          {preview.card}
        </div>
      );
    }
    show(state, <Row />);
    const inRow = screen.getByTestId("tag");
    await act(async () => {
      fireEvent.pointerDown(inRow, { pointerType: "mouse", button: 0 });
      fireEvent.click(inRow, { detail: 1 });
      await Promise.resolve();
    });
    expect(rowPress).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("takes the focus into the card from the keyboard", async () => {
    vi.spyOn(shareApi, "thumb").mockResolvedValue({ kind: "none" });
    const state = scenario("shareLiveRow");
    const target = liveTargetIn(state, "301");
    if (!target) throw new Error("no target");
    show(state, <LiveTagButton targets={[target]} placement={PLACE} />);
    const tag = screen.getByRole("button", { name: "GeceKurdu_ yayında" });
    tag.focus();
    // Enter on a button is a click without a pointer (`detail` 0).
    await act(async () => {
      fireEvent.click(tag, { detail: 0 });
      await Promise.resolve();
    });
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(document.activeElement?.textContent).toBe("İzle");
    // Esc closes it and the focus comes back to the tag.
    act(() => {
      fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    });
    await act(async () => {
      vi.advanceTimersByTime(300);
    });
    expect(screen.queryByRole("dialog")).toBeNull();
    // The layer gives the focus back a moment after it went (Radix's focus scope).
    await act(async () => {
      vi.advanceTimersByTime(10);
    });
    expect(document.activeElement).toBe(tag);
  });

  it("goes into the card from a row with the right arrow", async () => {
    vi.spyOn(shareApi, "thumb").mockResolvedValue({ kind: "none" });
    const state = scenario("shareLiveRow");
    const target = liveTargetIn(state, "301");
    if (!target) throw new Error("no target");
    function Row() {
      const preview = useLivePreview([target as LiveTarget], PLACE);
      return (
        <div>
          <button
            type="button"
            aria-label="GeceKurdu_"
            onKeyDown={(event) => preview.keyIn(event)}
            {...preview.focusProps}
          >
            <span {...preview.tagProps}>
              <LiveTag />
            </span>
          </button>
          {preview.card}
        </div>
      );
    }
    show(state, <Row />);
    const row = screen.getByRole("button", { name: "GeceKurdu_" });
    row.focus();
    await act(async () => {
      fireEvent.keyDown(row, { key: "ArrowRight" });
      await Promise.resolve();
    });
    expect(screen.getByRole("dialog", { name: "GeceKurdu_ yayında" })).toBeTruthy();
    expect(document.activeElement?.textContent).toBe("İzle");
  });
});
