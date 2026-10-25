// The game's own item pictures for the 7/24 Bulut inventory and AFK+'s pickers (ITEMART): the
// launcher draws the atlas from the player's installed Minecraft (nothing of the game ships with
// us, and the CSP keeps the site's CDN atlas out). The browser preview has none, so ItemSprite
// shows its stand-ins there.
import { type VanillaAtlas, vanillaAtlas } from "@pl/ui/items";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "@/bindings";
import { isTauri } from "./platform";

const loading = new Map<string, Promise<VanillaAtlas | undefined>>();

/**
 * The vanilla item atlas of a Minecraft version (else of the newest installed one); undefined
 * while no version is installed. Asked once per version; a miss is asked again next time.
 */
export function vanillaItemArt(minecraft?: string): Promise<VanillaAtlas | undefined> {
  const key = minecraft ?? "";
  const hit = loading.get(key);
  if (hit) return hit;
  const load = (async () => {
    if (!isTauri()) return undefined;
    const result = await commands.itemsAtlas(minecraft ?? null).catch(() => undefined);
    const files = result?.status === "ok" ? result.data : null;
    if (!files) return undefined;
    const large = files.large ? convertFileSrc(files.large) : undefined;
    return vanillaAtlas(files.index, convertFileSrc(files.image), large);
  })();
  loading.set(key, load);
  void load.then((atlas) => {
    if (!atlas) loading.delete(key);
  });
  return load;
}
