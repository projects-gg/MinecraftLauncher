// An AFK+ item's picture beside its name: the game's own texture (ITEMART's `ItemSprite`, from the
// atlas the launcher draws from the player's installed Minecraft of the account's version) on the
// chamfered tile, or its quiet stand-in where the atlas has no such item (the browser preview has
// no atlas at all). A custom item is drawn by its material and look (`cmd`, `model`): the server's
// own art where ITEMART has it, else the material as the game shows it without the server's pack;
// one whose material is not known yet (typed, waiting for its first sight) is the tile with a line
// cube in it, as on the site (an empty tile read as a picture that failed). Decorative: the name
// beside it says what it is.
import { Cube } from "@phosphor-icons/react";
import { cn } from "@pl/ui";
import type { TitleGlyph } from "@pl/ui/inventory";
import { hostMatches, type ItemArt, ItemArtProvider, ItemSprite } from "@pl/ui/items";
import { createContext, type ReactNode, useContext, useEffect, useState } from "react";
import { type ItemOverlay, itemArtOverlay, vanillaItemArt } from "@/lib/item-art";
import { customOf, type NamedCustom } from "./items";

/** The atlases drawn so far, by Minecraft version: a panel opened again shows them from its first frame. */
const known = new Map<string, NonNullable<ItemArt["vanilla"]>>();
/** IA-ART's layer once read (it is the same for every account). */
let knownOverlay: ItemOverlay | undefined;

type GlyphOf = (codePoint: number) => TitleGlyph | undefined;
const TitleGlyphs = createContext<GlyphOf | undefined>(undefined);

/** A menu title's pictures on the account's server (IA-ART), for an `InventoryView` in the panel. */
export function usePlusTitleGlyph(): GlyphOf | undefined {
  return useContext(TitleGlyphs);
}

/**
 * Gives the AFK+ panel (and the dialogs it opens) the vanilla atlas of the account's Minecraft
 * version (else of the newest installed one): the bot's items are the game's; and, on the servers
 * IA-ART names, the Projects server's own looks (FEAT-L's `itemArtOverlay()`), never elsewhere.
 */
export function PlusItemArt({
  minecraft,
  server,
  children,
}: {
  minecraft?: string | null;
  /** The account's server address (`play.projects.gg:25565`). */
  server?: string;
  children: ReactNode;
}) {
  const key = minecraft ?? "";
  const [vanilla, setVanilla] = useState(() => known.get(key));
  const [overlay, setOverlay] = useState(knownOverlay);
  useEffect(() => {
    let live = true;
    const hit = known.get(key);
    setVanilla(hit);
    if (!hit) {
      void vanillaItemArt(minecraft ?? undefined).then((atlas) => {
        if (!atlas) return;
        known.set(key, atlas);
        if (live) setVanilla(atlas);
      });
    }
    if (!knownOverlay) {
      void itemArtOverlay().then((loaded) => {
        if (!loaded) return;
        knownOverlay = loaded;
        if (live) setOverlay(loaded);
      });
    }
    return () => {
      live = false;
    };
  }, [key, minecraft]);
  const onItsServer = Boolean(overlay && hostMatches(server, overlay.hosts));
  const art: ItemArt | undefined =
    vanilla || onItsServer
      ? {
          vanilla,
          overlay: onItsServer ? overlay?.overlay : undefined,
          host: onItsServer ? server : undefined,
        }
      : undefined;
  return (
    <ItemArtProvider value={art}>
      <TitleGlyphs.Provider value={onItsServer ? overlay?.glyph : undefined}>
        {children}
      </TitleGlyphs.Provider>
    </ItemArtProvider>
  );
}

export type IconSize = 24 | 32 | 40 | 48;

/** A custom item whose material is not known yet. */
const UNSEEN = "custom:unseen";

/** The chamfered tile with an item drawn on it by its id and look. */
export function ItemTile({
  id,
  cmd,
  model,
  size = 32,
  className,
}: {
  id: string;
  cmd?: number | null;
  model?: string | null;
  size?: IconSize;
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
      {id === UNSEEN ? (
        <Cube size={Math.round(size * 0.45)} className="text-ash-400" />
      ) : (
        // Whole multiples of 16 keep the game's pixels square.
        <ItemSprite
          id={id}
          cmd={cmd ?? undefined}
          model={model ?? undefined}
          size={size >= 40 ? 32 : 16}
        />
      )}
    </span>
  );
}

export function PlusItemIcon({
  item,
  custom = [],
  size = 32,
  className,
}: {
  /** `minecraft:elytra` (the texture's key) or `custom:<k>`. */
  item: string;
  /** The account's custom items (a custom reference is drawn by its material and look). */
  custom?: readonly NamedCustom[];
  /** The tile's side in px. */
  size?: IconSize;
  className?: string;
}) {
  const own = customOf(item, custom);
  const id = item.startsWith("custom:") ? (own?.base ?? UNSEEN) : item;
  return (
    <ItemTile
      id={id}
      cmd={own?.look?.cmd}
      model={own?.look?.model}
      size={size}
      className={className}
    />
  );
}
