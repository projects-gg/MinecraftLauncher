// One "Sunucu adresi" field, as Minecraft's own server list has it: the port, when the server needs
// one, after a colon at the end ("mc.sunucu.net:25570"). A separate port field showed "0" for "found
// by itself" (FEAT-L, SHEET-UX). The settings keep host and port apart.

/** What the field shows for a stored host and port (0 or "": the server's own port). */
export function joinAddress(host: string, port: number | string): string {
  const shown = typeof port === "number" ? (port > 0 ? String(port) : "") : port.trim();
  return shown ? `${host}:${shown}` : host;
}

/**
 * Host and port in what is typed: digits after the one colon at the end are the port ("" while
 * there are none). Anything else stays in the host, whose own check names the problem; a bare
 * IPv6 address (several colons) has no port.
 */
export function splitAddress(text: string): { host: string; port: string } {
  const trimmed = text.trim();
  const colon = trimmed.lastIndexOf(":");
  if (colon < 0 || trimmed.indexOf(":") !== colon) return { host: trimmed, port: "" };
  const port = trimmed.slice(colon + 1).trim();
  if (!/^\d{0,6}$/.test(port)) return { host: trimmed, port: "" };
  return { host: trimmed.slice(0, colon).trim(), port };
}
