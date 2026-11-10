// The owner's feedback of 22 October (FIX2): deleted chat messages' tombstones, crew plan notes
// and who answered, the same menu wherever a player or a crew shows, and "Bağlan". Short, sentence
// case; names are never followed by a suffix.
export const trFix2 = {
  /** What a deleted message's quiet bubble says, by who deleted it. */
  tombstone: {
    mine: "Bu mesajı sildin",
    sender: "Bu mesaj silindi",
    leader: "Ekip lideri sildi",
    moderator: "Ekip moderatörü sildi",
    staff: "Projects moderatörleri kaldırdı",
  },
  /** The same menu for a person or a crew wherever they show. */
  menu: {
    chat: "Mesaj",
    addFriend: "Arkadaş Ekle",
    requestSent: "İstek Gönderildi",
    requestSentTo: "Arkadaşlık isteği gönderildi",
    acceptFriend: "İsteği Kabul Et",
    crewChat: "Sohbeti Aç",
    you: "Sen",
  },
  /** A crew plan's note. */
  note: {
    label: "Plan notu",
    placeholder: "Kısa bir not ekle (isteğe bağlı)",
    count: "{{count}}/80",
    link: "Notta bağlantı olamaz.",
    phone: "Notta telefon numarası olamaz.",
    long: "Not en çok 80 karakter olabilir.",
    write: "Not Ekle",
    edit: "Notu Düzenle",
    clear: "Notu Kaldır",
    save: "Kaydet",
    cancel: "Vazgeç",
    saved: "Not kaydedildi",
    cleared: "Not kaldırıldı",
    gone: "Bu plan artık yok.",
    /** A line about a plan (its chat line, its toast, its reminder) with the plan's note. */
    with: '{{text}}: "{{note}}"',
  },
  /** Who answered a plan, opened under its faces. */
  answers: {
    toggle: "Kimler geliyor",
    yes: "Geliyor",
    maybe: "Belki",
    no: "Gelemiyor",
    more_one: "{{count}} Kişi Daha",
    more_other: "{{count}} Kişi Daha",
    less: "Daha Az Göster",
  },
  /** "Rapor et" on a plan card: its maker, the plan as evidence. */
  report: {
    plan: "Plan: {{when}}",
    noteReason: "Uygunsuz Not",
  },
  /**
   * "Bağlan" (a call, an invite, a plan, a friend on Projects) while a game is open: the game
   * moves there in game (the overlay) or closes the way its X does and starts again into it.
   */
  connect: {
    connect: "Bağlan",
    cancel: "Vazgeç",
    address: "Sunucu adresi",
    moveTitle: "Sunucuya geçilsin mi?",
    moveBody: "Oyunda {{server}} sunucusuna geçeceksin. Olmazsa oyun yeniden açılır.",
    restartTitle: "Oyun yeniden açılsın mı?",
    restartBody: "Oyun kapanıp {{server}} sunucusunda yeniden açılır.",
    reopenBody: "Oyun baştan açılıp {{server}} sunucusuna bağlanacak.",
    saved: "Dünyan kaydedilir.",
    movingTitle: "Bağlanılıyor",
    movingBody: "{{server}} sunucusuna geçiliyor.",
    closingTitle: "Oyun kapanıyor",
    saving: "Dünyan kaydediliyor.",
    closingBody: "Oyun kapanınca {{server}} sunucusunda yeniden açılacak.",
    overdueTitle: "Oyun henüz kapanmadı",
    overdueBody: "Zorla kapatırsan dünyandaki kaydedilmemiş ilerleme kaybolabilir.",
    wait: "Beklemeye Devam Et",
    force: "Zorla Kapat",
    forcingTitle: "Oyun kapatılıyor",
    connecting: "{{server}} sunucusuna bağlanılıyor",
  },
};
