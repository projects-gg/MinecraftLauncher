import type { FastPacksView } from "@/bindings";
import type { FastPackApi } from "./api";

/**
 * "Hızlı kaynak paketi yükleme" in the browser preview: the switch shows (as with Studio's flag
 * on), and `?fastPacks=missing` (the pack has no mod for the version Play opens), `?fastPacks=own`
 * (Play opens the player's own install) or `?fastPacks=off` (the flag off) show the other states;
 * localStorage `pl.mock.fastPacks` does the same.
 */
function mode(): string {
  try {
    return (
      new URLSearchParams(window.location.search).get("fastPacks") ??
      localStorage.getItem("pl.mock.fastPacks") ??
      "on"
    );
  } catch {
    return "on";
  }
}

export const mockFastPack: FastPackApi = {
  async configure() {},
  async state(): Promise<FastPacksView> {
    const which = mode();
    return {
      shown: which !== "off",
      official: which !== "own",
      available: which !== "missing",
    };
  },
};
