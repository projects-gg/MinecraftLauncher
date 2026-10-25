// "Devam et" into a world (OVL-FIX). With the in-game overlay the overlay opens the world itself and
// nothing is said; without it, a mod that breaks the game's direct entry into worlds keeps the game
// on its main menu and one toast says what to do (its title) and why (under it). `mods` is the mod's
// own name ("Remove Reloading Screen"), never translated; "Tek Oyunculu" is the game's own button.
export const trWorldEntry = {
  /** The toast's title while the game starts on its main menu instead of the world. */
  fromTitle: "Dünyanı Tek Oyunculu'dan aç",
  /** Under it: why the world did not open by itself. */
  fromTitleWhy: "{{mods}} modu varken doğrudan açılamıyor.",
};
