// Connecting an AFK account whose client has to be fetched first (the card's state line, the line
// under it and the notices elsewhere), and the launch failures the general failure words
// (afk.failure) do not cover. Short, sentence case; the retry button is common.retry.
export const trAfkConnect = {
  /** The state line while the client is looked up in the content or unpacked. */
  preparing: "AFK istemcisi hazırlanıyor",
  /** The state line while it downloads; the percent follows it. */
  downloading: "AFK istemcisi indiriliyor",
  /** Turkish puts the sign before the figure. */
  percent: "%{{value}}",
  /** Replaces "Bağla" while the client is fetched: stops the download for this account. */
  cancel: "İptal",
  /**
   * Why the client could not be had: what happened (the state line) and what to do (the line
   * under it).
   */
  problem: {
    notPublished: {
      title: "AFK istemcisi henüz hazır değil",
      hint: "Biraz sonra tekrar dene.",
    },
    offline: {
      title: "AFK istemcisi indirilemedi",
      hint: "İnternet bağlantını kontrol et.",
    },
    diskFull: {
      title: "Diskte yer kalmadı",
      hint: "Biraz yer açıp tekrar dene.",
    },
    blocked: {
      title: "AFK istemcisi kurulamadı",
      hint: "Antivirüsün engellemediğinden emin ol.",
    },
    broken: {
      title: "AFK istemcisi indirilemedi",
      hint: "Biraz sonra tekrar dene.",
    },
  },
  /** Notices while the AFK page is not in front: the accounts never got to connect. */
  notice: {
    failed: "{{name}} bağlanamadı",
    failedMany: "{{count}} AFK hesabı bağlanamadı",
  },
  /** Why a client stopped where the general words do not fit; never the technical message. */
  failure: {
    /** The client reported a failure without a reason (its own words follow in brackets). */
    failed: "Bağlanamadı",
    unexpectedExit: "AFK istemcisi beklenmedik şekilde kapandı",
    launchFailed: "AFK istemcisi başlatılamadı",
    clientMissing: "AFK istemcisi bulunamadı",
    secretsUnavailable: "Giriş komutları okunamadı",
  },
};
