import { create } from "zustand";

// Whether the last input was the keyboard: a sheet opened with the mouse keeps focus quiet (no
// ring on the first card), one opened with the keyboard starts on the active account.
let keyboardInput = false;
if (typeof window !== "undefined") {
  window.addEventListener(
    "keydown",
    () => {
      keyboardInput = true;
    },
    true,
  );
  window.addEventListener(
    "pointerdown",
    () => {
      keyboardInput = false;
    },
    true,
  );
}
export const lastInputWasKeyboard = () => keyboardInput;

/** Where focus lands when the account center opens. */
export type CenterFocus = "list" | "add";

/** UI state of the account center, shared by the title bar, the palette, Settings and the host. */
interface AccountsUi {
  centerOpen: boolean;
  centerFocus: CenterFocus;
  /** Focus goes back here when the center closes (the control that opened it). */
  returnFocus: HTMLElement | null;
  openCenter: (options?: { focus?: CenterFocus; from?: HTMLElement | null }) => void;
  closeCenter: () => void;

  /** The Microsoft sign-in dialog; `relogin` renews that account instead of adding one. */
  login: { open: boolean; relogin: string | null };
  openLogin: (relogin?: string | null) => void;
  closeLogin: () => void;

  /** Account whose skin panel is open. */
  appearanceFor: string | null;
  openAppearance: (id: string | null) => void;

  /** An account that just arrived (added, signed in or brought back): gets a brief highlight. */
  fresh: string | null;
  markFresh: (id: string | null) => void;

  /** The palette asked for the "Projects eşlemesi" row (it opens once the center shows it). */
  linkRowWanted: boolean;
  wantLinkRow: (wanted: boolean) => void;

  /** Hosts (dialogs and sheets) mounted: the AFK launch guard only asks when one is. */
  hosts: number;
  setHostMounted: (mounted: boolean) => void;
}

export const useAccountsUi = create<AccountsUi>()((set) => ({
  centerOpen: false,
  centerFocus: "list",
  returnFocus: null,
  openCenter: (options) =>
    set({
      centerOpen: true,
      centerFocus: options?.focus ?? "list",
      returnFocus:
        options?.from ??
        (document.activeElement instanceof HTMLElement ? document.activeElement : null),
    }),
  closeCenter: () => set({ centerOpen: false }),

  login: { open: false, relogin: null },
  openLogin: (relogin) => set({ login: { open: true, relogin: relogin ?? null } }),
  closeLogin: () => set((s) => ({ login: { ...s.login, open: false } })),

  appearanceFor: null,
  openAppearance: (appearanceFor) => set({ appearanceFor }),

  fresh: null,
  markFresh: (fresh) => set({ fresh }),

  linkRowWanted: false,
  wantLinkRow: (linkRowWanted) => set({ linkRowWanted }),

  hosts: 0,
  setHostMounted: (mounted) => set((s) => ({ hosts: Math.max(0, s.hosts + (mounted ? 1 : -1)) })),
}));
