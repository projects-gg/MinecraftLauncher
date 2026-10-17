import { useEffect } from "react";
import { create } from "zustand";

interface DropState {
  /** Places with their own drop handling (skin panel, the import step) that are open now. */
  claims: number;
  /** A modpack dropped onto the window, waiting for the import flow. */
  pendingPack: string | null;
  claim: () => () => void;
  setPendingPack: (path: string | null) => void;
}

/** Window-wide file drops: who takes them, and a modpack on its way to the import flow. */
export const useDrop = create<DropState>((set) => ({
  claims: 0,
  pendingPack: null,
  claim: () => {
    set((s) => ({ claims: s.claims + 1 }));
    let released = false;
    return () => {
      if (released) return;
      released = true;
      set((s) => ({ claims: Math.max(0, s.claims - 1) }));
    };
  },
  setPendingPack: (pendingPack) => set({ pendingPack }),
}));

/** While `active`, files dropped onto the window are this component's; the window-wide drop
 * layer steps aside. */
export function useDropClaim(active: boolean) {
  const claim = useDrop((s) => s.claim);
  useEffect(() => (active ? claim() : undefined), [active, claim]);
}
