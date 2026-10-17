// Instance cards in the library as drop targets: which drags they take part in and which card
// takes the files. Pure functions (cards.test.ts); the store and the cards use them.
import type { DroppedFile } from "@/lib/api";
import { type DropTarget, isCopy, planDrop } from "./model";

/**
 * How far a card's hit box reaches past the card: half the library grid's gap, so neighbouring
 * cards meet in its middle and the cursor never falls between two of them.
 */
export const CARD_REACH = 6;

/**
 * What a file probably is from its name, until the backend has read it (usually milliseconds):
 * enough to keep a card from lighting up and going out again. A ".zip" is a resource or a
 * shader pack, or a modpack; only its contents tell.
 */
export function guessKind(path: string): "mod" | "pack" | "modpack" | "unknown" {
  const name = path.toLowerCase();
  if (name.endsWith(".jar") || name.endsWith(".litemod")) return "mod";
  if (name.endsWith(".zip")) return "pack";
  if (name.endsWith(".mrpack")) return "modpack";
  return "unknown";
}

/**
 * Cards take part in drags that hold files an instance takes in: mods, resource and shader
 * packs. A modpack alone, or files nobody takes, go to the window zone as on every page.
 */
export function cardsJoin(paths: string[], files: DroppedFile[] | null): boolean {
  if (files) return files.some((f) => isCopy(f.kind));
  return paths.some((p) => {
    const kind = guessKind(p);
    return kind === "mod" || kind === "pack";
  });
}

/**
 * Whether the card's instance takes some of the files: a vanilla instance takes no mods (the
 * drop pipeline's own rule, model.ts), packs go anywhere.
 */
export function cardTakes(paths: string[], files: DroppedFile[] | null, target: DropTarget) {
  if (files) return planDrop(files, target).rows.some((r) => isCopy(r.kind) && !r.blocked);
  return paths.some((p) => {
    const kind = guessKind(p);
    return kind === "pack" || (kind === "mod" && target.modded);
  });
}

/**
 * The card a drop goes into: the one under the cursor. A drop between or beside the cards goes
 * into none: with several instances on screen, guessing one could put a mod in the wrong place.
 */
export function cardTarget(cards: string[], hovered: string | null): string | null {
  return hovered !== null && cards.includes(hovered) ? hovered : null;
}
