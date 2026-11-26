// CUSTOM-VER: "Özel" in "Yeni Kurulum" (features.customVersions): versions other installers
// (OptiFine, Forge, a modpack's own) put in the data folder. Short, "sen"; titles and buttons in
// Title Case (owner, 7 Nov), lines as sentences. Versions are named as players know them
// ("OptiFine 1.21.11"); only a version of its own shows its own name.
export const trCustomVer = {
  /** The version filter's option in "Sıfırdan". */
  filter: "Özel",
  /** Under a version of its own: what it runs on (its loader, or OptiFine). */
  base: "{{loader}} ile Minecraft {{minecraft}}.",
  baseVanilla: "Minecraft {{minecraft}}.",
  empty: "Henüz özel sürüm yok.",
  failed: "Özel sürümler okunamadı.",
  /** Beside the list, where the loader is picked otherwise: how one gets there. */
  add: {
    title: "Sürüm Nasıl Eklenir",
    body: "OptiFine ya da Forge'u aç, klasör olarak veri klasörünün yolunu yapıştır. Kurduğun sürüm solda görünür.",
    copy: "Yolu Kopyala",
    copied: "Klasörün yolu kopyalandı.",
    open: "Klasörü Aç",
  },
  /** The instance's details: a version of its own, by its name. */
  fact: "Özel Sürüm",
};
