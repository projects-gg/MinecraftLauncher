// "Oyunda" for the account in game alone (PLAY-ACCT), and the question "Oyna" asks while another
// account's game, or another installation's, runs (one game at a time). Short, "sen", one
// sentence about what happens to the game that is on. Names are never given a suffix.
export const trPlayAcct = {
  /** The account center's and the account menu's label on the account whose game is on. */
  inGame: "Oyunda",
  switch: {
    /** Another account: the title names the account to play with. */
    titleAccount: "Oyun {{name}} ile açılsın mı?",
    /** The same account, another installation. */
    titleInstance: "{{instance}} açılsın mı?",
    /** What happens to the game that is on, by how far it is. */
    openAccount: "{{name}} ile açık olan oyun kapanacak.",
    openAccountInstance: "{{name}} ile açık olan {{instance}} kapanacak.",
    openInstance: "Açık olan {{instance}} kapanacak.",
    openingAccount: "{{name}} ile açılmakta olan oyun durdurulacak.",
    openingAccountInstance: "{{name}} ile açılmakta olan {{instance}} durdurulacak.",
    openingInstance: "Açılmakta olan {{instance}} durdurulacak.",
    preparingInstance: "{{instance}} için hazırlık durdurulacak.",
    /** The same without the installation's name (the list not read yet). */
    openGame: "Açık olan oyun kapanacak.",
    openingGame: "Açılmakta olan oyun durdurulacak.",
    preparingGame: "Oyunun hazırlığı durdurulacak.",
    /** The answer: the game closes the way its own close button does, then this one starts. */
    close: "Kapat ve oyna",
    /** The answer while the game opens or its files are prepared: nothing to save yet. */
    stop: "Durdur ve oyna",
    /** While it closes: what comes after. */
    closingAccount: "Kapanınca {{name}} ile açılacak.",
    closingInstance: "Kapanınca {{instance}} açılacak.",
    closingGame: "Kapanınca oyun yeniden açılacak.",
  },
};
