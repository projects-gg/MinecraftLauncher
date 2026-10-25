// "Devam et" into a world (OVL-FIX). With the in-game overlay the overlay opens the world itself and
// nothing is said; without it, a mod that breaks the game's direct entry into worlds keeps the game
// on its main menu and one toast says why and what to do. `mods` is the mod's own name ("Remove
// Reloading Screen"), never translated.
export const trWorldEntry = {
  /** The toast while the game starts on its main menu instead of the world. */
  fromTitle: "{{mods}} varken oyun dünyana doğrudan giremez. Dünyanı ana menüden aç.",
};
