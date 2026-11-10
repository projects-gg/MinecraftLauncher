// The launcher update flow (download, install, restart) without dead ends. Short, titles in Title Case.
export const trUpdates = {
  // Title bar pill and Settings → Güncellemeler while background downloads are off.
  available: "Güncelleme Var",
  download: "Güncellemeyi İndir",
  downloading: "İndiriliyor %{{percent}}",
  // A game is open or starting: installing would close it, so the update waits. A state, not an
  // action (the pill and the button take no press meanwhile).
  afterGame: "Oyun kapanınca güncellenecek",
  // The question before installing over open AFK accounts (a small popover at the button).
  afkComeBack_one: "AFK hesabın güncellemeden sonra yeniden bağlanır.",
  afkComeBack_other: "AFK hesapların güncellemeden sonra yeniden bağlanır.",
  later: "Sonra",
  // The toast once the game closed with an update waiting.
  readyAfterGame: "Güncelleme hazır",
  // The one name of installing it, wherever it is offered: the toast, the question about open
  // AFK accounts, Settings (the title bar's pill says "Güncelleme hazır, yeniden başlat").
  restart: "Yeniden Başlat",
};
