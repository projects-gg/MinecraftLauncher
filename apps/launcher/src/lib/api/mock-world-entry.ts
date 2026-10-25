import type { LaunchEvent, LaunchOptions } from "./types";

/**
 * Preview switch for "Devam et" into a world (OVL-FIX): `?worldEntry=title` (or localStorage
 * `pl.mock.worldEntry`) makes a launch into a world say that the game opens on its main menu, as the
 * backend does when a mod breaks the game's direct entry into worlds and no overlay opens it.
 */
function flag(): string {
  try {
    return (
      new URLSearchParams(window.location.search).get("worldEntry") ??
      localStorage.getItem("pl.mock.worldEntry") ??
      ""
    );
  } catch {
    return "";
  }
}

export function mockWorldFromTitle(options?: LaunchOptions): LaunchEvent | null {
  if (flag() !== "title" || options?.quickPlay?.kind !== "singleplayer") return null;
  return { type: "worldFromTitle", mods: ["Remove Reloading Screen"] };
}
