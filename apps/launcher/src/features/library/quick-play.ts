// Starting the game straight into a world or a server ("Oyna" on a world or server row), and
// the rows of the server list.

/**
 * Whether a Minecraft version can open a world or join a server right after start (the game's
 * quick play, 1.20 and its 23w14a snapshot onwards). Older versions open the title screen.
 */
export function canQuickPlay(mcVersion: string): boolean {
  const version = mcVersion.trim();
  const release = /^1\.(\d+)/.exec(version);
  if (release) return Number(release[1]) >= 20;
  const snapshot = /^(\d{2})w(\d{2})[a-z]$/i.exec(version);
  if (snapshot) {
    const year = Number(snapshot[1]);
    return year > 23 || (year === 23 && Number(snapshot[2]) >= 14);
  }
  // Year-numbered versions (25.x, 26.1.2) came long after quick play.
  const yearly = /^(\d{2})\.\d+/.exec(version);
  return yearly ? Number(yearly[1]) >= 25 : false;
}

export interface ServerAddress {
  host: string;
  port: number | null;
}

/**
 * `play.projects.gg`, `play.projects.gg:25566`, `[2001:db8::1]:25565`: the host and the port (null:
 * the default, or the server's SRV record). Null when it cannot be an address.
 */
export function parseServerAddress(address: string): ServerAddress | null {
  const text = address.trim();
  if (!text || /\s/.test(text)) return null;
  const bracketed = /^\[([0-9a-f:.]+)\](?::(\d{1,5}))?$/i.exec(text);
  if (bracketed) return withPort(bracketed[1] ?? "", bracketed[2]);
  const colons = text.split(":").length - 1;
  if (colons === 0) return validHost(text) ? { host: text, port: null } : null;
  if (colons === 1) {
    const [host = "", port = ""] = text.split(":");
    return validHost(host) && /^\d{1,5}$/.test(port) ? withPort(host, port) : null;
  }
  // A bare IPv6 address (the game asks for brackets with a port).
  return /^[0-9a-f:.]+$/i.test(text) ? { host: text, port: null } : null;
}

/**
 * Keys for the server rows: the address, with "#2", "#3"… on a repeat. The game's own list may
 * hold one server twice, and two rows with one key make React drop or duplicate rows.
 */
export function serverRowKeys(addresses: readonly string[]): string[] {
  const seen = new Map<string, number>();
  return addresses.map((address) => {
    const base = address.trim().toLowerCase();
    const count = (seen.get(base) ?? 0) + 1;
    seen.set(base, count);
    return count === 1 ? base : `${base}#${count}`;
  });
}

function validHost(host: string): boolean {
  return /^[\p{L}\p{N}._-]+$/u.test(host) && !host.startsWith(".") && !host.endsWith("..");
}

function withPort(host: string, port: string | undefined): ServerAddress | null {
  if (!host) return null;
  if (port === undefined) return { host, port: null };
  const number = Number(port);
  return number >= 1 && number <= 65_535 ? { host, port: number } : null;
}
