// SOC-ID: what the screens of the ways in keep between renders, for this run only: the name step
// after a new account's sign-in, the linked accounts' line under the section, and "Bağlı hesapları
// aç" (a note's action) sending the settings page to the section. And, on this computer, how the
// service last offered Microsoft (`pl.social-ways`): Home's invitation and the sign-in screen show
// it from the start, while the service is asked only when the sign-in screen or the settings open
// (no request at every start).
import { useEffect } from "react";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { IdentityFailure, SocialProvider } from "@/lib/api/social-types";
import { type MicrosoftOffer, microsoftOffer } from "./identity";
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

interface KeptOffer {
  microsoft: MicrosoftOffer;
  keep: (microsoft: MicrosoftOffer) => void;
}

/** How the service offered Microsoft last, kept on this computer (not at all, at first). */
const useKeptOffer = create<KeptOffer>()(
  persist(
    (set) => ({
      microsoft: "none",
      keep: (microsoft) => set({ microsoft }),
    }),
    {
      name: "pl.social-ways",
      version: 1,
      partialize: (s) => ({ microsoft: s.microsoft }),
    },
  ),
);

/** How the sign-in offers Microsoft now: the service's answer of this run, else the kept one. */
export function useMicrosoftOffer(): MicrosoftOffer {
  const known = useSocialUi((s) => s.view.providersKnown === true);
  const live = useSocialUi((s) => microsoftOffer(s.view));
  // A kept value this version cannot read counts as no Microsoft.
  const kept = useKeptOffer((s) =>
    s.microsoft === "linked" || s.microsoft === "open" ? s.microsoft : "none",
  );
  return known ? live : kept;
}

/** Keeps the service's answer for the next start (mounted once, with the friends host). */
export function useKeepMicrosoftOffer(): void {
  const known = useSocialUi((s) => s.view.providersKnown === true);
  const live = useSocialUi((s) => microsoftOffer(s.view));
  useEffect(() => {
    if (known && useKeptOffer.getState().microsoft !== live) useKeptOffer.getState().keep(live);
  }, [known, live]);
}
