// The question "Oyna" asks while another game runs (PLAY-ACCT): another account's game, or
// another installation's (one game at a time). The launch store opens it (stores/launch.ts), the
// dialog answers it and leads the way (switch-dialog.tsx). One question at a time: a second press
// meanwhile (a double click, the tray's "Oyna") changes nothing.
import { create } from "zustand";
import type { CloseStep } from "@/features/social/connect";
import type { GameAccount, LaunchOptions } from "@/lib/api";
import type { SwitchSides } from "./words";

export interface SwitchRequest {
  /** The installation to play, and how ("Devam et", a server). */
  instanceId: string;
  options?: LaunchOptions;
  /** The selected account, which plays once the other game is gone (null: none known). */
  account: GameAccount | null;
  /**
   * Where the selection goes back on "Vazgeç": the installation whose game this account has on
   * (another one's "Oyna" picked its own), else nowhere.
   */
  restore: string | null;
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
