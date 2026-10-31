// Which launch errors lead to the troubleshooter ("Sorunu bul" on the error's toast, HELP1).
import { needsSignIn } from "@/features/accounts/sign-in";

/**
 * Errors that already say their own fix: a game already open, a data folder being moved, no
 * account (its toast offers "Hesap ekle"), a cancel, and a full disk (its toast offers "Yer aç").
 */
const OWN_FIX: ReadonlySet<string> = new Set([
  "OYUN-ACIK",
  "TASI-SURUYOR",
  "HESAP-YOK",
  "IPTAL",
  "DISK-DOLU",
]);

/**
 * The troubleshooter can help with this launch error (its code, `launchErrorCode`). A session only
 * a new sign-in fixes is not its job either: that toast offers "Yeniden giriş yap".
 */
export function helpsWithLaunchError(code: string): boolean {
  return !OWN_FIX.has(code) && !needsSignIn(code);
}

/**
 * The error's line on a toast whose button is "Sorunu bul": without the message's closing "Tekrar
 * dene." ("Try again."), which asked for one thing while the button offered another.
 */
export function helpToastLine(message: string): string {
  const line = message.replace(/\s*(?:Tekrar dene|Try again)\.$/u, "").trim();
  return line || message;
}
