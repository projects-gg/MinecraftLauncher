// A launch that waits for its account (TELEM-FIX). Usage data showed new players pressing OYNA
// again and again with no account (HESAP-YOK: 7 and 4 times), the toast's "Hesap Ekle" unseen.
// Now OYNA without an account opens the account center on "Hesap ekle" instead of failing; once an
// account is there the center closes and the game starts by itself. The same after "Yeniden Giriş
// Yap" for a Microsoft session only a new sign-in fixes: the game starts once it is renewed.
// Closing the center or the sign-in without one lets the launch go, and a launch waits at most
// `HOLD_MS` (checked when something happens: no timer).
import { type AccountSummary, api, type LaunchOptions } from "@/lib/api";
import { useAccountsUi } from "./store";

/** A launch that waits: what OYNA was asked to start. */
export interface HeldPlay {
  instanceId: string;
  options?: LaunchOptions;
}

/** What it waits for: any account (none was selected), or a new sign-in of this one. */
export type PlayWait = { kind: "account" } | { kind: "signIn"; account: string };

export type HoldEvent =
  /** The selected account changed (`null`: none). */
  | { kind: "selected"; account: string | null }
  /** An account arrived: added, signed in again, or brought back. */
  | { kind: "fresh"; account: string }
  /** The center or the sign-in the wait opened closed. */
  | { kind: "closed" };

/** A launch waits at most this long for its account. */
export const HOLD_MS = 10 * 60_000;

/** What a launch waiting for `wait` since `since` does on `event` at `now`. */
export function holdStep(
  wait: PlayWait,
  since: number,
  now: number,
  event: HoldEvent,
): "resume" | "drop" | "keep" {
  if (now - since > HOLD_MS) return "drop";
  switch (event.kind) {
    case "closed":
      return "drop";
    case "fresh":
      return wait.kind === "account" || wait.account === event.account ? "resume" : "keep";
    case "selected":
      return wait.kind === "account" && event.account !== null ? "resume" : "keep";
  }
}

/** Whether the surfaces a wait opened are on screen: the center (and its sign-in), the sign-in. */
function surfaced(wait: PlayWait, ui: ReturnType<typeof useAccountsUi.getState>): boolean {
  return wait.kind === "account" ? ui.centerOpen || ui.login.open : ui.login.open;
}

interface Hold {
  play: HeldPlay;
  wait: PlayWait;
  since: number;
  resume: (play: HeldPlay) => void;
  stop: () => void;
}

let held: Hold | null = null;

function release() {
  held?.stop();
  held = null;
}

function step(event: HoldEvent) {
  const hold = held;
  if (!hold) return;
  const next = holdStep(hold.wait, hold.since, Date.now(), event);
  if (next === "keep") return;
  release();
  if (next !== "resume") return;
  // The bar shows the launch: the center that asked for the account goes.
  if (hold.wait.kind === "account") useAccountsUi.getState().closeCenter();
  hold.resume(hold.play);
}

/**
 * `play` waits for `wait`; `resume` starts it once the account is there. Call it before opening
 * the center or the sign-in it waits for. A newer wait replaces an older one.
 */
export function holdPlay(play: HeldPlay, wait: PlayWait, resume: (play: HeldPlay) => void) {
  release();
  const stop = useAccountsUi.subscribe((ui, before) => {
    if (ui.fresh && ui.fresh !== before.fresh) step({ kind: "fresh", account: ui.fresh });
    if (surfaced(wait, before) && !surfaced(wait, ui)) step({ kind: "closed" });
  });
  held = { play, wait, since: Date.now(), resume, stop };
}

/** The selected account changed (the shell's mirror of the account list). */
export function noteSelected(account: string | null | undefined) {
  if (account === undefined) return;
  step({ kind: "selected", account });
}

/** The launch that waits, if one does. */
export function heldPlay(): HeldPlay | null {
  return held?.play ?? null;
}

/** Forgets a waiting launch (tests; a launch that went another way). */
export function dropHeldPlay() {
  release();
}

/**
 * OYNA's account check: with no account selected, `play` waits for one and the account center
 * opens on "Hesap ekle" (true: do not start now). A launch that names its account, a page with
 * no account surfaces (the first-run flow) or an account list that cannot be read goes on: the
 * backend still says when there is none.
 */
export async function waitsForAccount(
  play: HeldPlay,
  resume: (play: HeldPlay) => void,
): Promise<boolean> {
  if (play.options?.account || useAccountsUi.getState().hosts === 0) return false;
  let accounts: AccountSummary[];
  try {
    accounts = await api.accounts();
  } catch {
    return false;
  }
  if (accounts.some((account) => account.active)) return false;
  holdPlay(play, { kind: "account" }, resume);
  useAccountsUi.getState().openCenter({ focus: "add" });
  return true;
}
