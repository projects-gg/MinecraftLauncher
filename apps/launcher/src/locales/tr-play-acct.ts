// "Oyunda" for the account in game alone (PLAY-ACCT), and the question "Oyna" asks while another
// launch is on its way (one launch at a time; games on screen stay, PLAY-AGAIN). Short, "sen", one
// sentence about what happens to the launch on its way. Names are never given a suffix; an
// installation is named with "kurulum", since "Projects" is also the server's name.
export const trPlayAcct = {
  /** The account center's and the account menu's label on the account whose game is on. */
  inGame: "Oyunda",
  switch: {
    /** Another account: the title names the account to play with. */
    titleAccount: "Oyun {{name}} hesabıyla açılsın mı?",
    /** The same account, another installation. */
    titleInstance: "{{instance}} kurulumu açılsın mı?",
    openingAccount: "{{name}} ile açılmakta olan oyun durdurulacak.",
    openingAccountInstance:
      "{{name}} ile {{instance}} kurulumunda açılmakta olan oyun durdurulacak.",
    openingInstance: "{{instance}} kurulumunda açılmakta olan oyun durdurulacak.",
    preparingInstance: "{{instance}} kurulumunun hazırlığı durdurulacak.",
    openingGame: "Açılmakta olan oyun durdurulacak.",
    preparingGame: "Oyunun hazırlığı durdurulacak.",
    cancel: "Vazgeç",
    /** The answer while the game opens or its files are prepared: nothing to save yet. */
    stop: "Durdur ve oyna",
    /** On the way: the game closes (its words stay until it has). */
    closingTitle: "Oyun kapanıyor",
    closingAccount: "Kapanınca {{name}} hesabıyla açılacak.",
    closingInstance: "Kapanınca {{instance}} kurulumu açılacak.",
    closingGame: "Kapanınca oyun yeniden açılacak.",
  },
};
