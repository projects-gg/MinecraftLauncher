// Preview of "Bağlan" (FIX2): the game closing at the launcher's word, and the overlay taking the
// player to a server in game.
// - `?close=slow`: the game does not close by itself (after 20 s: the warning and "Zorla kapat");
// - `?close=stuck`: nothing of it can be asked (the warning at once);
// - `?ingame=1`: the overlay moves the player in game; `?ingame=fail`: it cannot (the game closes).
// Otherwise the game takes a moment to save and closes. Several games may run (PLAY-AGAIN): each
// session ends on its own, by the launcher's number for its game.

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

/** The running sessions' ends, by game number. */
const ends = new Map<number, () => void>();
let lastGame = 100;

/** A new game number, as the launcher gives each game it starts. */
export function mockGameNumber(): number {
  lastGame += 1;
  return lastGame;
}

/** A session of `game` that lasts `ms` unless the game is closed first. */
export function mockSession(game: number, ms: number): Promise<void> {
  return new Promise<void>((resolve) => {
    const done = () => {
      if (ends.get(game) === done) ends.delete(game);
      resolve();
    };
    ends.set(game, done);
    void delay(ms).then(done);
  });
}

/** The launcher asks `game` to close: true when it could be asked. */
export async function mockCloseGame(game: number): Promise<boolean> {
  const close = flag("close");
  const end = ends.get(game);
  if (!end || close === "stuck") return false;
  if (close === "slow") return true;
  // Saving the world takes a moment.
  void delay(1400).then(end);
  return true;
}

/** "Zorla kapat": the game ends at once. */
export async function mockKillGame(game: number): Promise<void> {
  ends.get(game)?.();
}

/** The overlay in game: whether it is there (the question then offers moving in game). */
export function mockInGameReady(): boolean {
  return flag("ingame") !== "";
}

export async function mockConnectInGame(): Promise<boolean> {
  const ingame = flag("ingame");
  await delay(ingame === "fail" ? 700 : 1100);
  if (ingame === "fail" || ends.size === 0) return false;
  return true;
}
