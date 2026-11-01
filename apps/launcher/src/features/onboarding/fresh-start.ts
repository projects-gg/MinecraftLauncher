// After "Launcher'ı sıfırla" (INSTALL-UX) the start clears the page's storage on its first load
// and leaves this mark (src-tauri reset.rs, `FRESH_KEY`): the welcome then skips the old
// launcher's import and the player name another launcher suggests, which would bring the
// removed names back. Both stay one click away in Settings → Veri.

export const RESET_FRESH_KEY = "pl.freshStart";

type Store = Pick<Storage, "getItem" | "removeItem">;

/** The welcome follows a reset. */
export function afterReset(storage: Store = localStorage): boolean {
  try {
    return storage.getItem(RESET_FRESH_KEY) === "1";
  } catch {
    return false;
  }
}

/** The welcome is done: the next one (if ever) is an ordinary first run again. */
export function forgetReset(storage: Store = localStorage): void {
  try {
    storage.removeItem(RESET_FRESH_KEY);
  } catch {
    // Storage unavailable: nothing was marked either.
  }
}
