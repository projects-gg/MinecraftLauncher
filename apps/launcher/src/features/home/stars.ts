// Home's GitHub star line (STARS): asked once, after the launcher has proved itself. The launcher
// counts the games it brought on screen (each launch once; games an earlier start left running are
// not its launches) and keeps the count on this computer only (`pl.stars`). The line comes when at
// least ten games opened over at least a week, never while a game is prepared or played, never
// before the first-start setup is done. Its first appearance is remembered: it stays for this
// session until the player answers ("Yıldız ver" or ×, both for good) and never comes back after.
import { create } from "zustand";
import type { RunningGame } from "@/stores/launch";

export const STORAGE_KEY = "pl.stars";
/** Games brought on screen before the line may come. */
export const STAR_LAUNCHES = 10;
/** Time since the first counted game before the line may come. */
export const STAR_WAIT_MS = 7 * 24 * 60 * 60 * 1000;
/** The repository the line and Settings → Hakkında open. */
export const SOURCE_URL = "https://github.com/projects-gg/MinecraftLauncher";

export interface StarState {
  /** Games this launcher brought on screen (counting stops once the line has shown). */
  launches: number;
  /** When the first of them came on screen. */
  firstAt: number | null;
  /** When the line first appeared; it never comes back in a later session. */
  shownAt: number | null;
  /** The player answered ("Yıldız ver" or ×). */
  done: boolean;
}

export const EMPTY: StarState = { launches: 0, firstAt: null, shownAt: null, done: false };

const time = (value: unknown): number | null =>
  typeof value === "number" && Number.isFinite(value) && value > 0 ? value : null;

/** The saved state; anything unreadable starts over. */
export function parseStars(raw: string | null): StarState {
  if (!raw) return EMPTY;
  try {
    const value: unknown = JSON.parse(raw);
    if (!value || typeof value !== "object") return EMPTY;
    const saved = value as Record<string, unknown>;
    const launches = saved.launches;
    return {
      launches:
        typeof launches === "number" && Number.isInteger(launches) && launches > 0 ? launches : 0,
      firstAt: time(saved.firstAt),
      shownAt: time(saved.shownAt),
      done: saved.done === true,
    };
  } catch {
    return EMPTY;
  }
}

/** One more game on screen. Nothing changes once the line has shown or been answered. */
export function countLaunch(state: StarState, now: number): StarState {
  if (state.done || state.shownAt !== null) return state;
  return { ...state, launches: state.launches + 1, firstAt: state.firstAt ?? now };
}

/** Whether the line may come for the first time. */
export function starLineDue(state: StarState, now: number): boolean {
  return (
    !state.done &&
    state.shownAt === null &&
    state.launches >= STAR_LAUNCHES &&
    state.firstAt !== null &&
    now - state.firstAt >= STAR_WAIT_MS
  );
}

export interface StarLineContext {
  now: number;
  /** A game is being prepared, opening or played. */
  busy: boolean;
  /** The first-start setup is done. */
  onboarded: boolean;
  /** The line already appeared in this session (it stays until the player answers). */
  shownThisSession: boolean;
}

/** Whether Home shows the line now. */
export function starLineVisible(state: StarState, context: StarLineContext): boolean {
  if (state.done || context.busy || !context.onboarded) return false;
  return context.shownThisSession || starLineDue(state, context.now);
}

/** Runs of the page's own launches whose game is on screen and not counted yet. */
export function newShownRuns(
  games: readonly RunningGame[],
  counted: ReadonlySet<number>,
): number[] {
  return games
    .filter((game) => game.shown && game.run > 0 && !counted.has(game.run))
    .map((game) => game.run);
}

function load(): StarState {
  try {
    return parseStars(localStorage.getItem(STORAGE_KEY));
  } catch {
    return EMPTY;
  }
}

function save(state: StarState) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // Storage is full or blocked: the line keeps its state for this session.
  }
}

interface StarsStore {
  state: StarState;
  /** The line appeared in this session. */
  session: boolean;
  count: (now: number) => void;
  /** The line is on screen: remembered at once, so it never returns in a later session. */
  show: (now: number) => void;
  /** "Yıldız ver" or ×: the line leaves for good. */
  finish: () => void;
}

export const useStars = create<StarsStore>()((set, get) => ({
  state: load(),
  session: false,
  count(now) {
    const before = get().state;
    const state = countLaunch(before, now);
    if (state === before) return;
    save(state);
    set({ state });
  },
  show(now) {
    const before = get().state;
    if (get().session && before.shownAt !== null) return;
    const state = before.shownAt === null ? { ...before, shownAt: now } : before;
    if (state !== before) save(state);
    set({ state, session: true });
  },
  finish() {
    const before = get().state;
    if (before.done) return;
    const state = { ...before, done: true, shownAt: before.shownAt ?? Date.now() };
    save(state);
    set({ state });
  },
}));
