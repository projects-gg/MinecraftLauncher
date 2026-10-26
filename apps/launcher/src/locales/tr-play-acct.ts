// "Oyunda" for the account in game alone (PLAY-ACCT), and the question "Oyna" asks while another
// account's game, or another installation's, runs (one game at a time). Short, "sen", one
// sentence about what happens to the game that is on. Names are never given a suffix; an
// installation is named with "kurulum", since "Projects" is also the server's name.
export const trPlayAcct = {
  /** The account center's and the account menu's label on the account whose game is on. */
  inGame: "Oyunda",
  switch: {
    /** Another account: the title names the account to play with. */
    titleAccount: "Oyun {{name}} hesabıyla açılsın mı?",
    /** The same account, another installation. */
    titleInstance: "{{instance}} kurulumu açılsın mı?",
    /** What happens to the game that is on, by how far it is. */
    openAccount: "{{name}} ile açık olan oyun kaydedilip kapanacak.",
    openAccountInstance:
      "{{name}} ile {{instance}} kurulumunda açık olan oyun kaydedilip kapanacak.",
    openInstance: "{{instance}} kurulumunda açık olan oyun kaydedilip kapanacak.",
    openingAccount: "{{name}} ile açılmakta olan oyun durdurulacak.",
    openingAccountInstance:
      "{{name}} ile {{instance}} kurulumunda açılmakta olan oyun durdurulacak.",
    openingInstance: "{{instance}} kurulumunda açılmakta olan oyun durdurulacak.",
    preparingInstance: "{{instance}} kurulumunun hazırlığı durdurulacak.",
    /** The same without the installation's name (the list not read yet). */
    openGame: "Açık olan oyun kaydedilip kapanacak.",
    openingGame: "Açılmakta olan oyun durdurulacak.",
    preparingGame: "Oyunun hazırlığı durdurulacak.",
    cancel: "Vazgeç",
    /** The answer: the game closes the way its own close button does, then this one starts. */
    close: "Kapat ve oyna",
    /** The answer while the game opens or its files are prepared: nothing to save yet. */
    stop: "Durdur ve oyna",
    /** On the way: the game closes (its words stay until it has). */
    closingTitle: "Oyun kapanıyor",
    closingAccount: "Kapanınca {{name}} hesabıyla açılacak.",
    closingInstance: "Kapanınca {{instance}} kurulumu açılacak.",
    closingGame: "Kapanınca oyun yeniden açılacak.",
    /** It has not closed in 20 s: the player may wait on, end it, or leave it open. */
    overdueTitle: "Oyun henüz kapanmadı",
    overdueBody: "Zorla kapatırsan kaydedilmemiş ilerleme kaybolabilir.",
    wait: "Beklemeye devam et",
    force: "Zorla kapat ve oyna",
    forcingTitle: "Oyun kapatılıyor",
  },
};
