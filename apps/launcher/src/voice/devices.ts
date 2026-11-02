// The audio devices as the settings show them (design §6.4, §7.10): real devices only (Windows'
// "default" and "communications" entries are the "Varsayılan" choice, which the settings keep as
// none; the communications device would make Windows lower other sounds), and what to open when
// the chosen one is gone.
import type { VoiceDevice, VoiceDevices } from "./boundary";

/** The launcher drops a report with more, or longer, than this (bytes of UTF-8). */
const MAX_PER_KIND = 32;
const MAX_ID = 256;
const MAX_LABEL_BYTES = 200;
const ALIASES = new Set(["", "default", "communications"]);

/** `text` cut to whole characters within `bytes` of UTF-8. */
export function clipUtf8(text: string, bytes: number): string {
  let used = 0;
  let out = "";
  for (const char of text) {
    const code = char.codePointAt(0) ?? 0;
    const size = code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    if (used + size > bytes) break;
    used += size;
    out += char;
  }
  return out;
}

/** The inputs and outputs worth offering, in the system's order, once each. */
export function listDevices(all: readonly MediaDeviceInfo[]): VoiceDevices {
  const pick = (kind: MediaDeviceKind): VoiceDevice[] => {
    const seen = new Set<string>();
    const out: VoiceDevice[] = [];
    for (const device of all) {
      if (device.kind !== kind || ALIASES.has(device.deviceId) || seen.has(device.deviceId)) {
        continue;
      }
      if (device.deviceId.length > MAX_ID) continue;
      seen.add(device.deviceId);
      out.push({ id: device.deviceId, label: clipUtf8(device.label.trim(), MAX_LABEL_BYTES) });
      if (out.length >= MAX_PER_KIND) break;
    }
    return out;
  };
  return { inputs: pick("audioinput"), outputs: pick("audiooutput") };
}

/**
 * The device to open: the chosen one while it is there, else the system's default (`undefined`);
 * `missing` says the chosen one is gone.
 */
export function chooseDevice(
  chosen: string | null,
  present: readonly VoiceDevice[],
): { id: string | undefined; missing: boolean } {
  if (!chosen) return { id: undefined, missing: false };
  if (present.some((device) => device.id === chosen)) return { id: chosen, missing: false };
  return { id: undefined, missing: true };
}
