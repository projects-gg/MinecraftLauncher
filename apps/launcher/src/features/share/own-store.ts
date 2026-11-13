// The player's own live shares on the page (SQOL, sqol.md §1.2): the picker's "Ekranı Değiştir"
// mode, the stage's own tile (the card's picture asks for it), changes shown at once before the
// launcher answers (taken back when it refuses), and the minute a share whose source closed waits
// for another one. The shares themselves are voice chat's (`VoiceState.share.mine`, sqol-r.md).
import { useMemo } from "react";
import { create } from "zustand";
import { useVoiceUi } from "@/features/voice/store";
import type { VoiceMyShare } from "@/share/types";
import { useShare, useShareUi } from "./store";

/** What a change shows before the launcher's state says it. */
export type OwnPatch = Partial<
  Pick<
    VoiceMyShare,
    | "kind"
    | "label"
    | "source"
    | "preset"
    | "fps"
    | "audio"
    | "motion"
    | "cursor"
    | "thumbs"
    | "st"
    | "hold"
    | "holdUntil"
  >
>;

interface Pending {
  seq: number;
  n: number;
  patch: OwnPatch;
  /** What the state said of the patched fields when it was made (it moves: the answer came). */
  base: OwnPatch;
  /** The launcher took it: it goes when the state moves on its fields (or soon, `SETTLED_MS`). */
  settled: boolean;
}

/** Why a share's source went away, as its notice said (`sourceClosed`). */
export type LostWhy = "closed" | "gameClosed";

/** A share whose source went away, as a notice said (before the state says it too). */
export interface LostMark {
  why: LostWhy;
  /** When it stops by itself (Unix seconds). */
  until: number | null;
  /** The state showed it held since: its going live again ends the mark. */
  sawHold: boolean;
}

interface OwnUi {
  /** "Ekranı Değiştir": the share the picker switches (null: the picker starts a new share). */
  switching: number | null;
  /** The stage shows the player's own share (its picture pressed on the card). */
  tile: number | null;
  pending: Pending[];
  lost: Record<number, LostMark>;
}

export const useOwnUi = create<OwnUi>()(() => ({
  switching: null,
  tile: null,
  pending: [],
  lost: {},
}));

/** How long a taken change waits for the launcher's state before the state alone shows. */
export const SETTLED_MS = 4_000;

/** How long a share whose source went away waits for another one (sqol.md §1.2). */
export const LOST_WAIT_S = 60;

// --- "Ekranı Değiştir" ------------------------------------------------------------------------

/** Opens the picker to switch share `n` (it goes on meanwhile). */
export function openSwitch(n: number) {
  useOwnUi.setState({ switching: n });
  useShareUi.getState().openPicker();
}

/** The picker closed: its next opening starts a new share again. */
export function endSwitch() {
  if (useOwnUi.getState().switching !== null) useOwnUi.setState({ switching: null });
}

/** "Ekranını Paylaş": the picker for a new share (never a switch the last one was for). */
export function openNewShare() {
  endSwitch();
  useShareUi.getState().openPicker();
}

// --- the stage's own tile (SQOL-U1 draws it) ---------------------------------------------------

/** The card's picture: the stage shows the player's own share `n` (its local picture). */
export function showOwnTile(n: number) {
  useOwnUi.setState({ tile: n });
}

export function hideOwnTile() {
  if (useOwnUi.getState().tile !== null) useOwnUi.setState({ tile: null });
}

/** The own share the stage is asked to show (null: none; it is cleared when the share ends). */
export function useOwnTile(): number | null {
  return useOwnUi((s) => s.tile);
}

// --- changes before the launcher's answer -------------------------------------------------------

let seq = 0;
const settleTimers = new Map<number, ReturnType<typeof setTimeout>>();
/** What runs once the state shows the room took a settled change (it says so; never on a refusal). */
const whenTaken = new Map<number, (now: VoiceMyShare) => void>();

function forgetTimer(which: number) {
  const timer = settleTimers.get(which);
  if (timer !== undefined) clearTimeout(timer);
  settleTimers.delete(which);
}

/** A change left the page: its timer goes, and what it would have said with it. */
function forget(which: number) {
  forgetTimer(which);
  whenTaken.delete(which);
}

/** The state's values of `patch`'s fields on share `n` now. */
function baseOf(n: number, patch: OwnPatch): OwnPatch {
  const now = useVoiceUi.getState().state.share?.mine.find((m) => m.n === n);
  const base: Record<string, unknown> = {};
  for (const key of Object.keys(patch)) base[key] = now?.[key as keyof OwnPatch];
  return base as OwnPatch;
}

const sameValue = (a: unknown, b: unknown) =>
  a === b || (a !== null && typeof a === "object" && JSON.stringify(a) === JSON.stringify(b));

/**
 * The state moved on a patch's fields since it was made: the launcher's answer is in (any other
 * change of the voice state, someone joining or a mute, keeps the patch).
 */
function answered(entry: Pending, now: VoiceMyShare): boolean {
  return Object.keys(entry.patch).some(
    (key) => !sameValue(now[key as keyof OwnPatch], entry.base[key as keyof OwnPatch]),
  );
}

/** Shows `patch` on share `n` at once; returns its number for `settlePatch`/`dropPatch`. */
export function applyPatch(n: number, patch: OwnPatch): number {
  seq += 1;
  const mine = seq;
  const base = baseOf(n, patch);
  useOwnUi.setState((s) => ({
    pending: [...s.pending, { seq: mine, n, patch, base, settled: false }],
  }));
  return mine;
}

/** What the room decides on a live change (`share_update` asks it first); the rest is local. */
const ROOM_FIELDS: ReadonlyArray<keyof OwnPatch> = ["preset", "fps", "audio"];

/** The change asks the room (its quality, frame rate or sound). */
export function asksRoom(patch: OwnPatch): boolean {
  return ROOM_FIELDS.some((key) => patch[key] !== undefined);
}

/** Changes the room refused before the launcher's own answer came (their `seq`). */
const refusedEarly = new Set<number>();

/**
 * Whether the room refused change `which` before the launcher answered (`share_update` resolves
 * after it asked the room, and the room's notice can come first): it must neither settle nor be
 * said as done. Asked once.
 */
export function takeRefused(which: number): boolean {
  return refusedEarly.delete(which);
}

/**
 * Change `entry`'s `fields` go back at once, whether the launcher answered yet or not (the rest of
 * it stays: its picture, cursor or small picture). Returns what went back.
 */
function takeBack(entry: Pending, fields: ReadonlyArray<keyof OwnPatch>): OwnPatch {
  const { pending } = useOwnUi.getState();
  if (!entry.settled) refusedEarly.add(entry.seq);
  // Refused: never said as done, whatever stays of it.
  whenTaken.delete(entry.seq);
  const pick = (patch: OwnPatch, refused: boolean) =>
    Object.fromEntries(
      Object.entries(patch).filter(([key]) => fields.includes(key as keyof OwnPatch) === refused),
    ) as OwnPatch;
  const refused = pick(entry.patch, true);
  const kept = pick(entry.patch, false);
  if (Object.keys(kept).length === 0) {
    forget(entry.seq);
    useOwnUi.setState({ pending: pending.filter((p) => p.seq !== entry.seq) });
  } else {
    useOwnUi.setState({
      pending: pending.map((p) =>
        p.seq === entry.seq ? { ...p, patch: kept, base: pick(p.base, false) } : p,
      ),
    });
  }
  return refused;
}

/**
 * The room refused share `n`'s live change that asked `asked` (its notice names both): that
 * change's fields go back, on that share only. It is the change asking the same values, else the
 * oldest asking those fields (the room answers in order). Returns what went back, null when
 * nothing waited for that answer any more.
 */
export function dropRefusedChange(n: number, asked: OwnPatch): OwnPatch | null {
  const fields = ROOM_FIELDS.filter((key) => asked[key] !== undefined);
  const waiting = useOwnUi
    .getState()
    .pending.filter((p) => p.n === n && fields.some((key) => p.patch[key] !== undefined));
  const entry =
    waiting.find((p) => fields.every((key) => sameValue(p.patch[key], asked[key]))) ?? waiting[0];
  return entry ? takeBack(entry, fields) : null;
}

/**
 * The room refused a live change and its notice names none (a launcher before SQOL-R's exact
 * notice): the latest change the room decides on goes back, its room fields only (every other
 * change and share stays). Returns what went back, null when nothing waited for the room.
 */
export function dropRefused(): OwnPatch | null {
  const entry = [...useOwnUi.getState().pending].reverse().find((p) => asksRoom(p.patch));
  return entry ? takeBack(entry, ROOM_FIELDS) : null;
}

/** The launcher refused (or failed): the share shows what it really has again. */
export function dropPatch(which: number) {
  forget(which);
  if (!useOwnUi.getState().pending.some((p) => p.seq === which)) return;
  useOwnUi.setState((s) => ({ pending: s.pending.filter((p) => p.seq !== which) }));
}

/**
 * The launcher took it: the next state says the truth (what the room allowed). `taken` runs once
 * the state shows the room took it (with the share as it is then); never when the room refuses
 * or never moves.
 */
export function settlePatch(which: number, taken?: (now: VoiceMyShare) => void) {
  const entry = useOwnUi.getState().pending.find((p) => p.seq === which);
  if (!entry) return;
  if (taken) {
    const now = useVoiceUi.getState().state.share?.mine.find((m) => m.n === entry.n);
    if (now && answered(entry, now)) {
      // The room's answer came before the launcher's: said now, with what it took.
      dropPatch(which);
      taken(now);
      return;
    }
    whenTaken.set(which, taken);
  }
  useOwnUi.setState((s) => ({
    pending: s.pending.map((p) => (p.seq === which ? { ...p, settled: true } : p)),
  }));
  forgetTimer(which);
  settleTimers.set(
    which,
    setTimeout(() => dropPatch(which), SETTLED_MS),
  );
}

/** A share as the player sees it now: the state with the changes on their way. */
export function withPending(mine: VoiceMyShare, pending: readonly Pending[]): VoiceMyShare {
  let shown = mine;
  for (const entry of pending) if (entry.n === mine.n) shown = { ...shown, ...entry.patch };
  return shown;
}

// --- a share whose source went away -------------------------------------------------------------

/** A notice said share `n`'s source went away (`sourceClosed`; the state may say it too). */
export function markLost(n: number, why: LostWhy, until: number | null) {
  const now = Math.floor(Date.now() / 1000);
  const held = useVoiceUi.getState().state.share?.mine.find((m) => m.n === n)?.st === "hold";
  useOwnUi.setState((s) => ({
    lost: { ...s.lost, [n]: { why, until: until ?? now + LOST_WAIT_S, sawHold: held } },
  }));
}

export function clearLost(n: number) {
  if (!(n in useOwnUi.getState().lost)) return;
  useOwnUi.setState((s) => {
    const { [n]: _gone, ...rest } = s.lost;
    return { lost: rest };
  });
}

/** What the card says of a share (`closed`: its window or screen went away). */
export type OwnStatus = "starting" | "live" | "paused" | "minimized" | "closed" | "gameClosed";

/** A share whose source went away waits for another one. */
export function isLost(status: OwnStatus | undefined): status is "closed" | "gameClosed" {
  return status === "closed" || status === "gameClosed";
}

/** A share's state in the card's words: the launcher's `hold` first, a notice's mark next. */
export function ownStatus(mine: VoiceMyShare, lost?: LostMark | null): OwnStatus {
  if (mine.phase === "starting") return "starting";
  const why = mine.hold ?? lost?.why ?? (mine.st === "hold" ? "paused" : null);
  if (why === "closed" || why === "gameClosed" || why === "paused") return why;
  if (mine.st === "min") return "minimized";
  return "live";
}

/** When a share whose source went away stops by itself (Unix seconds; null: it does not). */
export function lostUntil(mine: VoiceMyShare, lost?: LostMark | null): number | null {
  return mine.holdUntil ?? lost?.until ?? null;
}

/**
 * The voice state moved: a taken change gives way to it once it moved on the change's fields (it
 * says what the room allowed; an unrelated change keeps it), a mark ends when its share goes live
 * again or ends, and a switch or a tile of a share that ended goes.
 */
export function followShares(mine: readonly VoiceMyShare[]) {
  const { pending, lost, switching, tile } = useOwnUi.getState();
  const said: Array<() => void> = [];
  const keep = pending.filter((p) => {
    const it = mine.find((m) => m.n === p.n);
    if (it === undefined) return false;
    if (!(p.settled && answered(p, it))) return true;
    // The room took it: said now, with what it took.
    const taken = whenTaken.get(p.seq);
    if (taken) said.push(() => taken(it));
    return false;
  });
  for (const gone of pending) if (!keep.includes(gone)) forget(gone.seq);
  let marks = lost;
  for (const key of Object.keys(lost)) {
    const n = Number(key);
    const mark = lost[n];
    const it = mine.find((m) => m.n === n);
    if (!mark) continue;
    if (!it || (mark.sawHold && it.st !== "hold")) {
      const { [n]: _gone, ...rest } = marks;
      marks = rest;
    } else if (!mark.sawHold && it.st === "hold") {
      marks = { ...marks, [n]: { ...mark, sawHold: true } };
    }
  }
  const patch: Partial<OwnUi> = {};
  if (keep.length !== pending.length) patch.pending = keep;
  if (marks !== lost) patch.lost = marks;
  if (switching !== null && !mine.some((m) => m.n === switching)) patch.switching = null;
  if (tile !== null && !mine.some((m) => m.n === tile)) patch.tile = null;
  if (Object.keys(patch).length) useOwnUi.setState(patch);
  for (const say of said) say();
}

useVoiceUi.subscribe((now, before) => {
  if (now.state.share !== before.state.share) followShares(now.state.share?.mine ?? []);
});

// --- reading -------------------------------------------------------------------------------------

/** The player's shares as they see them now (changes on their way shown). */
export function useOwnShares(): VoiceMyShare[] {
  const mine = useShare().mine;
  const pending = useOwnUi((s) => s.pending);
  return useMemo(
    () => (pending.length ? mine.map((m) => withPending(m, pending)) : mine),
    [mine, pending],
  );
}

/** One of the player's shares as they see it now (null: it is gone). */
export function useOwnShare(n: number): VoiceMyShare | null {
  return useOwnShares().find((m) => m.n === n) ?? null;
}

/** A share's state in the card's words, and when a share that lost its source stops. */
export function useOwnStatus(mine: VoiceMyShare): { status: OwnStatus; until: number | null } {
  const lost = useOwnUi((s) => s.lost[mine.n] ?? null);
  return { status: ownStatus(mine, lost), until: lostUntil(mine, lost) };
}

/** Tests start from nothing. */
export function resetOwnUi() {
  for (const which of [...settleTimers.keys()]) forgetTimer(which);
  whenTaken.clear();
  refusedEarly.clear();
  useOwnUi.setState({ switching: null, tile: null, pending: [], lost: {} });
}
