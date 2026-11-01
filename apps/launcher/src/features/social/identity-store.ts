// SOC-ID: what the screens of the ways in keep between renders, for this run only: the name step
// after a new account's sign-in, the linked accounts' line under the section, and "Bağlı hesapları
// aç" (a note's action) sending the settings page to the section.
import { create } from "zustand";
import type { IdentityFailure, SocialProvider } from "@/lib/api/social-types";
import { useSocialUi } from "./store";

interface IdentityUi {
  /** The name step fills the panel: a new account signed in with a placeholder name. */
  nameStep: boolean;
  setNameStep: (on: boolean) => void;
  /** Why the last link or removal did not go through, said under "Bağlı hesaplar". */
  failure: { provider: SocialProvider; reason: IdentityFailure } | null;
  setFailure: (failure: { provider: SocialProvider; reason: IdentityFailure } | null) => void;
  /** Grows when "Bağlı hesapları aç" wants the section on screen. */
  linkedCue: number;
}

export const useIdentityUi = create<IdentityUi>()((set) => ({
  nameStep: false,
  setNameStep: (nameStep) => set({ nameStep }),
  failure: null,
  setFailure: (failure) => set({ failure }),
  linkedCue: 0,
}));

/** "Bağlı hesapları aç": the friends panel on its settings, at "Bağlı hesaplar". */
export function openLinkedAccounts(): void {
  const ui = useSocialUi.getState();
  if (ui.open) ui.goTo("settings");
  else ui.openPanel({ page: "settings" });
  useIdentityUi.setState((s) => ({ linkedCue: s.linkedCue + 1 }));
}
