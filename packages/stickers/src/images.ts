// The launcher's sticker images (256 px WebP, alpha; drawn at 112 px in chat and 64 px in the
// picker). The bundler turns the pattern below into the set's files, each with its own hashed
// name, so only the stickers on screen are ever read.
import type { Sticker } from "./index";

/** The image of a sticker (its URL in the app). */
export function stickerImage(sticker: Pick<Sticker, "set" | "id">): string {
  if (sticker.set !== "pl1") throw new Error(`No images for the set ${sticker.set}`);
  return new URL(`../pl1/launcher/${sticker.id}.webp`, import.meta.url).href;
}
