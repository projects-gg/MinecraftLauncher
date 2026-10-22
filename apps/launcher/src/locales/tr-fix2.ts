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
    chat: "Mesaj gönder",
    addFriend: "Arkadaş ekle",
    requestSent: "İstek gönderildi",
    requestSentTo: "Arkadaşlık isteği gönderildi",
    acceptFriend: "İsteği kabul et",
    crewChat: "Sohbeti aç",
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
    write: "Not ekle",
    edit: "Notu düzenle",
    clear: "Notu kaldır",
    save: "Kaydet",
    cancel: "Vazgeç",
    saved: "Not kaydedildi",
    cleared: "Not kaldırıldı",
    gone: "Bu plan artık yok.",
    /** A line about a plan (its chat line, its toast, its reminder) with the plan's note. */
    with: "{{text}}: “{{note}}”",
  },
  /** Who answered a plan, opened under its faces. */
  answers: {
    toggle: "Kimler geliyor",
    yes: "Geliyor",
    maybe: "Belki",
    no: "Gelemiyor",
    more_one: "{{count}} kişi daha",
    more_other: "{{count}} kişi daha",
    less: "Daha az göster",
  },
  /** "Rapor et" on a plan card: its maker, the plan as evidence. */
  report: {
    plan: "Plan: {{when}}",
    noteReason: "Uygunsuz not",
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
    moveBody: "Oyunda {{server}} sunucusuna geçeceksin; olmazsa oyun yeniden açılır.",
    restartTitle: "Oyun yeniden açılsın mı?",
    restartBody: "Oyun kapanacak ve {{server}} sunucusuna bağlanarak yeniden açılacak.",
    reopenBody: "Oyun baştan açılıp {{server}} sunucusuna bağlanacak.",
    saved: "Dünyan kaydedilir.",
    movingTitle: "Bağlanılıyor",
    movingBody: "{{server}} sunucusuna geçiliyor.",
    closingTitle: "Oyun kapanıyor",
    saving: "Dünyan kaydediliyor.",
    closingBody: "Oyun kapanınca {{server}} sunucusuna bağlanacak.",
    overdueTitle: "Oyun kapanmadı",
    overdueBody: "Zorla kapatırsan kaydedilmemiş tek oyunculu ilerleme kaybolabilir.",
    wait: "Beklemeye devam et",
    force: "Zorla kapat",
    forcingTitle: "Oyun kapatılıyor",
    connecting: "{{server}} sunucusuna bağlanılıyor",
  },
};
