// What world sharing's interface keeps (HOST1): the backend's view (`hostApi`), the invite window,
// the world's sheet, Home lines closed in this run, the question before a guest is removed and a
// failed opening asked about while the launcher was in front. The privacy line of the invite
// window shows once per computer (`useHostPrefs`).
import { create } from "zustand";
import { persist } from "zustand/middleware";
import {
  HIDDEN_HOST_VIEW,
  type HostView,
  type OpenFailure,
  type OpenRequest,
  type WorldGuest,
} from "@/lib/api/host-types";
import type { WorldMode } from "@/lib/api/library-types";

/** The invite window: opening a world from its row, or more invites to the open one. */
export type PickerRequest =
  | {
      kind: "open";
      instanceId: string;
      folder: string;
      worldName: string;
      gameMode: WorldMode;
      hardcore: boolean;
      /** Friends ticked from the start (a failed opening's "Tekrar dene" without its request). */
      preselect?: string[];
    }
  | { kind: "more" };

interface HostUi {
  view: HostView;
  setView: (view: HostView) => void;
  picker: PickerRequest | null;
  openPicker: (request: PickerRequest) => void;
  closePicker: () => void;
  /** The world's sheet (its guests, "Davet et", "Paylaşımı kapat"). */
  sheet: boolean;
  openSheet: () => void;
  closeSheet: () => void;
  /** Invites whose Home line was closed (this run only; the invite itself stays). */
  homeClosed: string[];
  closeHome: (session: string) => void;
  /** "Dünyadan çıkar" asks one sentence first. */
  removing: WorldGuest | null;
  askRemove: (guest: WorldGuest | null) => void;
  /** An opening that failed while the launcher was in front: its two ways out. */
  failure: OpenFailure | null;
  showFailure: (reason: OpenFailure | null) => void;
  /** The last "Dünyayı aç" of this run, for "Tekrar dene" (memory only). */
  lastOpen: OpenRequest | null;
  setLastOpen: (request: OpenRequest | null) => void;
}

export const useHostUi = create<HostUi>()((set) => ({
  view: HIDDEN_HOST_VIEW,
  // Views arrive in order, but a slow first answer must not undo a newer push.
  setView: (view) => set((s) => (view.seq >= s.view.seq ? { view } : s)),
  picker: null,
  // After the menu or the panel that asked has let go of the focus.
  openPicker: (picker) => window.setTimeout(() => set({ picker }), 0),
  closePicker: () => set({ picker: null }),
  sheet: false,
  openSheet: () => window.setTimeout(() => set({ sheet: true }), 0),
  closeSheet: () => set({ sheet: false }),
  homeClosed: [],
  closeHome: (session) => set((s) => ({ homeClosed: [...s.homeClosed, session] })),
  removing: null,
  askRemove: (removing) => window.setTimeout(() => set({ removing }), 0),
  failure: null,
  showFailure: (failure) => set({ failure }),
  lastOpen: null,
  setLastOpen: (lastOpen) => set({ lastOpen }),
}));

interface HostPrefs {
  /** "IP adresin arkadaşlarına görünmez." was shown once (host1.md §1.2). */
  privacySeen: boolean;
  setPrivacySeen: () => void;
}

export const useHostPrefs = create<HostPrefs>()(
  persist(
    (set) => ({
      privacySeen: false,
      setPrivacySeen: () => set({ privacySeen: true }),
    }),
    { name: "pl.host", version: 1 },
  ),
);
