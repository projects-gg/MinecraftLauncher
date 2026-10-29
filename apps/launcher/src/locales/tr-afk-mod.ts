// 7/24 Bulut's moderation on the player's side (services/cloud-afk README "Moderation player API",
// "Appeals"): the card's one sentence and "İtiraz et" (the form is the site's), "Durduruldu" on a
// paused row, a refused save with "Yanlışsa bildir", and the moderation notice. The same words as
// afk.projects.gg and the phone; never a rule's internals, a pattern, an id or a code. Root key
// `afkMod`.
export const trAfkMod = {
  categories: {
    advertising: "Reklam",
    spam: "Spam",
    scam: "Dolandırıcılık",
    rmt: "Gerçek parayla ticaret",
    impersonation: "Yetkili taklidi",
    harassment: "Hakaret",
    hate: "Nefret söylemi",
    sexual: "Uygunsuz içerik",
    minor_safety: "Gençler için tehlikeli içerik",
    personal_info: "Kişisel bilgi",
    off_platform: "Başka uygulamaya çekme",
    evasion: "Kuraldan kaçış",
    automation: "Otomatik tıklama",
    other: "Kural ihlali",
  },
  /** The card's sentence by the step in force (a stop says it on the state line instead). */
  card: {
    warn: "Uyarı aldı: {{kind}}",
    restrict_rows: "Bir satır durduruldu: {{kind}}",
    pause_ai: "AFK+ yapay zekâsı durduruldu: {{kind}}",
    suspend: "Durduruldu: {{kind}}",
    appeal: "İtiraz et",
    appealOpen: "İtirazın inceleniyor.",
  },
  /**
   * Under a moderation stop's state line: when it opens again, where the appeal stands, what
   * happens to the days while the decision may still be lifted. Days come with their case ending
   * ("3 Kasım'a", "29 Ekim'de"), as on the site.
   */
  hint: {
    until: "Hesap {{date}} yeniden açılır.",
    appeal: "{{date}} kadar itiraz edebilirsin.",
    open: "İtirazın inceleniyor.",
    upheld: "İtirazını inceledik, karar değişmedi.",
    days: "Karar kaldırılırsa durduğu günler hesabına eklenir.",
  },
  /** The state line of an account a moderation case stopped. */
  stateLine: "Durduruldu: {{kind}}",
  /** The line under a row a case paused. */
  paused: {
    line: "Durduruldu: {{kind}}. Düzeltip kaydedince yeniden çalışır.",
  },
  /** A save turned away (README "Saving"): not a punishment. */
  refused: "Bu satır {{kind}} içeriyor gibi görünüyor. Düzelt ya da sil.",
  /** Under a refused save: the player tells staff it was wrong (README "Refusal reports"). */
  report: {
    link: "Yanlışsa bildir",
    sending: "Gönderiliyor",
    sent: "Sağ ol, bir yetkili bakacak. Bu arada satırı düzeltip kaydedebilirsin.",
    limited: "Bugün en çok 3 kez bildirebilirsin.",
    notRefused: "Bu satır artık kaydedilebilir.",
  },
  /** The `moderation` notice's line under the account's name (the device writes it). */
  notice: {
    warn: "Uyarı: {{kind}}. Bir daha olursa hesap durdurulur.",
    restrict_rows: "Bir satırın durduruldu ({{kind}}). Düzelt ya da sil.",
    pause_ai: "AFK+ yapay zekâsı durduruldu ({{kind}}). Hesabın AFK'de kalıyor.",
    suspend: "Durduruldu: {{kind}}. Yanlışlık olduğunu düşünüyorsan itiraz et.",
    appeal_received: "İtirazın alındı. En geç 3 gün içinde bakacağız.",
    upheld: "İtirazını inceledik. Karar değişmedi.",
    overturned: "İtirazın kabul edildi. Verilen karar kaldırıldı.",
    lifted: "Verilen karar kaldırıldı.",
  },
};
