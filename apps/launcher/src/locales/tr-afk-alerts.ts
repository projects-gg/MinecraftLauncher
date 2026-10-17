// AFK notifications and the tray summary of running AFK accounts. Short, sentence case. (The tray
// tooltip itself is written by the backend, src-tauri/src/shell.rs.)
export const trAfkAlerts = {
  // An account stopped for good: banned, not on the whitelist, wrong password, ...
  stopped: "{{name}} durdu",
  stoppedMany: "{{count}} AFK hesabı durdu",
  // Several stopped for the same reason.
  sameReason: "{{reason}}: {{names}}",
  show: "Göster",
  // The mandatory update screen while AFK accounts or the game are open.
  update: {
    running: "Açık AFK hesapların",
    more: "+{{count}}",
    stopAndUpdate: "Durdur ve güncelle",
    stopping: "AFK hesapları durduruluyor",
    installing: "Güncelleme kuruluyor",
    quitGame: "Güncellemek için oyundan çık",
  },
};
