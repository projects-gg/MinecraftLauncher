// When a crew tag's card opens and closes (TEAM6 §4.3): one intent for every chip, so at most one
// card is open and nothing runs while nobody points at a chip. The pointer resting on a chip for
// 350 ms opens its card (as tooltips do); reaching another chip within 150 ms of a card opens the
// next one at once, without its entrance; leaving a chip or its card closes it 150 ms later,
// unless the pointer is on its way to the card (the corridor of badges.ts) or back on either. A
// press pins the card until a press outside, Esc or the chip again; a card whose menu is open or
// whose request is under way never closes by itself. Keyboard focus on a chip opens it as a
// preview after the same 350 ms (the focus stays on the chip); Enter moves the focus into it.
import { create } from "zustand";
import { corridor, inTriangle, type Point } from "./badges";

/** A resting pointer (or the keyboard's focus) opens a card this late, as tooltips open. */
export const OPEN_DELAY_MS = 350;
/** A card opened this soon after another one closed comes at once, without its entrance. */
export const SKIP_MS = 150;
/** Leaving a chip or its card closes it this late (the pointer may be on its way). */
export const GRACE_MS = 150;

export interface BadgeIntent {
  /** The chip whose card is open (each chip has its own id), or null. */
  open: string | null;
  /** It opened without its entrance (another card was open a moment before). */
  instant: boolean;
  /** Opened by a press: it stays until a press outside, Esc or the chip again. */
  pinned: boolean;
  /** The keyboard asked for the focus in a chip's card (Enter on it): the card takes it once per
   * `n`. Null when the card opened under the pointer (the focus stays where it is). */
  focus: { id: string; n: number } | null;
}

const CLOSED: Omit<BadgeIntent, "focus"> = { open: null, instant: false, pinned: false };

export const useBadgeIntent = create<BadgeIntent>()(() => ({ ...CLOSED, focus: null }));

let openTimer: ReturnType<typeof setTimeout> | null = null;
let pending: string | null = null;
let closeTimer: ReturnType<typeof setTimeout> | null = null;
let closedAt = Number.NEGATIVE_INFINITY;
let focusRequests = 0;
/** Cards that must not close by themselves now (their menu is open, a request is under way). */
const holds = new Set<string>();
/** The cards on screen, for the corridor and for where the focus goes when one closes. */
const cards = new Map<string, HTMLElement>();
/** The card last closed, and whether the focus was in it then (it goes back to the chip). */
let lastClosed: { id: string; focused: boolean } | null = null;
let stopCorridor: (() => void) | null = null;

function cancelOpen() {
  if (openTimer !== null) clearTimeout(openTimer);
  openTimer = null;
  pending = null;
}

function cancelClose() {
  if (closeTimer !== null) clearTimeout(closeTimer);
  closeTimer = null;
}

function endCorridor() {
  stopCorridor?.();
  stopCorridor = null;
}

/** Opens a chip's card: at once (`pinned` by a press; `keys`: the focus goes into it). */
export function openCard(
  id: string,
  how: { pinned?: boolean; keys?: boolean } = {},
  nowMs: number = Date.now(),
) {
  cancelOpen();
  cancelClose();
  endCorridor();
  const state = useBadgeIntent.getState();
  const already = state.open === id;
  if (how.keys) focusRequests += 1;
  useBadgeIntent.setState({
    open: id,
    instant: already ? state.instant : state.open !== null || nowMs - closedAt < SKIP_MS,
    pinned: Boolean(how.pinned) || (already && state.pinned),
    focus: how.keys ? { id, n: focusRequests } : already ? state.focus : null,
  });
}

/** Closes the open card (only `id`'s when given). */
export function closeCard(id?: string, nowMs: number = Date.now()) {
  const state = useBadgeIntent.getState();
  if (state.open === null || (id !== undefined && state.open !== id)) {
    if (id !== undefined && pending === id) cancelOpen();
    return;
  }
  cancelOpen();
  cancelClose();
  endCorridor();
  holds.delete(state.open);
  closedAt = nowMs;
  const card = cards.get(state.open);
  const active = typeof document === "undefined" ? null : document.activeElement;
  lastClosed = { id: state.open, focused: Boolean(card && active && card.contains(active)) };
  useBadgeIntent.setState(CLOSED);
}

/** Whether `id`'s card had the focus when it closed (Esc, "Ekibi aç"…): it goes back to the chip. */
export function closedWithFocus(id: string): boolean {
  return lastClosed?.id === id && lastClosed.focused;
}

function scheduleClose(id: string) {
  cancelClose();
  closeTimer = setTimeout(() => {
    closeTimer = null;
    closeCard(id);
  }, GRACE_MS);
}

/** Whether a card may close by itself now (open, not pinned, not held). */
function loose(id: string): boolean {
  const state = useBadgeIntent.getState();
  return state.open === id && !state.pinned && !holds.has(id);
}

/** The pointer (a mouse) came onto a chip. */
export function hoverChip(id: string, nowMs: number = Date.now()) {
  const state = useBadgeIntent.getState();
  if (state.open === id) {
    cancelClose();
    endCorridor();
    return;
  }
  // A pinned card stays where it was put.
  if (state.open !== null && state.pinned) return;
  cancelClose();
  endCorridor();
  if (state.open !== null || nowMs - closedAt < SKIP_MS) {
    openCard(id, {}, nowMs);
    return;
  }
  if (pending === id) return;
  cancelOpen();
  pending = id;
  openTimer = setTimeout(() => {
    openTimer = null;
    pending = null;
    openCard(id);
  }, OPEN_DELAY_MS);
}

/** The pointer left a chip, at `from` (it may be on its way to the card). */
export function leaveChip(id: string, from?: Point) {
  if (pending === id) cancelOpen();
  if (!loose(id)) return;
  scheduleClose(id);
  if (from) watchCorridor(id, from);
}

/** The pointer came onto the open card. */
export function enterCard(id: string) {
  if (useBadgeIntent.getState().open !== id) return;
  cancelClose();
  endCorridor();
}

/** The pointer left the open card. */
export function leaveCard(id: string) {
  if (loose(id)) scheduleClose(id);
}

/** A press on a chip (a click, a tap, Enter or Space): pins its card, or closes it when it was
 * pinned already. From the keyboard the focus goes into the card. */
export function pressChip(id: string, keys: boolean) {
  const state = useBadgeIntent.getState();
  if (state.open === id && state.pinned && !keys) {
    closeCard(id);
    return;
  }
  openCard(id, { pinned: true, keys });
}

/** The keyboard's focus came onto a chip: its card opens as a preview after the delay. */
export function focusChip(id: string) {
  const state = useBadgeIntent.getState();
  if (state.open !== null || pending === id) return;
  cancelOpen();
  pending = id;
  openTimer = setTimeout(() => {
    openTimer = null;
    pending = null;
    openCard(id);
  }, OPEN_DELAY_MS);
}

/** The focus left a chip before its preview came. */
export function blurChip(id: string) {
  if (pending === id) cancelOpen();
}

/** Keeps a card open while its menu is open or its request is under way. */
export function holdCard(id: string, on: boolean) {
  if (on) {
    holds.add(id);
    cancelClose();
  } else {
    holds.delete(id);
  }
}

/** The open card's element, for the corridor (null when it goes). */
export function registerCard(id: string, element: HTMLElement | null) {
  if (element) cards.set(id, element);
  else cards.delete(id);
}

/**
 * While the pointer moves inside the corridor from where it left the chip to the card, the grace
 * starts again with each move; once it is outside, the grace runs out. Listens only until the
 * card closes or the pointer reaches it.
 */
function watchCorridor(id: string, from: Point) {
  endCorridor();
  const card = cards.get(id);
  if (!card || typeof document === "undefined") return;
  const box = card.getBoundingClientRect();
  const path = corridor(from, box);
  if (!path) return;
  const onMove = (event: PointerEvent) => {
    if (inTriangle({ x: event.clientX, y: event.clientY }, path)) scheduleClose(id);
  };
  document.addEventListener("pointermove", onMove, { passive: true });
  stopCorridor = () => document.removeEventListener("pointermove", onMove);
}

/** Tests start from nothing open. */
export function resetBadgeIntent() {
  cancelOpen();
  cancelClose();
  endCorridor();
  holds.clear();
  cards.clear();
  closedAt = Number.NEGATIVE_INFINITY;
  lastClosed = null;
  focusRequests = 0;
  useBadgeIntent.setState({ ...CLOSED, focus: null });
}
