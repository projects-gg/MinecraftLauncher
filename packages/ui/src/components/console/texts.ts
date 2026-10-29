// The console looks' words (CONSOLE-COLOR), in both languages the launcher and the site speak.
import type { InventoryLang } from "../inventory/texts";

export interface ConsoleTexts {
  /** The game's "Type: …" line of an entity's hover. */
  entityType: string;
  runTitle: string;
  run: string;
  openTitle: string;
  open: string;
  cancel: string;
  fromServer: string;
  /** `{name}`: the player whose line it is. */
  fromPlayer: string;
  copied: string;
  /** A copy's toast with what was copied (`{value}`: `copiedPreview`), so nothing is copied unseen. */
  copiedValue: string;
  /** A private line's direction mark: read out for one to you, shown for your own. */
  pmIn: string;
  pmOut: string;
  pmOutSaid: string;
  /** A server's dialog (DIALOG-CARD): where a command from it came from, its close, its label. */
  dialogSource: string;
  dialogClose: string;
  dialogPlayOnly: string;
  dialogLabel: string;
  /** A press the machine or the client refused: the dialog had changed, or any other reason. */
  dialogChanged: string;
  dialogFailed: string;
}

export const CONSOLE_TEXTS: Record<InventoryLang, ConsoleTexts> = {
  tr: {
    entityType: "Tür",
    runTitle: "Bu komut çalışsın mı?",
    run: "Çalıştır",
    openTitle: "Bu bağlantı açılsın mı?",
    open: "Aç",
    cancel: "Vazgeç",
    fromServer: "Sunucu mesajından",
    fromPlayer: "{name} adlı oyuncunun mesajından",
    copied: "Kopyalandı",
    copiedValue: "Kopyalandı: {value}",
    pmIn: "Sana gelen özel mesaj",
    pmOut: "Sen",
    pmOutSaid: "Senin özel mesajın",
    dialogSource: "Sunucu penceresinden",
    dialogClose: "Oyunda kapat",
    dialogPlayOnly: "Oyuna girince",
    dialogLabel: "Sunucu penceresi",
    dialogChanged: "Sunucu penceresi değişti.",
    dialogFailed: "Bu düğme şu an çalışmadı.",
  },
  en: {
    entityType: "Type",
    runTitle: "Run this command?",
    run: "Run",
    openTitle: "Open this link?",
    open: "Open",
    cancel: "Cancel",
    fromServer: "From the server's message",
    fromPlayer: "From {name}'s message",
    copied: "Copied",
    copiedValue: "Copied: {value}",
    pmIn: "A private message to you",
    pmOut: "You",
    pmOutSaid: "Your private message",
    dialogSource: "From the server's window",
    dialogClose: "Close in game",
    dialogPlayOnly: "Once in game",
    dialogLabel: "Server window",
    dialogChanged: "The server's window changed.",
    dialogFailed: "This button didn't work just now.",
  },
};
