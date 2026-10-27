// The game's item data for an AFK+ account (FREE-ITEMS): names in both languages, stack sizes,
// durability and the enchantments that fit, for the account's Minecraft version. The backend loads
// it from our CDN with its SHA-256 checked and keeps it on disk (pl-cloud-afk `plus_items.rs`);
// here it is parsed once per version and shared by every picker, card and row. A custom item is
// named by the account's settings (the player's own name for it).
import { type ItemData, type ItemInfo, itemDataFromText, nameHint } from "@pl/ui/items";
import { useCallback, useEffect, useState } from "react";
import { plusApi } from "./api";
import type { CustomItemLike } from "./order-draft";
import { currentLang, type Lang } from "./words";

const loaded = new Map<string, ItemData>();
const loading = new Map<string, Promise<ItemData | undefined>>();

function load(minecraft: string | null): Promise<ItemData | undefined> {
  const key = minecraft ?? "";
  const hit = loading.get(key);
  if (hit) return hit;
  const next = plusApi
    .itemData(minecraft)
    .then((file) => {
      const data = file ? itemDataFromText(file.text) : undefined;
      if (data) loaded.set(key, data);
      return data;
    })
    .catch(() => undefined);
  loading.set(key, next);
  void next.then((data) => {
    // A miss is asked again next time ("Tekrar dene").
    if (!data) loading.delete(key);
  });
  return next;
}

export interface ItemDataState {
  data: ItemData | undefined;
  /** It could not be loaded: "Eşya listesi şu an yüklenemedi." */
  failed: boolean;
  retry: () => void;
}

/** The item data of the account's Minecraft version (`server.mc`; null: the newest). */
export function usePlusItemData(minecraft: string | null): ItemDataState {
  const key = minecraft ?? "";
  const [data, setData] = useState<ItemData | undefined>(() => loaded.get(key));
  const [failed, setFailed] = useState(false);
  const [round, setRound] = useState(0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `round` asks again on "Tekrar dene".
  useEffect(() => {
    let live = true;
    const known = loaded.get(key);
    if (known) {
      setData(known);
      setFailed(false);
      return;
    }
    void load(minecraft).then((next) => {
      if (!live) return;
      setData(next);
      setFailed(!next);
    });
    return () => {
      live = false;
    };
  }, [key, minecraft, round]);
  const retry = useCallback(() => {
    setFailed(false);
    setRound((n) => n + 1);
  }, []);
  return { data, failed, retry };
}

/** An item id read plainly, for an item the data does not know: "netherite_sword" → "Netherite sword". */
function plainName(id: string): string {
  const path = id.split(":").pop() ?? id;
  const words = path.replace(/_/g, " ").trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : id;
}

/** A custom item as the panel names and draws it: a stored one, or one a draft or a pick adds. */
export type NamedCustom = Pick<CustomItemLike, "k" | "name" | "base" | "look">;

/** The custom item a `custom:<k>` reference names. */
export function customOf(ref: string, custom: readonly NamedCustom[]): NamedCustom | undefined {
  if (!ref.startsWith("custom:")) return undefined;
  const k = ref.slice("custom:".length);
  return custom.find((item) => item.k === k);
}

/** What the panel needs to name items: the data and the account's custom items. */
export interface Namer {
  data: ItemData | undefined;
  custom: readonly NamedCustom[];
  /** A custom item without a name of its own (never stored so: a fallback for the words). */
  unnamed: string;
}

/** An item's name in the interface's language. */
export function itemLabel(ref: string, namer: Namer, lang: Lang = currentLang()): string {
  // A custom item that shows no name yet (an empty one while the form asks) reads "Adsız eşya".
  if (ref.startsWith("custom:")) return customOf(ref, namer.custom)?.name.trim() || namer.unnamed;
  const info = namer.data?.byId.get(ref);
  return info ? info.name[lang] : plainName(ref);
}

/** The other language's name beside a name two items share ("Fener" is Beacon and Lantern). */
export function itemHint(info: ItemInfo, data: ItemData, lang: Lang = currentLang()) {
  return nameHint(data, info, lang);
}

/** The item a reference names in the data (none for a custom item). */
export function itemInfo(ref: string | null, data: ItemData | undefined): ItemInfo | undefined {
  if (!ref || ref.startsWith("custom:")) return undefined;
  return data?.byId.get(ref);
}

/**
 * The game's item behind a reference: a custom item's material. None while that cannot be told
 * (no data yet, an item the data lacks, a custom item without a known material).
 */
export function baseInfo(
  ref: string | null,
  data: ItemData | undefined,
  custom: Namer["custom"],
): ItemInfo | undefined {
  if (!ref) return undefined;
  const id = ref.startsWith("custom:") ? customOf(ref, custom)?.base : ref;
  return id ? data?.byId.get(id) : undefined;
}
