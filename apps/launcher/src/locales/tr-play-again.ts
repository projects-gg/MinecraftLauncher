// One more game (PLAY-AGAIN; owner, 26 Oct 2026): "Oyunda" can be pressed, and after the owner's
// question one more game opens with the same account; other accounts' games start beside the ones
// on screen. Before any extra game the free memory is asked (lead, 27 Oct 2026: our players' PCs
// are small). Short, "sen"; names never get a suffix.
export const trPlayAgain = {
  /** The owner's question: a game with the selected account's name is on screen. */
  title: "Bu adla oyun zaten açık",
  body: "Yine de bir tane daha açılsın mı?",
  confirm: "Bir tane daha aç",
  /** Before any extra game, when the computer's free memory would not hold it. */
  memory: "Bilgisayarında bir oyun daha açmaya yetecek boş bellek yok.",
  /** Only other accounts in game and the memory short: the question names the one more game. */
  memoryTitle: "Bir oyun daha açılsın mı?",
  memoryConfirm: "Yine de aç",
  /** OYNA's name for screen readers while it says "Oyunda": what a press does. */
  runningAria: "Oyunda, bir oyun daha aç",
  /** The command bar's quiet line while other accounts play and the selected one does not. */
  othersOne: "{{name}} oyunda",
  othersTwo: "{{first}} ve {{second}} oyunda",
  othersMany: "{{total}} hesap oyunda",
  /** A game crashed while another game is on screen: the notification that brings the help. */
  crashBody: "Ne olduğunu görmek için tıkla.",
};
