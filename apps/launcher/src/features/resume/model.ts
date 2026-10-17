// "Devam et": when the shortcut to the last world or server is worth showing.
import type { ResumeTarget } from "@/lib/api";

const DEFAULT_PORT = 25565;

/** `host`, `host:port` or `[v6]:port` → lowercase host (no trailing dot) and port. */
export function parseAddress(address: string): { host: string; port: number } | null {
  const raw = address.trim();
  if (!raw) return null;
  let host = raw;
  let port = DEFAULT_PORT;
  const bracket = /^\[([^\]]+)\](?::(\d+))?$/.exec(raw);
  if (bracket?.[1]) {
    host = bracket[1];
    if (bracket[2]) port = Number(bracket[2]);
  } else if ((raw.match(/:/g) ?? []).length === 1) {
    const [h, p] = raw.split(":");
    if (!h || !p || !/^\d+$/.test(p)) return null;
    host = h;
    port = Number(p);
  }
  return { host: host.toLowerCase().replace(/\.$/, ""), port };
}

/**
 * Whether OYNA already does what "Devam et" would: the target is the server the instance joins
 * on its own (auto-join on). Then only OYNA is shown.
 */
export function sameAsAutoJoin(
  target: ResumeTarget,
  autoJoin: string | undefined,
  autoJoinOn: boolean,
): boolean {
  if (!autoJoinOn || !autoJoin || target.quickPlay.kind !== "multiplayer") return false;
  const server = parseAddress(autoJoin);
  const port = target.quickPlay.port ?? DEFAULT_PORT;
  return (
    server !== null &&
    server.host === target.quickPlay.host.toLowerCase().replace(/\.$/, "") &&
    server.port === port
  );
}
