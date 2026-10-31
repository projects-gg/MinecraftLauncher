import { needsSignIn } from "@/features/accounts/sign-in";

/** The launcher's code for a disk that cannot hold the download (`CmdError::DISK_FULL`). */
export const DISK_FULL = "DISK-DOLU";

/**
 * The launcher's code for a launch straight into a world the player's other game has open
 * (PLAY-AGAIN, `launch::WORLD_OPEN`): nothing started, nothing to copy.
 */
export const WORLD_OPEN = "DUNYA-ACIK";

/** The launcher's code for a launch with no account to play with (`launch.rs`). */
export const NO_ACCOUNT = "HESAP-YOK";

/**
 * The code of a launch error. The app's API keeps it on the first line of the details
 * (`lib/api/tauri.ts`), so "Ayrıntıları kopyala" copies it along; anything else reads as no code.
 * Microsoft's own error names follow the first part as they are ("MSA-invalid_grant").
 */
export function launchErrorCode(details: string): string {
  const first = (details.split("\n", 1)[0] ?? "").trim();
  return /^[A-Z][A-Z0-9]*(?:-[A-Za-z0-9_]+)*$/.test(first) ? first : "";
}

/**
 * What a launch error's toast offers: room on a full disk ("Yer aç"), a new sign-in for a session
 * only that fixes ("Yeniden giriş yap"), an account when there is none ("Hesap ekle"), else the
 * details.
 */
export function launchErrorAction(
  code: string,
): "makeSpace" | "signIn" | "addAccount" | "copyDetails" {
  if (code === DISK_FULL) return "makeSpace";
  if (needsSignIn(code)) return "signIn";
  if (code === NO_ACCOUNT) return "addAccount";
  return "copyDetails";
}
