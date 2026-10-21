// Which launch errors lead to the troubleshooter ("Sorunu bul" on the error's toast, HELP1).

/**
 * Errors that already say their own fix: a game already open, a data folder being moved, no
 * account, a cancel, and a full disk (its toast offers "Yer aç").
 */
const OWN_FIX: ReadonlySet<string> = new Set([
  "OYUN-ACIK",
  "TASI-SURUYOR",
  "HESAP-YOK",
  "IPTAL",
  "DISK-DOLU",
]);

/** The troubleshooter can help with this launch error (its code, `launchErrorCode`). */
export function helpsWithLaunchError(code: string): boolean {
  return !OWN_FIX.has(code);
}
