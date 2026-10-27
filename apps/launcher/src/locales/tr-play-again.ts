// One more game (PLAY-AGAIN; owner, 26 Oct 2026): "Oyunda" can be pressed, and after the owner's
// question one more game opens with the same account; other accounts' games start beside the ones
// on screen. Before any extra game the free memory is asked (lead, 27 Oct 2026: our players' PCs
// are small). One question, one shape: the title asks, one line says why, "Yine de aç" answers.
// Short, "sen"; names never get a suffix.
export const trPlayAgain = {
  /** The question for one more game (the same name in game, or the memory short). */
  title: "Bir oyun daha açılsın mı?",
  /** The owner's words with the name: the selected account is in game already, and what that means. */
  sameName: "{{name}} zaten oyunda.",
  sameNameUnknown: "Bu adla oyun zaten açık.",
  sameServer: "İki oyun aynı sunucuya girerse biri atılır.",
  /** Before any extra game, when the computer's free memory would not hold it (its own line). */
  memory: "Bilgisayarında boş bellek az, oyunlar takılabilir.",
  confirm: "Yine de aç",
  /** OYNA while it says "Oyunda": under the pointer it says what a press does (a verb: third look). */
  runningHover: "Bir oyun daha aç",
  /** OYNA's name for screen readers while it says "Oyunda": what a press does. */
  runningAria: "Oyunda, bir oyun daha aç",
  /** The command bar's quiet line while other accounts play and the selected one does not. */
  othersOne: "{{name}} oyunda",
  othersTwo: "{{first}} ve {{second}} oyunda",
  othersMany: "{{total}} hesap oyunda",
  /** The same as the bar shows it: the name (or the count) over "Oyunda", as the account menu. */
  othersCount: "{{total}} hesap",
  /** "Devam et" while its world is open in a game: where it is (in its label's place; short, so the
   * world's name keeps its room at 1024 px), and under the pointer whose game has it when that is
   * one account. */
  worldBusy: "Başka oyunda",
  worldBusyName: "{{name}} bu dünyada",
  worldBusyAria: "{{world}}: {{reason}}",
  /** A world's "Oyna" while the world is open in a game (said before any question). */
  worldOpen: "Bu dünya diğer oyunda açık.",
  /** "Bağlan" with several games on screen: which one closes (its account; no suffix on it). */
  restartBodyAccount: "{{name}} hesabının oyunu kapanıp {{server}} sunucusunda yeniden açılır.",
  restartWorldAccount: "{{name}} hesabının oyunu kapanıp arkadaşının dünyasında yeniden açılır.",
  /** Over the server's address in "Bağlan": what to do with it. */
  addressHint: "Ya da oyunda bu adrese bağlan:",
  /** "Bağlan" while a game closes, when several are on screen: whose. */
  closingTitleAccount: "{{name}} hesabının oyunu kapanıyor",
  overdueTitleAccount: "{{name}} hesabının oyunu henüz kapanmadı",
  forcingTitleAccount: "{{name}} hesabının oyunu kapatılıyor",
  /** "Bağlan"'s answer when the game on screen closes first, and the way out while it closes. */
  closeAndConnect: "Kapat ve bağlan",
  stopConnecting: "Bağlanmaktan vazgeç",
  /** "Bağlan" when the game did not close in time: the way out that leaves it as it is. */
  keepOpen: "Oyunu açık bırak",
  /** A game crashed while another game is on screen: the notification that brings the help. */
  crashBody: "Ne olduğunu görmek için tıkla.",
};
