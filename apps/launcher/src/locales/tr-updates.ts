// The launcher update flow (download, install, restart) without dead ends. Short, sentence case.
export const trUpdates = {
  // Title bar pill and Settings → Güncellemeler while background downloads are off.
  available: "Güncelleme var",
  download: "Güncellemeyi indir",
  downloading: "İndiriliyor %{{percent}}",
  // A game is open or starting: installing would close it, so the update waits.
  afterGame: "Oyundan sonra güncelle",
  // The question before installing over open AFK accounts (a small popover at the button).
  afkComeBack_one: "AFK hesabın güncellemeden sonra yeniden bağlanır.",
  afkComeBack_other: "AFK hesapların güncellemeden sonra yeniden bağlanır.",
  install: "Güncelle",
  later: "Sonra",
  // The toast once the game closed with an update waiting.
  readyAfterGame: "Güncelleme hazır",
  restart: "Yeniden başlat",
};
