import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { isTauri } from "./platform";

/**
 * "Start with the computer": registers the launcher as a login item (started with
 * `--minimized`, so it waits in the tray). Throws when the OS refuses.
 */
export async function applyStartOnBoot(on: boolean): Promise<void> {
  if (!isTauri()) return;
  if (on) await enable();
  else await disable();
}

/** The OS-side state (the user may change it outside the launcher); `null` when unknown. */
export async function readStartOnBoot(): Promise<boolean | null> {
  if (!isTauri()) return null;
  try {
    return await isEnabled();
  } catch {
    return null;
  }
}
