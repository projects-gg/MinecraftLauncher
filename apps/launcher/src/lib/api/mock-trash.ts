// Instances deleted in the browser preview, shared by the library (delete, "Geri al" in the
// toast) and Settings → Veri → Çöp kutusu (listed there with "Geri al" and "Kalıcı olarak sil").
import type { InstanceSummary } from "./types";

export interface MockTrashedInstance {
  instance: InstanceSummary;
  deletedAt: number;
  sizeBytes: number;
}

/** Trash id → the deleted instance. */
export const mockTrashedInstances = new Map<string, MockTrashedInstance>();
