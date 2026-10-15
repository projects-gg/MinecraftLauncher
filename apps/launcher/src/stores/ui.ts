import { create } from "zustand";

/** Transient, app-wide UI state (dialogs opened from several places). */
interface UiState {
  migrationOpen: boolean;
  setMigrationOpen: (open: boolean) => void;
}

export const useUi = create<UiState>()((set) => ({
  migrationOpen: false,
  setMigrationOpen: (migrationOpen) => set({ migrationOpen }),
}));
