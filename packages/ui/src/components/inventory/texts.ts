// The inventory's words. Apps may pass their own (`texts`); these are the defaults, with the game's
// own Turkish where the game has a word for it (tr_tr.json: "Araç Çubuğu", "Diğer El",
// "Dayanıklılık: %s / %s", "ve %s tane daha..."). Placeholders: {item}, {n}, {left}, {max}, {p},
// {title}.

export interface InventoryTexts {
  /** The player inventory's label in a menu, and the grid's name. */
  inventory: string;
  hotbar: string;
  /** The equipment row (armour and the off hand): its grid's name and the title of the player's own screen. */
  equipment: string;
  /** A window without a title. */
  menu: string;
  closeMenu: string;
  empty: string;
  head: string;
  chest: string;
  legs: string;
  feet: string;
  offhand: string;

  /** Spoken after an item's name. */
  count: string;
  durabilityPercent: string;
  enchanted: string;
  inHand: string;
  onCursor: string;
  pending: string;

  /** Tooltip lines, as the game writes them. */
  durability: string;
  insideStack: string;
  /** More stacks inside than the tooltip lists: how many, when the frame tells. */
  insideMore: string;
  insideMoreUnknown: string;
  partial: string;

  /** Menu rows: the game's clicks by name. */
  take: string;
  takeHalf: string;
  place: string;
  swap: string;
  placeOne: string;
  putBack: string;
  /** Shift-click, named by where the stack goes. */
  toInventory: string;
  toHotbar: string;
  putOn: string;
  toOffhand: string;
  /**
   * With a window open the server decides what a click does (a shop buys, a menu opens): the rows
   * name the click itself, as servers' item descriptions do ("Sol Tık: Satın al").
   */
  click: string;
  rightClick: string;
  shiftClick: string;
  hold: string;
  use: string;
  /** A single item; `dropOne` and `dropStack` for more. */
  drop: string;
  dropOne: string;
  dropStack: string;

  /** What happened, for screen readers. */
  pickedUp: string;
  placed: string;
  swapped: string;
  moved: string;
  dropped: string;
  used: string;
  held: string;
  putBackDone: string;
  menuOpened: string;
  menuClosed: string;

  /** Answers to moves, and the lines that stay while they hold. */
  slowDown: string;
  refused: string;
  tradeLocked: string;
  viewOnly: string;
  notRunning: string;
  unreachable: string;
  off: string;
  /** Under a finger, while a stack is carried. */
  carryHint: string;
}

export type InventoryLang = "tr" | "en";

export const INVENTORY_TEXTS: Record<InventoryLang, InventoryTexts> = {
  tr: {
    inventory: "Envanter",
    hotbar: "Araç çubuğu",
    equipment: "Ekipman",
    menu: "Menü",
    closeMenu: "Menüyü oyunda kapat",
    empty: "Boş",
    head: "Kafa",
    chest: "Göğüs",
    legs: "Bacak",
    feet: "Ayak",
    offhand: "Diğer el",

    count: "{n} adet",
    durabilityPercent: "dayanıklılık yüzde {p}",
    enchanted: "büyülü",
    inHand: "elde",
    onCursor: "İmleçte: {item}",
    pending: "bekliyor",

    durability: "Dayanıklılık: {left} / {max}",
    insideStack: "{item} x {n}",
    insideMore: "ve {n} tane daha...",
    insideMoreUnknown: "ve daha fazlası...",
    partial: "Bazı özellikleri gösterilemiyor.",

    take: "Al",
    takeHalf: "Yarısını al",
    place: "Bırak",
    swap: "Yer değiştir",
    placeOne: "Bir tane bırak",
    putBack: "Yerine koy",
    toInventory: "Envantere taşı",
    toHotbar: "Araç çubuğuna taşı",
    putOn: "Giy",
    toOffhand: "Diğer ele taşı",
    click: "Sol tık",
    rightClick: "Sağ tık",
    shiftClick: "Shift+sol tık",
    hold: "Elde tut",
    use: "Kullan",
    drop: "At",
    dropOne: "Bir tane at",
    dropStack: "Yığını at",

    pickedUp: "{item} alındı",
    placed: "{item} bırakıldı",
    swapped: "{item} ile yer değiştirdi",
    moved: "{item} taşındı",
    dropped: "{item} atıldı",
    used: "{item} kullanıldı",
    held: "{item} elde",
    putBackDone: "{item} yerine kondu",
    menuOpened: "Menü açıldı: {title}",
    menuClosed: "Menü kapandı",

    slowDown: "Biraz yavaşla. Son tıklama sayılmadı.",
    refused: "Sunucu bu hamleye izin vermedi.",
    tradeLocked: "Takas sürerken envanter kilitli.",
    viewOnly: "Bu envanterde değişiklik yapamazsın.",
    notRunning: "Hesap oyunda değilken envanter görünmez.",
    unreachable: "Hesaba şu an ulaşılamıyor.",
    off: "Envanter görünümü bu hesapta kapalı.",
    carryHint: "Bırakmak için bir yuvaya dokun.",
  },
  en: {
    inventory: "Inventory",
    hotbar: "Hotbar",
    equipment: "Equipment",
    menu: "Menu",
    closeMenu: "Close the menu in game",
    empty: "Empty",
    head: "Head",
    chest: "Chest",
    legs: "Legs",
    feet: "Feet",
    offhand: "Off hand",

    count: "{n} items",
    durabilityPercent: "durability {p} percent",
    enchanted: "enchanted",
    inHand: "in hand",
    onCursor: "On the cursor: {item}",
    pending: "waiting",

    durability: "Durability: {left} / {max}",
    insideStack: "{item} x{n}",
    insideMore: "and {n} more...",
    insideMoreUnknown: "and more...",
    partial: "Some details can't be shown.",

    take: "Take",
    takeHalf: "Take half",
    place: "Place",
    swap: "Swap",
    placeOne: "Place one",
    putBack: "Put back",
    toInventory: "Move to inventory",
    toHotbar: "Move to hotbar",
    putOn: "Put on",
    toOffhand: "Move to off hand",
    click: "Left click",
    rightClick: "Right click",
    shiftClick: "Shift+left click",
    hold: "Hold",
    use: "Use",
    drop: "Drop",
    dropOne: "Drop one",
    dropStack: "Drop stack",

    pickedUp: "Picked up {item}",
    placed: "Placed {item}",
    swapped: "Swapped with {item}",
    moved: "Moved {item}",
    dropped: "Dropped {item}",
    used: "Used {item}",
    held: "Holding {item}",
    putBackDone: "Put {item} back",
    menuOpened: "Menu opened: {title}",
    menuClosed: "Menu closed",

    slowDown: "Slow down a little. That click didn't count.",
    refused: "The server didn't allow that move.",
    tradeLocked: "The inventory is locked during a trade.",
    viewOnly: "You can't make changes to this inventory.",
    notRunning: "The inventory shows while the account is in game.",
    unreachable: "Can't reach the account right now.",
    off: "Inventory view is off for this account.",
    carryHint: "Tap a slot to put it down.",
  },
};

/** Fills `{name}` placeholders; unknown ones stay as written. */
export function fill(template: string, values: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (whole, key: string) =>
    key in values ? String(values[key]) : whole,
  );
}
