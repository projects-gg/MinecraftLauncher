// The stand-ins for items with no icon yet (no atlas loaded, an unknown or custom item outside its
// server): one 16 x 16 silhouette per category in one flat tone (no light, no outline, no texture),
// so a stand-in is never taken for a real item. A category gets its own shape only where the game
// never draws that shape as an empty-slot hint: a block's three faces, an apple, a book's spine.
// Tools, weapons, armour and potions are exactly the game's hints (the smithing table's sword and
// pickaxe, the armour slots, the brewing stand's bottle) and the inventory's own empty-armour
// outlines, so a stand-in shaped like them reads as an empty slot; they, and anything else, show a
// tied sack: an item whose picture is not here. "#" is the silhouette, "." is nothing.

/** A sack tied at the neck (our own drawing; the game's bundle has a wide folded top). */
const SACK = [
  "................",
  ".....##..##.....",
  "......####......",
  ".......##.......",
  "......####......",
  ".....######.....",
  "....########....",
  "...##########...",
  "..############..",
  "..############..",
  "..############..",
  "..############..",
  "..############..",
  "...##########...",
  "....########....",
  "................",
] as const;

export const FALLBACK_GLYPHS = {
  /** A block: its three faces. */
  block: [
    ".......##.......",
    ".....######.....",
    "...##########...",
    ".##############.",
    ".#..########..#.",
    ".###..####..###.",
    ".#####....#####.",
    ".######.#######.",
    ".######.#######.",
    ".######.#######.",
    ".######.#######.",
    ".######.#######.",
    "...####.#####...",
    ".....##.###.....",
    ".......##.......",
    "................",
  ],
  tool: SACK,
  weapon: SACK,
  armor: SACK,
  /** Food: an apple. */
  food: [
    "................",
    ".........#......",
    "........#.......",
    "...####.#.####..",
    "..############..",
    ".##############.",
    ".##############.",
    ".##############.",
    ".##############.",
    ".##############.",
    "..############..",
    "..############..",
    "...##########...",
    "....###..###....",
    "................",
    "................",
  ],
  potion: SACK,
  /** A book and its spine. */
  book: [
    "................",
    "................",
    "...#########....",
    "..##.########...",
    "..##.#########..",
    "..##.#########..",
    "..##.#########..",
    "..##.#########..",
    "..##.#########..",
    "..##.#########..",
    "..##.#########..",
    "..##.#########..",
    "..##.#########..",
    "..##.#########..",
    "...###########..",
    "................",
  ],
  other: SACK,
} as const satisfies Record<string, readonly string[]>;
