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
  },
};
