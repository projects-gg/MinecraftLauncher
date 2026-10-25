// An AFK+ item's picture beside its name: the game's own texture (ITEMART's `ItemSprite`, from the
// atlas the launcher draws from the player's installed Minecraft) on the chamfered tile, or its
// quiet stand-in where the atlas has no such item (a server's own item, or the browser preview,
// which has no atlas at all). Decorative: the name beside it says what it is.
import { cn } from "@pl/ui";
import { type ItemArt, ItemArtProvider, ItemSprite } from "@pl/ui/items";
import { type ReactNode, useEffect, useState } from "react";
import { vanillaItemArt } from "@/lib/item-art";
import type { PlusCatalog } from "./api";
import { currentLang } from "./words";

/** The atlas once it was drawn: a panel opened again shows the pictures from its first frame. */
let known: ItemArt | undefined;

/**
 * Gives the AFK+ panel (and the dialogs it opens) the vanilla atlas of the newest installed
 * Minecraft: the bot's items are the game's, whatever version the player plays.
 */
export function PlusItemArt({ children }: { children: ReactNode }) {
  const [art, setArt] = useState(known);
  useEffect(() => {
    if (known) return;
    let live = true;
    void vanillaItemArt().then((vanilla) => {
      if (!vanilla) return;
      known = { vanilla };
      if (live) setArt(known);
    });
    return () => {
      live = false;
    };
  }, []);
  return <ItemArtProvider value={art}>{children}</ItemArtProvider>;
}

export function PlusItemIcon({
  item,
  size = 32,
  className,
}: {
  /** The catalog id (`minecraft:elytra`): the texture's key. */
  item: string;
  /** The tile's side in px. */
  size?: 24 | 32 | 40 | 48;
  className?: string;
}) {
  return (
    <span
      aria-hidden
      className={cn(
        // Exact px: the root is 14 px, so Tailwind's size steps are 3.5 px and break the pixel grid.
        "chamfer grid shrink-0 place-items-center bg-obsidian-800",
        size === 24
          ? "size-[24px] [--cut:4px]"
          : size === 32
            ? "size-[32px] [--cut:5px]"
            : size === 40
              ? "size-[40px] [--cut:6px]"
              : "size-[48px] [--cut:7px]",
        className,
      )}
    >
      {/* Whole multiples of 16 keep the game's pixels square. */}
      <ItemSprite id={item} size={size >= 40 ? 32 : 16} />
    </span>
  );
}

/** An item's name in the interface's language, from the catalog (its id read plainly without). */
export function itemName(catalog: PlusCatalog | null | undefined, id: string): string {
  const found = catalog?.items.find((item) => item.id === id);
  if (found) return currentLang() === "en" ? found.en : found.tr;
  const path = id.split(":").pop() ?? id;
  const words = path.replace(/_/g, " ").trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : id;
}
