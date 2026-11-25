import { create } from "zustand";

/**
 * A move of the data folder asked for outside Settings (OYNA's YOL-HARF "Veri Klasörünü Taşı",
 * CRASH-DIAG): Settings → Veri ve Depolama opens its move dialog with this place already picked.
 */
interface DataMoveRequest {
  requested: string | null;
  request: (picked: string) => void;
  clear: () => void;
}

export const useDataMove = create<DataMoveRequest>((set) => ({
  requested: null,
  request: (picked) => set({ requested: picked }),
  clear: () => set({ requested: null }),
}));
