// The 7/24 Bulut settings as questions (SETUP-UX, SETUP-UX-L, 28 Oct 2026): the first setup asks
// one question a step, afterwards the same questions sit in one quiet list, each with its answer in
// plain words and "Değiştir". The questions, their order and their answers are afk.projects.gg's
// (its tr-setup.ts, QA-FIX-L), so the site and the launcher ask the same thing the same way; the
// fields under them keep their own words (tr-afkplus.ts `afkPlus.settings`, tr-cloud-afk.ts
// `cloudAfk.settings`). Sentences are put together in features/afk/cloud/setup/answers.ts; a
// fragment that starts with a comma or a space follows another one there. Root key `setup`.
export const trSetup = {
  change: "Değiştir",
  // An open question folds with this (the site's word: "Kapat" read as closing the panel).
  close: "Gizle",
  advanced: "Gelişmiş",
  // The list's last row before the launcher's own "Bildirimler": what no question holds.
  other: "Diğer ayarlar",
  unsaved: "Kaydedilmemiş değişiklik var",
  marked: "Düzeltilecek alan var",
  chatCaption: "Sohbette böyle görünür",
  pmCaption: "Özel mesajda böyle görünür",
  later: "Daha sonra",
  back: "Geri",
  next: "Devam",
  count: "{{step}} / {{total}}",
  questionsLabel: "Ayar soruları",
  stepsLabel: "Kurulum adımları",

  plus: {
    title: "ProBot AI'ı kur",
    railFoot: "Başlatana kadar kimseye yazmaz.",
    start: "Başlat",
    starting: "Başlatılıyor",
    started: "ProBot AI çalışmaya başladı",
    changeLater: "Hepsini sonra Ayarlar'dan değiştirebilirsin.",
    resume: "Kuruluma devam et",
    // Under the title: where it works, in one line (the old first step's place).
    where: "{{server}} sunucusunda konuşur ve anlaşır.",
    // The talk preview: another player sells what the first buy order buys (an elytra without
    // one), ProBot AI offers in the chosen tone, never past one deal's limit. A gamer tag.
    previewPlayer: "Kuzgun_21",
    chatThem: "elytra satıyorum",
    chatSell: "{{item}} satıyorum",
    // The offer as it writes it: {{amount}} "32.000" in full words, "32k" like a player; {{amountDat}}
    // with its ending ("32.000'e", "32k'ya").
    samples: {
      friendly: "Merhaba, {{amount}} olur mu?",
      brief: "{{amount}}?",
      polite: "Merhaba, {{amount}} sizin için uygun olur mu?",
      playful: "Selam, {{amountDat}} el sıkışalım mı?",
    },
    samplesPlayer: {
      friendly: "slm knk, {{amount}} olur mu",
      brief: "{{amount}}?",
      polite: "merhaba, {{amount}} olur mu?",
      playful: "{{amountDat}} anlaşalım mı xd",
    },
    questions: {
      orders: "Ne alıp satsın?",
      money: "Ne kadar harcayabilsin?",
      act: "ProBot AI ne yapsın?",
      chat: "Sohbette ne yapsın?",
      talk: "Nasıl konuşsun?",
      who: "Kimlere cevap versin?",
      time: "Ne zaman çalışsın?",
      // The launcher's own last row (the phone and this computer get the notices).
      notify: "Bildirimler",
    },
    ledes: {
      orders: "Kendi sözlerinle yaz, ProBot AI siparişe çevirir.",
      money: "Yalnız oyundaki parayla harcar, bu sınırları asla aşmaz.",
      chat: "İstersen etkin siparişlerini genel sohbette duyurur.",
      talk: "Özel mesajlara bu tarzda cevap verir. Sorulursa yapay zekâ olduğunu söyler.",
    },
    // "A limit or no buying for now" is one choice.
    notNow: "Şimdilik alım yapmasın",
    notNowHelp: "Aradığını bulunca yalnız haber verir.",
    required: 'İki tutarı da yaz ya da "Şimdilik alım yapmasın"ı aç.',
    // How far the limits go with the first buy order's item, in the game's slots.
    fits: "Bu sınırlarla",
    fitDeal: "Tek anlaşmada {{count}} {{item}}",
    fitTotal: "Toplamda {{count}} {{item}}",
    fitWhole: "Siparişin tamamı tek anlaşmaya sığar: {{count}} {{item}}",
    fitTotalWhole: "Toplamda siparişin tamamı: {{count}} {{item}}",
    fitAt: "Tanesi {{price}} olursa.",
    // The meter under the money answer.
    spentOfTotal: "Toplam sınırdan {{amount}} harcandı",
    sellOnly: "Satış için harcama gerekmez",
    noOrders: "Sipariş yok",
    active: "Etkin",
    // Under "Alış" and "Satış" in "ProBot AI ne yapsın?".
    sidesHelp: { buy: "Kapatırsan hiçbir şey almaz.", sell: "Kapatırsan hiçbir şey satmaz." },
    // "Sohbette ne yapsın?": on means it scans the chat and writes to a seller once (both on by
    // default, said, not hidden); off keeps the scan and stops the writing.
    writeSellers: "Satılık ilan görünce satıcıya bir kez yazsın",
    // The answers under each question.
    answers: {
      orders: "{{count}} sipariş: {{names}}",
      money: "Tek anlaşmada {{perDeal}}, toplam {{total}}",
      noBuying: "Alım yapmaz, aradığını bulunca haber verir.",
      reply: "Pazarlık etmez, teklifleri sana iletir.",
      ask: "Pazarlık eder, anlaşınca sana sorar.",
      deal: "Sınırlarının içinde sormadan tamamlar.",
      dealAsks: "{{amount}} üstü alımlarda önce sana sorar.",
      // Both sides are the default: said only when one is off.
      buyOnly: "Yalnız alır.",
      sellOnly: "Yalnız satar.",
      neither: "Alış ve satış kapalı.",
      // {{every}}: "saatte", "2 saatte", "45 dakikada" (`everyHour`, `everyHours`, `everyMinutes`).
      ads: "{{every}} bir ilan verir",
      noAds: "İlan vermez",
      // An older service (no interval): the ads are on, nothing more to say.
      adsPlain: "İlan verir",
      adsStopped: "Bugün ilan vermez",
      writes: ", satılık ilan görünce satıcıya bir kez yazar.",
      scans: ", satış ilanlarını tarar.",
      noScan: ".",
      tone: "{{preset}}, oyuncu gibi",
      whoAll: "Herkesin her özel mesajına cevap verir",
      whoAllFriends: "Arkadaşlarının her özel mesajına cevap verir",
      whoTrade: "Herkesin takas mesajlarına cevap verir",
      whoTradeFriends: "Arkadaşlarının takas mesajlarına cevap verir",
      whoOff: "Yalnız hazır cevaplar verir",
      away: ", başka mesajlara uzakta olduğunu söyler.",
      noAway: ".",
      blocked: " {{count}} oyuncuyu engelledin.",
      always: "Her zaman",
      hours: "Yalnız seçtiğin saatlerde",
      // {{when}} comes with its ending ("30 Ekim'e").
      onBreak: "{{when}} kadar molada",
      more: "Kontör tasarrufu, günlük özet",
      moreNoSaver: "Günlük özet",
      notifyOn: "Hepsi açık",
      notifySome: "{{count}} bildirim kapalı",
      notifyQuiet: ", sessiz saatler açık",
    },
    // What each question's "Gelişmiş" holds, said while it is folded.
    adv: {
      money: "Günlük sınır, en az kalacak bakiye",
      moneyCurrency: "Günlük sınır, en az kalacak bakiye, para birimi",
      act: "Ödeme, onay bekleme süresi",
      actNoPay: "Onay bekleme süresi",
      chat: "Satış ilanlarını taramak",
      // After `chat` while the ad's text can be chosen (ads on, more than one in the profile).
      chatText: ", ilan metni",
      talk: "Tanıtım yazısı, mesaj etiketi, dil, konu sınırı",
    },
    ads: {
      note: "Sunucu spam saymasın diye her ilanı biraz farklı yazar, uyarı gelirse daha seyrek ilan verir.",
      everyHour: "saatte",
      everyHours: "{{n}} saatte",
      everyMinutes: "{{n}} dakikada",
    },
  },

  account: {
    title: "Hesabını hazırla",
    finish: "Kuruluma devam et",
    done: "Bitir",
    // Under "Bitir" while the account runs: saving reconnects it.
    reconnects: "Kaydedince hesap yeniden bağlanır.",
    questions: {
      server: "Nereye girsin?",
      login: "Nasıl giriş yapsın?",
      join: "Girince ne yapsın?",
      replies: "Biri yazınca ne desin?",
    },
    ledes: {
      server: "Hesabın bu sunucuya girer ve orada AFK kalır.",
      login: "Sunucu şifre soruyorsa giriş komutunu yaz. Bu bilgisayarda şifrelenip gönderilir.",
      join: "Girişten sonra sırayla yazar.",
      replies: "Sohbette bir söz geçince hesabın yanıt verir.",
    },
    answers: {
      server: "{{host}}, sürüm {{version}}",
      versionAuto: "otomatik",
      login: "Girişte {{text}} yazar",
      // Two or more rows: never one.
      loginMany: "Girişte {{n}} gizli komut yazar",
      noLogin: "Giriş komutu yok",
      join: "{{delay}} sn sonra {{text}} yazar",
      joinMany: "{{n}} komut yazar: {{text}} ve diğerleri",
      noJoin: "Bir şey yazmaz",
      replies: "{{count}} kural: {{words}}",
      noReplies: "Yanıt vermez",
      // The launcher's "Diğer ayarlar" (the name you play with is under the replies' "Gelişmiş").
      more: "AFK engelleme, tekrarlanan komutlar, konsol dili, oyuncu adı",
    },
    // Under "Yalnız bana yanıt versin" while the name it listens to is not written yet.
    ownerFirst: "Önce Gelişmiş'te oynadığın hesabın adını yaz.",
    loginAdd: "Giriş komutu ekle",
    loginEmpty: "Sunucu şifre sormuyorsa boş bırakabilirsin.",
    repeatTitle: "Tekrarlanan komutlar",
    behaviourTitle: "Davranış",
    // Two ready-made rules: the replies players set up most.
    ready: "Hazır kurallar",
    templates: {
      away: {
        title: 'Biri "selam" yazınca uzakta olduğunu söylesin',
        reply: 'Yanıtı: "Merhaba, şu an klavye başında değilim."',
        match: "selam",
        text: "Merhaba, şu an klavye başında değilim.",
      },
      login: {
        title: 'Sunucu "giriş yap" deyince şifreni yazsın',
        reply: "Yanıtı: giriş komutun, gizli",
        match: "giriş yap",
      },
    },
    add: "Ekle",
    added: "Eklendi",
    chatThem: "selam",
  },
};
