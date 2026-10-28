// Sample frames for the preview, the tests and apps' mock modes: an SMP account's inventory and the
// menus a bot meets (a chest, a server's kit menu, a hopper, a shulker box, a dispenser, a window
// the view does not know). Names as the game's language files write them.
import type { InvFrame, InvItem, InvLabel, WinFrame } from "./types";

const l = (tr: string, en: string): InvLabel => ({ tr, en });

const ench = (id: string, lvl: number, tr: string, en: string) => ({
  id: `minecraft:${id}`,
  lvl,
  label: l(tr, en),
});

function item(
  slot: number,
  id: string,
  count: number,
  label: InvLabel,
  extra: Partial<InvItem> = {},
): InvItem {
  return { slot, id: `minecraft:${id}`, count, label, ...extra };
}

export const SAMPLE_INVENTORY: InvFrame = {
  held: 0,
  cursor: null,
  items: [
    item(5, "diamond_helmet", 1, l("Elmas Miğfer", "Diamond Helmet"), {
      enchanted: true,
      ench: [
        ench("protection", 4, "Koruma IV", "Protection IV"),
        ench("unbreaking", 3, "Kırılmazlık III", "Unbreaking III"),
      ],
      durability: { left: 363, max: 363 },
    }),
    item(6, "diamond_chestplate", 1, l("Elmas Göğüslük", "Diamond Chestplate"), {
      enchanted: true,
      ench: [ench("protection", 4, "Koruma IV", "Protection IV")],
      durability: { left: 412, max: 528 },
    }),
    item(7, "iron_leggings", 1, l("Demir Tozluk", "Iron Leggings"), {
      durability: { left: 96, max: 225 },
    }),
    item(8, "diamond_boots", 1, l("Elmas Botlar", "Diamond Boots"), {
      enchanted: true,
      ench: [
        ench("feather_falling", 4, "Tüy Düşüşü IV", "Feather Falling IV"),
        ench("protection", 3, "Koruma III", "Protection III"),
      ],
      durability: { left: 429, max: 429 },
    }),
    item(45, "totem_of_undying", 1, l("Ölümsüzlük Totemi", "Totem of Undying")),
    item(36, "diamond_sword", 1, l("Elmas Kılıç", "Diamond Sword"), {
      enchanted: true,
      ench: [
        ench("sharpness", 5, "Keskinlik V", "Sharpness V"),
        ench("unbreaking", 3, "Kırılmazlık III", "Unbreaking III"),
        ench("mending", 1, "Onarım", "Mending"),
      ],
      durability: { left: 1440, max: 1561 },
    }),
    item(37, "bow", 1, l("Yay", "Bow"), {
      enchanted: true,
      ench: [ench("power", 5, "Güç V", "Power V"), ench("infinity", 1, "Sonsuzluk", "Infinity")],
      durability: { left: 384, max: 384 },
    }),
    item(38, "golden_apple", 12, l("Altın Elma", "Golden Apple")),
    item(39, "ender_pearl", 16, l("Ender İncisi", "Ender Pearl")),
    item(40, "cooked_beef", 48, l("Biftek", "Steak")),
    item(41, "torch", 64, l("Meşale", "Torch")),
    item(42, "water_bucket", 1, l("Su Kovası", "Water Bucket")),
    item(43, "oak_planks", 64, l("Meşe Kalası", "Oak Planks")),
    item(9, "cobblestone", 64, l("Kırıktaş", "Cobblestone")),
    item(10, "cobblestone", 64, l("Kırıktaş", "Cobblestone")),
    item(11, "iron_ingot", 37, l("Demir Külçesi", "Iron Ingot")),
    item(12, "gold_ingot", 12, l("Altın Külçesi", "Gold Ingot")),
    item(13, "diamond", 9, l("Elmas", "Diamond")),
    item(14, "redstone", 64, l("Kızıltaş Tozu", "Redstone Dust")),
    item(15, "emerald", 5, l("Zümrüt", "Emerald")),
    item(17, "purple_shulker_box", 1, l("Mor Shulker Kutusu", "Purple Shulker Box"), {
      inside: {
        count: 292,
        top: [
          { id: "minecraft:diamond", count: 64, label: l("Elmas", "Diamond") },
          { id: "minecraft:diamond", count: 64, label: l("Elmas", "Diamond") },
          { id: "minecraft:iron_ingot", count: 64, label: l("Demir Külçesi", "Iron Ingot") },
          { id: "minecraft:gold_ingot", count: 64, label: l("Altın Külçesi", "Gold Ingot") },
          { id: "minecraft:emerald", count: 32, label: l("Zümrüt", "Emerald") },
          {
            id: "minecraft:netherite_ingot",
            count: 4,
            label: l("Netherit Külçesi", "Netherite Ingot"),
          },
        ],
      },
    }),
    item(18, "enchanted_book", 1, l("Büyülü Kitap", "Enchanted Book"), {
      enchanted: true,
      ench: [ench("sharpness", 5, "Keskinlik V", "Sharpness V")],
    }),
    item(19, "potion", 1, l("Şifa İksiri", "Potion of Healing"), {
      potion: "minecraft:strong_healing",
      color: 0xf82423,
    }),
    item(20, "experience_bottle", 32, l("Tecrübe Şişesi", "Bottle o' Enchanting"), {
      enchanted: true,
    }),
    item(21, "elytra", 1, l("Elitra", "Elytra"), {
      enchanted: true,
      ench: [ench("mending", 1, "Onarım", "Mending")],
      durability: { left: 300, max: 432 },
    }),
    item(22, "netherite_pickaxe", 1, l("Netherit Kazma", "Netherite Pickaxe"), {
      enchanted: true,
      ench: [
        ench("efficiency", 5, "Verimlilik V", "Efficiency V"),
        ench("unbreaking", 3, "Kırılmazlık III", "Unbreaking III"),
        ench("fortune", 3, "Servet III", "Fortune III"),
        ench("mending", 1, "Onarım", "Mending"),
      ],
      durability: { left: 1802, max: 2031 },
    }),
    item(24, "netherite_sword", 1, l("Netherit Kılıç", "Netherite Sword"), {
      name: "Ejderha Kılıcı",
      lore: ["Projects SMP, 2026", "Sezon ödülü"],
      enchanted: true,
      ench: [
        ench("sharpness", 5, "Keskinlik V", "Sharpness V"),
        ench("fire_aspect", 2, "Alev Bölgesi II", "Fire Aspect II"),
      ],
      durability: { left: 2031, max: 2031 },
    }),
    item(27, "firework_rocket", 64, l("Havai Fişek", "Firework Rocket")),
    item(28, "arrow", 64, l("Ok", "Arrow")),
    item(29, "bread", 20, l("Ekmek", "Bread")),
    item(30, "oak_log", 32, l("Meşe Kütüğü", "Oak Log")),
    item(31, "glass", 48, l("Cam", "Glass")),
    item(33, "obsidian", 14, l("Obsidyen", "Obsidian")),
    item(34, "name_tag", 2, l("İsim Etiketi", "Name Tag")),
  ],
};

export const SAMPLE_CHEST: WinFrame = {
  wseq: 1_000_001,
  open: { type: "generic_9x3", title: "Sandık", size: 27 },
  items: [
    item(0, "iron_ingot", 64, l("Demir Külçesi", "Iron Ingot")),
    item(1, "iron_ingot", 21, l("Demir Külçesi", "Iron Ingot")),
    item(2, "coal", 40, l("Kömür", "Coal")),
    item(4, "wheat", 64, l("Buğday", "Wheat")),
    item(5, "carrot", 33, l("Havuç", "Carrot")),
    item(9, "oak_sapling", 7, l("Meşe Fidanı", "Oak Sapling")),
    item(10, "bone_meal", 52, l("Kemik Tozu", "Bone Meal")),
    item(13, "iron_pickaxe", 1, l("Demir Kazma", "Iron Pickaxe"), {
      durability: { left: 31, max: 250 },
    }),
    item(18, "string", 18, l("İp", "String")),
    item(19, "leather", 9, l("Deri", "Leather")),
    item(26, "saddle", 1, l("Eyer", "Saddle")),
  ],
};

const pane = (slot: number): InvItem =>
  item(slot, "gray_stained_glass_pane", 1, l("Gri Vitray Cam Paneli", "Gray Stained Glass Pane"), {
    name: " ",
  });

/** A server's kit menu: decorative panes, kits with names and lore that buy on click. */
export const SAMPLE_KITS: WinFrame = {
  wseq: 1_000_002,
  open: { type: "generic_9x3", title: "Kitler", size: 27 },
  items: [
    ...[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26].map(pane),
    item(11, "iron_sword", 1, l("Demir Kılıç", "Iron Sword"), {
      name: "Başlangıç Kiti",
      lore: ["Demir zırh ve aletler", "Bekleme: 24 saat", "Tıkla ve al"],
    }),
    item(13, "diamond_chestplate", 1, l("Elmas Göğüslük", "Diamond Chestplate"), {
      name: "Savaşçı Kiti",
      lore: ["Elmas zırh, kalkan", "Fiyat: 2.500 ripple"],
      enchanted: true,
    }),
    item(15, "golden_apple", 1, l("Altın Elma", "Golden Apple"), {
      name: "Yemek Kiti",
      lore: ["32 biftek, 8 altın elma"],
    }),
  ],
};

/**
 * Two of Mojang's own default skins on its texture server, far from the plain head's look (orange
 * hair; purple hair): heads that draw their skin.
 */
const ORANGE_SKIN = "46acd06e8483b176e8ea39fc12fe105eb3a2a4970f5100057e9d84d4b60bdfa7";
const PURPLE_SKIN = "daf3d88ccb38f11f74814e92053d92f7728ddb1a7955652a60e30cb27ae6659f";
const HEAD = l("Oyuncu Kafası", "Player Head");

/**
 * A server's head shop (INV-HEADS): heads that draw their own skins, a plain one, and names and
 * lore in the server's colours and flags, with the blank spacer lines servers put between them.
 */
export const SAMPLE_HEADS: WinFrame = {
  wseq: 1_000_009,
  open: { type: "generic_9x3", title: "Kafa Mağazası", size: 27 },
  items: [
    ...[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26].map(pane),
    item(11, "player_head", 1, HEAD, {
      skin: ORANGE_SKIN,
      name: "Gezgin Kafası",
      nameStyle: [[13, "6l"]],
      lore: ["Fiyat: 250 ripple", "", "Tıkla ve satın al"],
      loreStyle: [
        [
          [7, "7"],
          [10, "e"],
        ],
        null,
        [[17, "a"]],
      ],
    }),
    item(13, "player_head", 1, HEAD, {
      skin: PURPLE_SKIN,
      name: "Büyücü Kafası",
      nameStyle: [[13, "b"]],
      lore: ["Nadir", "gizli ödül", "", "Fiyat: 900 ripple"],
      loreStyle: [
        [[5, "dl"]],
        [
          [5, "5k"],
          [5, "7"],
        ],
        null,
        [
          [7, "7"],
          [10, "e"],
        ],
      ],
    }),
    item(15, "player_head", 1, HEAD, {
      name: "Gizemli Kafa",
      nameStyle: [
        [7, "cn"],
        [5, "c"],
      ],
      lore: ["Stokta yok"],
      loreStyle: [[[10, "8m"]]],
    }),
  ],
};

/**
 * A server's book menu whose art is wider than the window (the owner's "KİTAP" menu, INV-HEADS): a
 * move back past the window's left edge, then a picture 40 GUI pixels wider on each side.
 */
export const SAMPLE_BOOK: WinFrame = {
  wseq: 1_000_010,
  open: { type: "generic_9x3", title: "\uf80a\ue002", size: 27 },
  items: [
    item(10, "written_book", 1, l("Yazılı Kitap", "Written Book"), {
      name: "Kurallar",
      lore: ["Sunucunun kuralları"],
    }),
    item(12, "writable_book", 1, l("Kitap ve Tüy", "Book and Quill"), {
      name: "Günlük",
      lore: ["Kendi notların"],
    }),
    item(14, "enchanted_book", 1, l("Büyülü Kitap", "Enchanted Book"), {
      name: "Büyüler",
      lore: ["Hangi büyü nerede"],
    }),
    item(16, "book", 1, l("Kitap", "Book"), {
      name: "Rehber",
      lore: ["Yeni başlayanlar için"],
    }),
  ],
};

export const SAMPLE_DOUBLE_CHEST: WinFrame = {
  wseq: 1_000_003,
  open: { type: "generic_9x6", title: "Geniş Sandık", size: 54 },
  items: Array.from({ length: 30 }, (_, i) =>
    i % 3 === 0
      ? item(i * 2 - (i % 2), "cobblestone", 64, l("Kırıktaş", "Cobblestone"))
      : i % 3 === 1
        ? item(i * 2 - (i % 2), "dirt", 1 + ((i * 7) % 64), l("Toprak", "Dirt"))
        : item(i * 2 - (i % 2), "oak_log", 16, l("Meşe Kütüğü", "Oak Log")),
  ).filter((it) => it.slot >= 0 && it.slot < 54),
};

export const SAMPLE_HOPPER: WinFrame = {
  wseq: 1_000_004,
  open: { type: "hopper", title: "Eşya Hunisi", size: 5 },
  items: [
    item(0, "wheat_seeds", 23, l("Buğday Tohumu", "Wheat Seeds")),
    item(2, "egg", 16, l("Yumurta", "Egg")),
  ],
};

export const SAMPLE_SHULKER: WinFrame = {
  wseq: 1_000_005,
  open: { type: "shulker_box", title: "Mor Shulker Kutusu", size: 27 },
  items: [
    item(0, "diamond", 64, l("Elmas", "Diamond")),
    item(1, "diamond", 64, l("Elmas", "Diamond")),
    item(2, "iron_ingot", 64, l("Demir Külçesi", "Iron Ingot")),
    item(3, "gold_ingot", 64, l("Altın Külçesi", "Gold Ingot")),
    item(4, "emerald", 32, l("Zümrüt", "Emerald")),
    item(5, "netherite_ingot", 4, l("Netherit Külçesi", "Netherite Ingot")),
  ],
};

export const SAMPLE_DISPENSER: WinFrame = {
  wseq: 1_000_006,
  open: { type: "generic_3x3", title: "Fırlatıcı", size: 9 },
  items: [
    item(4, "arrow", 64, l("Ok", "Arrow")),
    item(0, "fire_charge", 8, l("Ateş Topu", "Fire Charge")),
  ],
};

/** A window the view has no layout for: drawn as rows of 9. */
export const SAMPLE_UNKNOWN: WinFrame = {
  wseq: 1_000_007,
  open: { type: "anvil", title: "Onarım ve Adlandırma", size: 3 },
  items: [
    item(0, "diamond_sword", 1, l("Elmas Kılıç", "Diamond Sword"), {
      durability: { left: 800, max: 1561 },
    }),
  ],
};

/** A Projects menu drawn by its own picture: its title is a move and a picture (IA-ART). */
export const SAMPLE_MARKET: WinFrame = {
  wseq: 1_000_008,
  open: { type: "generic_9x3", title: "\uf808\ue001", size: 27 },
  items: [
    item(10, "diamond", 1, l("Elmas", "Diamond"), {
      name: "Elmas sat",
      lore: ["Tanesi 120 ripple"],
    }),
    item(12, "emerald", 1, l("Zümrüt", "Emerald"), {
      name: "Zümrüt sat",
      lore: ["Tanesi 80 ripple"],
    }),
    item(14, "gold_ingot", 1, l("Altın Külçesi", "Gold Ingot"), {
      name: "Altın sat",
      lore: ["Tanesi 40 ripple"],
    }),
    item(16, "writable_book", 1, l("Kitap ve Tüy", "Book and Quill"), { name: "Satış geçmişi" }),
  ],
};

/** An inventory with an item on the cursor, as after a pickup. */
export function withCursor(frame: InvFrame, slot: number): InvFrame {
  const picked = frame.items.find((it) => it.slot === slot);
  if (!picked) return frame;
  const { slot: _slot, ...cursor } = picked;
  return { ...frame, cursor, items: frame.items.filter((it) => it.slot !== slot) };
}
