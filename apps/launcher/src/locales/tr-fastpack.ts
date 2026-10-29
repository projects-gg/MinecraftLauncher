// Settings → Genel "Hızlı kaynak paketi yükleme" (FASTPACK). Short, sentence case, "sen". The line
// under the switch says what it does; where it does nothing, the reason takes its place and names
// the thing, and the switch is held off and dimmed (lead, 29 Oct, TASTE-SWEEP FASTPACK-L r1:
// "kaynak paketi" as everywhere else in the launcher and the game).
export const trFastPack = {
  /** The switch: on by default, shown while Studio's `features.fastPacks` is on. */
  setting: "Hızlı kaynak paketi yükleme",
  /** What it does, under the switch while it works for the install Play opens. */
  help: "Sunucuya girerken gelen kaynak paketini daha çabuk yükler.",
  /** In place of `help` while the launcher's pack has no fast-pack mod for the version Play opens. */
  missing: "Minecraft {{version}} için henüz yok.",
  /** In place of `help` while Play opens an install the launcher does not manage (the player's own). */
  notHere: "Yalnız Projects paketiyle kurulan oyunlarda çalışır.",
};
