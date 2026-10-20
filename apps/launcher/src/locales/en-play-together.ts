import type { trPlayTogether } from "./tr-play-together";

// "Play together": joining the server an invite came from ("Join" on the toast or the invite row).
export const enPlayTogether: typeof trPlayTogether = {
  confirmTitle: "Open Minecraft?",
  confirmBody: "The game will open and connect to {{server}}.",
  yes: "Yes",
  no: "No",
  openTitle: "Minecraft is open",
  openBody: "Join {{server}} from Multiplayer in the game.",
  copyAddress: "Copy address",
  addressCopied: "Address copied",
  ok: "OK",
  already: "You're already on this server",
};
