// Potion colours by potion id (the effect's colour, the way the game tints the bottle's liquid);
// water and the effectless potions keep the atlas's default.

const SLOWNESS = 0x8bafe0;
const RESISTANCE = 0x9146f0;

/** The weighted mix the game shows for a potion of several effects (turtle master). */
function mix(a: number, wa: number, b: number, wb: number): number {
  const channel = (shift: number) =>
    Math.round((((a >> shift) & 255) * wa + ((b >> shift) & 255) * wb) / (wa + wb));
  return (channel(16) << 16) | (channel(8) << 8) | channel(0);
}

export const POTION_COLORS: Readonly<Record<string, number>> = {
  night_vision: 0xc2ff66,
  invisibility: 0xf6f6f6,
  leaping: 0xfdff84,
  fire_resistance: 0xff9900,
  swiftness: 0x33ebff,
  slowness: SLOWNESS,
  turtle_master: mix(SLOWNESS, 4, RESISTANCE, 3),
  water_breathing: 0x98dac0,
  healing: 0xf82423,
  harming: 0xa9656a,
  poison: 0x87a363,
  regeneration: 0xcd5cab,
  strength: 0xffc700,
  weakness: 0x484d48,
  luck: 0x59c106,
  slow_falling: 0xf3cfb9,
  wind_charged: 0xbdc9ff,
  weaving: 0x78695a,
  oozing: 0x99ffa3,
  infested: 0x8c9b8c,
};
