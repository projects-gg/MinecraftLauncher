// "Bağlan" in game (FIX2 with OVL2): while the game runs our overlay, the launcher asks it to take
// the player to the server without a restart; the mod leaves the world or server the usual way (a
// singleplayer world is saved) and connects. The launcher decides where (only a server it would
// join anyway) and asks only right after the player's "Bağlan"; the shell answers within five
// seconds (`overlay::connect_in_game`). Anything but "done": the game closes and starts again.
import { commands } from "@/bindings";
import { useOverlay } from "@/features/overlay/store";
import { hostApi } from "@/lib/api/host-api";
import { mockConnectInGame, mockInGameReady } from "@/lib/api/mock-connect";
import { isTauri } from "@/lib/platform";
import { type JoinTarget, targetAddress } from "./play-together";

/** The overlay is in the running game: the question offers moving there in game. */
export function inGameReady(): boolean {
  return isTauri() ? useOverlay.getState().connected : mockInGameReady();
}

/**
 * Whether the overlay took the player to `target` (`host[:port]`, as the shell takes it). A
 * friend's world (HOST1) goes through its join, which hands the overlay the world's label.
 */
export async function connectInGame(target: JoinTarget): Promise<boolean> {
  if (!inGameReady()) return false;
  if (target.kind === "world") {
    return hostApi.connectInGame(target.session, target.label).catch(() => false);
  }
  if (!isTauri()) return mockConnectInGame();
  try {
    const outcome = await commands.overlayConnectInGame(targetAddress(target));
    if (outcome.status === "done") return true;
    console.info(`in-game connect unavailable: ${outcome.reason}`);
    return false;
  } catch (error) {
    console.info("in-game connect failed", error);
    return false;
  }
}
