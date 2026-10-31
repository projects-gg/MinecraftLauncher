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
  /** Under the buttons while the client is still joining: its command buttons wait for the game. */
  dialogPlayOnly: string;
  dialogLabel: string;
  /** A press the machine or the client refused: the dialog had changed, or any other reason. */
  dialogChanged: string;
  dialogFailed: string;
  /** A menu's opening the client refused: the server took it away (CONSOLE-ANY). */
  dialogMenuGone: string;
  /** The game's "waiting for response" screen after a press (CONSOLE-ANY). */
  dialogWaiting: string;
  dialogBack: string;
  /** The back button while it waits, as the game counts down (`{n}`: seconds). */
  dialogBackIn: string;
  /** Under a window the client had to cut: not all of it shows. */
  dialogCut: string;
  /** A long list's pages (`{page}`, `{pages}`). */
  dialogPage: string;
  dialogPrev: string;
  dialogNext: string;
  /** The row of what the server's pause screen and quick actions open. */
  dialogMenus: string;
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
    dialogPlayOnly: "Komut düğmeleri oyuna girince çalışır.",
    dialogLabel: "Sunucu penceresi",
    dialogChanged: "Sunucu penceresi değişti.",
    dialogFailed: "Bu düğme şu an çalışmadı.",
    dialogMenuGone: "Bu menü şu an yok.",
    dialogWaiting: "Sunucudan cevap bekleniyor",
    dialogBack: "Geri",
    dialogBackIn: "Geri ({n})",
    dialogCut: "Pencerenin bir kısmı burada görünmüyor.",
    dialogPage: "{page} / {pages}",
    dialogPrev: "Önceki sayfa",
    dialogNext: "Sonraki sayfa",
    dialogMenus: "Sunucu menüleri",
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
    dialogPlayOnly: "Command buttons work once you're in the game.",
    dialogLabel: "Server window",
    dialogChanged: "The server's window changed.",
    dialogFailed: "This button didn't work just now.",
    dialogMenuGone: "This menu isn't there now.",
    dialogWaiting: "Waiting for the server's answer",
    dialogBack: "Back",
    dialogBackIn: "Back ({n})",
    dialogCut: "Part of this window doesn't show here.",
    dialogPage: "{page} / {pages}",
    dialogPrev: "Previous page",
    dialogNext: "Next page",
    dialogMenus: "Server menus",
  },
};
