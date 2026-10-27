/** The launcher's code for a disk that cannot hold the download (`CmdError::DISK_FULL`). */
export const DISK_FULL = "DISK-DOLU";

/**
 * The launcher's code for a launch straight into a world the player's other game has open
 * (PLAY-AGAIN, `launch::WORLD_OPEN`): nothing started, nothing to copy.
 */
export const WORLD_OPEN = "DUNYA-ACIK";

/**
 * The code of a launch error. The app's API keeps it on the first line of the details
 * (`lib/api/tauri.ts`), so "Ayrıntıları kopyala" copies it along; anything else reads as no code.
 */
export function launchErrorCode(details: string): string {
  const first = (details.split("\n", 1)[0] ?? "").trim();
  return /^[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)*$/.test(first) ? first : "";
}

/** What a launch error's toast offers: room on a full disk ("Yer aç"), else the details. */
export function launchErrorAction(code: string): "makeSpace" | "copyDetails" {
  return code === DISK_FULL ? "makeSpace" : "copyDetails";
}
