// @vitest-environment happy-dom
// SQOL §1.3 "Okunmamış": the previews over the stage fade 6 s after each came, wait while the
// pointer rests on them, and go at once with Esc; out of the screen reader's reading (the room
// chat's host tells them) and out of the Tab order.
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { RoomChatPeek } from "./peek";
import { PEEK_MS } from "./rules";
import { EMPTY_ROOM_CHAT, useRoomChat } from "./store";

// The stage is on the page and the panel's chat is the room's (hoisted over the imports).
vi.mock("../stage-mode", () => ({ useStageMode: () => "page" }));
vi.mock("./panel-chat", () => ({ usePanelChat: () => ({ kind: "room" }) }));

const ROOM = "g0123";

// The previews fade. happy-dom leaves a cancelled animation's `finished` rejection unhandled (a
// browser marks it handled, as the Web Animations spec says), so a fade cut short by a test's end
// failed the run with "AbortError: The animation was canceled" (rule-assist.test.tsx does the same).
const animate = Element.prototype.animate;
beforeAll(() => {
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
  Element.prototype.animate = animate;
});

beforeEach(() => {
  vi.useFakeTimers();
  const now = Date.now();
  useRoomChat.setState({
    ...EMPTY_ROOM_CHAT,
    room: ROOM,
    peeks: [
      { key: "r:a", from: "301", text: "Kapıdayım, gelin", until: now + PEEK_MS },
      { key: "r:b", from: "303", text: "Geliyorum", until: now + PEEK_MS + 1_500 },
    ],
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  useRoomChat.setState({ ...EMPTY_ROOM_CHAT });
});

const keys = () => useRoomChat.getState().peeks.map((p) => p.key);

describe("RoomChatPeek", () => {
  it("shows the previews out of reading and out of the Tab order", () => {
    const view = render(<RoomChatPeek />);
    const list = view.container.firstElementChild;
    expect(list?.getAttribute("aria-hidden")).toBe("true");
    const buttons = view.container.querySelectorAll("button");
    expect(buttons).toHaveLength(2);
    for (const button of buttons) expect(button.getAttribute("tabindex")).toBe("-1");
    expect(view.container.textContent).toContain("Kapıdayım, gelin");
  });

  it("lets each go on its own time", () => {
    render(<RoomChatPeek />);
    act(() => vi.advanceTimersByTime(PEEK_MS - 100));
    expect(keys()).toEqual(["r:a", "r:b"]);
    act(() => vi.advanceTimersByTime(200));
    expect(keys()).toEqual(["r:b"]);
    act(() => vi.advanceTimersByTime(1_500));
    expect(keys()).toEqual([]);
  });

  it("holds them while the pointer rests on them", () => {
    const view = render(<RoomChatPeek />);
    const list = view.container.firstElementChild as HTMLElement;
    act(() => vi.advanceTimersByTime(1_000));
    fireEvent.pointerEnter(list);
    act(() => vi.advanceTimersByTime(PEEK_MS * 2));
    expect(keys()).toEqual(["r:a", "r:b"]);
    fireEvent.pointerLeave(list);
    // Their time moved on by the rest: the first still has its 5 s.
    act(() => vi.advanceTimersByTime(PEEK_MS - 1_100));
    expect(keys()).toEqual(["r:a", "r:b"]);
    act(() => vi.advanceTimersByTime(200));
    expect(keys()).toEqual(["r:b"]);
  });

  it("goes with Esc, which it keeps: a full-screen stage under it leaves on the next one", () => {
    render(<RoomChatPeek />);
    // `false`: the key was taken (SQOL-U1's full-screen Esc yields to a taken key).
    expect(fireEvent.keyDown(document, { key: "Escape" })).toBe(false);
    expect(keys()).toEqual([]);
    // Nothing left to take away: the next Esc goes on to whatever is under them.
    expect(fireEvent.keyDown(document, { key: "Escape" })).toBe(true);
  });

  it("leaves an Esc another layer took", () => {
    render(<RoomChatPeek />);
    // A menu or dialog over the page takes its Esc first (Radix listens on the way in).
    const take = (event: KeyboardEvent) => event.preventDefault();
    window.addEventListener("keydown", take, true);
    try {
      fireEvent.keyDown(document, { key: "Escape" });
    } finally {
      window.removeEventListener("keydown", take, true);
    }
    expect(keys()).toHaveLength(2);
  });
});
