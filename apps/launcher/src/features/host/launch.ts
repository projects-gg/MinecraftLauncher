// Starting the game for world sharing (HOST1): into the player's own world to open it ("Dünyayı
// aç"), or into a friend's world through the launcher's own way to it on this computer ("Katıl").
// Both go through OYNA's launch, so its progress, the Java question and the crash help work as for
// any start. Neither waits for the game to end.
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";

/** True while a game is being prepared or running. */
function busy(): boolean {
  const kind = useLaunch.getState().phase.kind;
  return kind === "working" || kind === "launching" || kind === "running";
}

/** The game starts straight into the player's world; the backend opens it once it has loaded. */
export function startOwnWorld(instanceId: string, folder: string): boolean {
  if (busy()) return false;
  usePrefs.getState().selectInstance(instanceId);
  void useLaunch
    .getState()
    .play(instanceId, { quickPlay: { kind: "singleplayer", world: folder } });
  return true;
}

/** The game starts with the install the join prepared, straight into the friend's world. */
export function startFriendWorld(target: { instance: string; port: number }): boolean {
  if (busy()) return false;
  usePrefs.getState().selectInstance(target.instance);
  void useLaunch.getState().play(target.instance, {
    autoJoin: false,
    quickPlay: { kind: "multiplayer", host: "127.0.0.1", port: target.port },
  });
  return true;
}
