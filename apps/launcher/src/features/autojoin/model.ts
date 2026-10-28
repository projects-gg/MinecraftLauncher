// AUTOJOIN: the rows of "Doğrudan sunucuya bağlan"'s address picker. The lobby (the instance's
// own address) comes first and is the default; then the Projects sub-servers Studio lists, as
// "play.projects.gg → SkyBlock". Rows Studio marks "Yalnız testçiler" show only to players the
// beta program knows. The pick is kept per instance by the sub-server's id; the command it stands
// for stays in Rust (src-tauri commands/subservers.rs).
import type { SubServerView } from "@/bindings";

/** One row: the lobby (`id: null`) or a sub-server. */
export interface RouteRow {
  id: string | null;
  label: string;
  /** The narrow command bar's words for it: "→ SkyBlock" (none for the lobby). */
  short: string | null;
}

/** "play.projects.gg → SkyBlock". */
export function routeLabel(address: string, name: string): string {
  return `${address} → ${name}`;
}

/**
 * The picker's rows: the lobby, then the sub-servers this player may see. Nothing when there is
 * no sub-server to pick (the bar then shows the address as before).
 */
export function routeRows(
  address: string | undefined,
  subs: readonly SubServerView[],
  tester: boolean,
): RouteRow[] {
  if (!address) return [];
  const shown = subs.filter((sub) => !sub.testersOnly || tester);
  if (shown.length === 0) return [];
  return [
    { id: null, label: address, short: null },
    ...shown.map((sub) => ({
      id: sub.id,
      label: routeLabel(address, sub.name),
      short: `→ ${sub.name}`,
    })),
  ];
}

/** The row that applies: the saved pick while the picker offers it, else the lobby. */
export function currentRoute(
  saved: string | undefined,
  rows: readonly RouteRow[],
): RouteRow | null {
  if (rows.length === 0) return null;
  return rows.find((row) => row.id !== null && row.id === saved) ?? rows[0] ?? null;
}
