// INST-ADD: the rules behind "Yeni Kurulum" → "Daha Fazla Seçenek" (create-more.tsx).

/**
 * The most memory a new instance can be given: the machine's, less 1 GB for the system, in 512 MB
 * steps, and never under 2 GB (Settings → Oyun's default uses the same ceiling).
 */
export function memoryCeiling(totalMb: number): number {
  return Math.max(2048, Math.floor((totalMb - 1024) / 512) * 512);
}

/** Where "Özel" starts: the amount chosen before, else 4 GB, within the ceiling. */
export function customMemoryStart(totalMb: number, current: number | null): number {
  return Math.min(current ?? 4096, memoryCeiling(totalMb));
}

/**
 * The tab a new instance opens on: its settings when the player opened "Daha Fazla Seçenek" (Java
 * and the window size are chosen there, as the section says), else the overview.
 */
export function landingTab(moreOpen: boolean): "settings" | undefined {
  return moreOpen ? "settings" : undefined;
}
