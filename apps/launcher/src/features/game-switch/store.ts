// The question "Oyna" asks while something runs (PLAY-ACCT, PLAY-AGAIN): another launch on its way
// ("Durdur ve oyna", one launch at a time), a game of the selected account on screen ("Bu adla oyun
// zaten açık", then one more), or one more game the free memory would not hold. The launch store
// opens it (stores/launch.ts), the dialog answers it and leads the way (switch-dialog.tsx). One
// question at a time: a second press meanwhile (a double click, the tray's "Oyna") changes nothing.
import { create } from "zustand";
import type { CloseStep } from "@/features/social/connect";
import type { GameAccount, LaunchOptions } from "@/lib/api";
import type { SwitchSides } from "./words";

/**
 * What the question is about: `stop` another launch on its way; `again` one more game with a name
 * already in game (the owner's question); `memory` one more game beside other accounts' games that
 * the free memory would not hold.
 */
export type SwitchKind = "stop" | "again" | "memory";

export interface SwitchRequest {
  kind: SwitchKind;
  /** The installation to play, and how ("Devam et", a server). */
  instanceId: string;
  options?: LaunchOptions;
  /** The selected account, which plays (null: none known). */
  account: GameAccount | null;
  /**
   * Where the selection goes back on "Vazgeç": the installation whose launch this account has on
   * its way (another one's "Oyna" picked its own), else nowhere.
   */
  restore: string | null;
  /** The free memory would not hold one more game: the question says so first. */
  memoryShort?: boolean;
}

interface SwitchState {
  request: SwitchRequest | null;
  /** After the answer: where the way is, and the words it began with (they stay put). */
  step: CloseStep | null;
  sides: SwitchSides | null;
}

export const useGameSwitch = create<SwitchState>()(() => ({
  request: null,
  step: null,
  sides: null,
}));

/** Opens the question; false while one is open already. */
export function askGameSwitch(request: SwitchRequest): boolean {
  if (useGameSwitch.getState().request) return false;
  useGameSwitch.setState({ request, step: null, sides: null });
  return true;
}
