// CREW-PLUS: crew plan templates and how long a friend has been playing. Chips, buttons and menu
// rows in Title Case (each word's first letter, Turkish İ and I); sentences as sentences.
export const trCrewPlus = {
  templates: {
    label: "Plan şablonları",
    builtIn: {
      tonight: { name: "Bu Akşam", note: "Sesliye gelin" },
      friday: { name: "Cuma Akşamı", note: "Herkes gelsin" },
      weekend: { name: "Hafta Sonu", note: "Uzun oyun" },
    },
    fill: "{{name}}: {{when}}",
    save: "Şablonlara Ekle",
    saved: "Şablonlara eklendi",
    exists: "Bu şablon zaten var.",
    full: "En çok 5 şablonun olabilir. Yenisi için birini sil.",
    rename: "Yeniden Adlandır",
    name: "Şablonun adı",
    renameSave: "Kaydet",
    renameCancel: "Vazgeç",
    remove: "Sil",
    removed: "Şablon silindi",
    undo: "Geri Al",
  },
  privacy: {
    playTime: "Oynama Süremi Göster",
    playTimeNeedsSeen: "Son görülmen açıkken görünür.",
  },
  played: {
    minutes: "{{count}} dk",
    hours: "{{count}} sa",
    spoken: {
      minutes: "{{count}} dakikadır oyunda",
      hours: "{{count}} saattir oyunda",
    },
  },
};
