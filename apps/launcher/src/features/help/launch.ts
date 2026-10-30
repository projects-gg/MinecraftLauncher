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

/**
 * The error's line on a toast whose button is "Sorunu bul": without the message's closing "Tekrar
 * dene." ("Try again."), which asked for one thing while the button offered another.
 */
export function helpToastLine(message: string): string {
  const line = message.replace(/\s*(?:Tekrar dene|Try again)\.$/u, "").trim();
  return line || message;
}
