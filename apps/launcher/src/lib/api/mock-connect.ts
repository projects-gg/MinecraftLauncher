// Preview of "Bağlan" (FIX2): the game closing at the launcher's word, and the overlay taking the
// player to a server in game.
// - `?close=slow`: the game does not close by itself (after 20 s: the warning and "Zorla kapat");
// - `?close=stuck`: nothing of it can be asked (the warning at once);
// - `?ingame=1`: the overlay moves the player in game; `?ingame=fail`: it cannot (the game closes).
// Otherwise the game takes a moment to save and closes.

function flag(key: string): string {
  try {
    return (
      new URLSearchParams(window.location.search).get(key) ??
      localStorage.getItem(`pl.mock.${key}`) ??
      ""
    );
  } catch {
    return "";
  }
}

const delay = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** The running session's end, while one runs (one game at a time). */
let end: (() => void) | null = null;

/** A session that lasts `ms` unless the game is closed first. */
export function mockSession(ms: number): Promise<void> {
  return new Promise<void>((resolve) => {
    const done = () => {
      if (end === done) end = null;
      resolve();
    };
    end = done;
    void delay(ms).then(done);
  });
}

/** The launcher asks the game to close: true when it could be asked. */
export async function mockCloseGame(): Promise<boolean> {
  const close = flag("close");
  if (!end || close === "stuck") return false;
  if (close === "slow") return true;
  const done = end;
  // Saving the world takes a moment.
  void delay(1400).then(done);
  return true;
}

/** "Zorla kapat": the game ends at once. */
export async function mockKillGame(): Promise<void> {
  end?.();
}

/** The overlay in game: whether it is there (the question then offers moving in game). */
export function mockInGameReady(): boolean {
  return flag("ingame") !== "";
}

export async function mockConnectInGame(): Promise<boolean> {
  const ingame = flag("ingame");
  await delay(ingame === "fail" ? 700 : 1100);
  if (ingame === "fail" || !end) return false;
  return true;
}
