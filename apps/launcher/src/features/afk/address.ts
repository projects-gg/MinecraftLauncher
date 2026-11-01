// One "Sunucu adresi" field, as Minecraft's own server list has it: the port, when the server needs
// one, after a colon at the end ("mc.sunucu.net:25570"). A separate port field showed "0" for "found
// by itself" (FEAT-L, SHEET-UX). The settings keep host and port apart.

/** What the field shows for a stored host and port (0 or "": the server's own port). */
export function joinAddress(host: string, port: number | string): string {
  const shown = typeof port === "number" ? (port > 0 ? String(port) : "") : port.trim();
  return shown ? `${host}:${shown}` : host;
}

/**
 * The server's address in a pasted link: without "<scheme>://" in front ("https://") and without
 * a path behind ("/", "/join"), which no server address has (QOL-L, TASTE-2: a link copied from a
 * website was refused with a sentence about the port).
 */
function addressOf(text: string): string {
  const unlinked = text.replace(/^[a-z][a-z0-9+.-]*:\/\//i, "");
  const slash = unlinked.indexOf("/");
  return (slash < 0 ? unlinked : unlinked.slice(0, slash)).trim();
}

/**
 * Host and port in what is typed or pasted: digits after the one colon at the end are the port
 * ("" while there are none; any count, so a port too long is the port's own range problem).
 * Anything else stays in the host, whose own check names the problem; a bare IPv6 address
 * (several colons) has no port.
 */
export function splitAddress(text: string): { host: string; port: string } {
  const trimmed = addressOf(text.trim());
  const colon = trimmed.lastIndexOf(":");
  if (colon < 0 || trimmed.indexOf(":") !== colon) return { host: trimmed, port: "" };
  const port = trimmed.slice(colon + 1).trim();
  if (!/^\d*$/.test(port)) return { host: trimmed, port: "" };
  return { host: trimmed.slice(0, colon).trim(), port };
}

/**
 * What the field shows once its host or port change from outside. Typing is never fought: a text
 * that still reads as that address stays as typed ("mc.sunucu.net:" while the port comes), another
 * address (a reload) shows as stored. `stored`: a save was taken or the edits were dropped, so the
 * field shows what was kept as Minecraft writes it, even when it reads the same: a pasted
 * "https://mc.sunucu.net:25565" becomes "mc.sunucu.net:25565" (SMALLS-L3), the port only when one
 * was given.
 */
export function shownAddress(
  text: string,
  host: string,
  port: number | string,
  stored = false,
): string {
  if (!stored) {
    const typed = splitAddress(text);
    if (typed.host === host.trim() && (typed.port || "0") === String(port || "0")) return text;
  }
  return joinAddress(host, port);
}
