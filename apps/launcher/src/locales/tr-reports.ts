// "Rapor et" (MOD1): one dialog for players, crews and messages. The reported person never learns
// who reported them; the note is for moderators only.
export const trReports = {
  action: "Rapor Et",
  title: "Rapor Et",
  crewAction: "Ekibi Rapor Et",
  what: {
    label: "Neyi rapor ediyorsun?",
    crew: "Ekibi",
    inviter: "Davet Edeni",
  },
  reasonsLabel: "Neden",
  reasons: {
    playerName: "Uygunsuz Ad",
    crewName: "Uygunsuz Ad, Etiket ya da Slogan",
    profile: "Uygunsuz Profil Resmi",
    harassment: "Taciz ya da Zorbalık",
    inappropriate: "Uygunsuz Mesaj",
    spam: "Spam",
    scam: "Dolandırıcılık",
    other: "Diğer",
    // BAN-TYPES: offered only for a player reported from a voice room.
    voice: "Sesli Sohbet İhlali",
  },
  note: "Not",
  notePlaceholder: "İstersen kısaca anlat",
  noteHelp: "Yalnız yetkililer görür.",
  block: "Ayrıca Engelle",
  private: "Kimin rapor ettiği hiçbir zaman söylenmez.",
  message: "Mesaj",
  send: "Gönder",
  cancel: "Vazgeç",
  sent: "Raporun iletildi.",
  failed: "Rapor gönderilemedi.",
};
