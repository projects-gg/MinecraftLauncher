import { create } from "zustand";
import { persist } from "zustand/middleware";
import { HIDDEN_SNAPSHOT, type SignInFailure, type SocialSnapshot } from "@/lib/api/social-types";

export type SocialPage = "list" | "add" | "settings";

/** Pages deeper in the panel than the list slide in from the right. */
const DEPTH: Record<SocialPage, number> = { list: 0, add: 1, settings: 1 };

interface SocialUi {
  /** The latest snapshot from the backend (hidden until the first one arrives). */
  view: SocialSnapshot;
  /** Keeps the newest snapshot: an older one (out-of-order event) is ignored. */
  setView: (view: SocialSnapshot) => void;

  open: boolean;
  page: SocialPage;
  /** Direction of the last page change: 1 deeper, -1 back. */
  direction: 1 | -1;
  /** A friend code to look up when the add page opens (`projects://friend/…`). */
  prefillCode: string | null;
  /** Focus goes back here when the panel closes. */
  returnFocus: HTMLElement | null;
  openPanel: (options?: { page?: SocialPage; code?: string; from?: HTMLElement | null }) => void;
  closePanel: () => void;
  goTo: (page: SocialPage) => void;
  takePrefill: () => string | null;

  /** The browser page of the sign-in in progress ("Tarayıcıyı yeniden aç"). */
  signInUrl: string | null;
  signInError: SignInFailure | null;
  setSignIn: (url: string | null, error?: SignInFailure | null) => void;

  /** Friend id → when "Birlikte oyna" may be sent again (ms). */
  inviteCooldown: Record<string, number>;
  coolDown: (id: string, until: number) => void;

  /** The new-computer question was answered (or not needed): backups may be written. */
  restoreSettled: boolean;
  setRestoreSettled: (settled: boolean) => void;
}

export const useSocialUi = create<SocialUi>()((set, get) => ({
  view: HIDDEN_SNAPSHOT,
  setView: (view) => {
    if (view.seq < get().view.seq) return;
    set({ view });
    if (!view.enabled && get().open) set({ open: false });
  },

  open: false,
  page: "list",
  direction: 1,
  prefillCode: null,
  returnFocus: null,
  openPanel: (options) => {
    const page = options?.page ?? "list";
    set((s) => ({
      open: true,
      page,
      direction: DEPTH[page] >= DEPTH[s.page] ? 1 : -1,
      prefillCode: options?.code ?? s.prefillCode,
      returnFocus:
        options?.from ??
        (document.activeElement instanceof HTMLElement ? document.activeElement : null),
    }));
  },
  closePanel: () => set({ open: false }),
  goTo: (page) => set((s) => ({ page, direction: DEPTH[page] >= DEPTH[s.page] ? 1 : -1 })),
  takePrefill: () => {
    const code = get().prefillCode;
    if (code) set({ prefillCode: null });
    return code;
  },

  signInUrl: null,
  signInError: null,
  setSignIn: (signInUrl, error = null) => set({ signInUrl, signInError: error }),

  inviteCooldown: {},
  coolDown: (id, until) => set((s) => ({ inviteCooldown: { ...s.inviteCooldown, [id]: until } })),

  restoreSettled: true,
  setRestoreSettled: (restoreSettled) => set({ restoreSettled }),
}));

/** Friend preferences kept on this computer (and in the settings backup). */
interface SocialPrefs {
  /** The opt-in screen was accepted (Discord sign-in pressed there), Unix ms. */
  consentAt: number | null;
  favorites: string[];
  notifyFavoriteOnline: boolean;
  notifyFavoriteProjects: boolean;
  /** Back up launcher preferences to the friends service. */
  backup: boolean;
  /** Fingerprint of the last uploaded backup (skips uploads that change nothing). */
  backupFingerprint: string | null;
  setConsent: () => void;
  toggleFavorite: (id: string) => void;
  set: <K extends keyof SocialPrefValues>(key: K, value: SocialPrefValues[K]) => void;
}

export type SocialPrefValues = Omit<SocialPrefs, "setConsent" | "toggleFavorite" | "set">;

export const useSocialPrefs = create<SocialPrefs>()(
  persist(
    (set) => ({
      consentAt: null,
      favorites: [],
      notifyFavoriteOnline: true,
      notifyFavoriteProjects: true,
      backup: true,
      backupFingerprint: null,
      setConsent: () => set({ consentAt: Date.now() }),
      toggleFavorite: (id) =>
        set((s) => ({
          favorites: s.favorites.includes(id)
            ? s.favorites.filter((f) => f !== id)
            : [...s.favorites, id],
        })),
      set: (key, value) => set({ [key]: value } as Partial<SocialPrefs>),
    }),
    { name: "pl.social", version: 1 },
  ),
);
