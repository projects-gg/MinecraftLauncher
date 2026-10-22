// "Bağlan" in game (FIX2 with OVL2): while the game runs our overlay, the launcher asks it to take
// the player to the server without a restart; the mod leaves the world or server the usual way (a
// singleplayer world is saved) and connects. Until the overlay's bridge offers it, the game closes
// and starts again (the preview can show the move: mock-connect.ts).
import { mockConnectInGame, mockInGameReady } from "@/lib/api/mock-connect";
import { isTauri } from "@/lib/platform";
import type { JoinTarget } from "./play-together";

/** The overlay is in the running game: the question offers moving there in game. */
export function inGameReady(): boolean {
  return isTauri() ? false : mockInGameReady();
}

/** Whether the overlay took the player to `target`. */
export async function connectInGame(target: JoinTarget): Promise<boolean> {
  void target;
  if (!inGameReady()) return false;
  return isTauri() ? false : mockConnectInGame();
}
