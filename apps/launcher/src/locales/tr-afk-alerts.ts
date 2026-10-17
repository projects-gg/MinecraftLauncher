// AFK notifications and the tray summary of running AFK accounts. Short, sentence case. (The tray
// tooltip itself is written by the backend, src-tauri/src/shell.rs.)
export const trAfkAlerts = {
  // An account stopped for good: banned, not on the whitelist, wrong password, ...
  stopped: "{{name}} durdu",
  stoppedMany: "{{count}} AFK hesabı durdu",
  // Several stopped for the same reason.
  sameReason: "{{reason}}: {{names}}",
  show: "Göster",
};
