// One more game (PLAY-AGAIN; owner, 26 Oct 2026): "Oyunda" can be pressed, and after the owner's
// question one more game opens with the same account; other accounts' games start beside the ones
// on screen. Before any extra game the free memory is asked (lead, 27 Oct 2026: our players' PCs
// are small). One question, one shape: the title asks, one line says why, "Yine de aç" answers.
// Short, "sen"; names never get a suffix.
export const trPlayAgain = {
  /** The question for one more game (the same name in game, or the memory short). */
  title: "Bir oyun daha açılsın mı?",
  /** The owner's words with the name: the selected account is in game already. */
  sameName: "{{name}} adıyla oyun zaten açık.",
  sameNameUnknown: "Bu adla oyun zaten açık.",
  /** Before any extra game, when the computer's free memory would not hold it. */
  memory: "Bilgisayarında boş bellek az, oyunlar takılabilir.",
  confirm: "Yine de aç",
  /** OYNA while it says "Oyunda": under the pointer it says what a press does. */
  runningHover: "Bir oyun daha",
  /** OYNA's name for screen readers while it says "Oyunda": what a press does. */
  runningAria: "Oyunda, bir oyun daha aç",
  /** The command bar's quiet line while other accounts play and the selected one does not. */
  othersOne: "{{name}} oyunda",
  othersTwo: "{{first}} ve {{second}} oyunda",
  othersMany: "{{total}} hesap oyunda",
  /** "Devam et" while its world is open in a game: whose game, or that it is another one. */
  worldBusyName: "{{name}} bu dünyada",
  worldBusy: "Başka bir oyunda açık",
  worldBusyShort: "Oyunda açık",
  worldBusyAria: "{{world}}: {{reason}}",
  /** A world's "Oyna" while the world is open in a game (said before any question). */
  worldOpen: "Bu dünya diğer oyunda açık.",
  /** "Bağlan" with several games on screen: which one closes (its account; no suffix on it). */
  restartBodyAccount:
    "{{name}} hesabının oyunu kapanacak ve {{server}} sunucusuna bağlanarak yeniden açılacak.",
  restartWorldAccount:
    "{{name}} hesabının oyunu kapanacak ve arkadaşının dünyasına bağlanarak yeniden açılacak.",
  /** Over the server's address in "Bağlan": what it is for. */
  addressHint: "Oyunun içinden de girebilirsin:",
  /** "Bağlan" when the game did not close in time: the way out that leaves it as it is. */
  keepOpen: "Oyunu açık bırak",
  /** A game crashed while another game is on screen: the notification that brings the help. */
  crashBody: "Ne olduğunu görmek için tıkla.",
};
