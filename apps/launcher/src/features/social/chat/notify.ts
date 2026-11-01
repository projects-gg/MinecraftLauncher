// How a new chat message is told (CHAT1), pure rules (notify.test.ts): a direct message gets a toast
// (a system notification while the launcher is not in front); crew chat only counts unread unless
// the player turned its notifications on. Nothing while the game runs, on do-not-disturb, for a
// conversation muted here or a crew muted with "Sessize al", or for one already on screen.
//
// CHAT-CALM: a reply to one of the player's own messages is heard in crew chat too, as a mention
// would be (a crowded crew starts muted; there are no mentions). And a conversation's messages are
// gathered (`ChatNotices`): the first waits a moment for those that follow, then one notice says
// them all ("Kaan: 5 yeni mesaj"); a conversation's system notification pops up at most every two
// minutes and changes its words in place between, so a burst never piles notifications up.
import type { StatusMode } from "@/lib/api/social-types";

export interface ChatNoticeContext {
  kind: "direct" | "crew";
  status: StatusMode | undefined;
  /** A game is being prepared or played. */
  gameBusy: boolean;
  /** The launcher's window has the focus. */
  inFront: boolean;
  /** The conversation is open and shows its newest messages. */
  onScreen: boolean;
  /** "Sessize al" on this conversation, on this computer. */
  mutedHere: boolean;
  /** The crew is muted ("Sessize al", on its own or all crews). */
  crewMuted: boolean;
  /** "Ekip sohbeti bildirimleri" (off by default). */
  crewChatOn: boolean;
  /** CHAT-CALM: it answers one of the player's own messages. */
  toMe: boolean;
}

export type ChatNoticeWay = "none" | "toast" | "system";

export function chatNoticeWay(ctx: ChatNoticeContext): ChatNoticeWay {
  if (ctx.status === "dnd" || ctx.gameBusy || ctx.mutedHere) return "none";
  if (ctx.kind === "crew" && !ctx.toMe && (!ctx.crewChatOn || ctx.crewMuted)) return "none";
  if (ctx.inFront) return ctx.onScreen ? "none" : "toast";
  return "system";
}

/** A message as a notification shows it: at most one line of about 90 characters. */
export function preview(text: string, max = 90): string {
  const line = text.replace(/\s+/g, " ").trim();
  const chars = [...line];
  return chars.length > max
    ? `${chars
        .slice(0, max - 1)
        .join("")
        .trimEnd()}…`
    : line;
}

// --- gathering (CHAT-CALM) -----------------------------------------------------------------------

/** The first message of a conversation waits this long for those that follow. */
export const CHAT_GATHER_MS = 4_000;
/** A conversation's system notification pops up at most this often; between, it changes in place. */
export const CHAT_REPOP_MS = 120_000;

/** One message worth telling, as a notice shows it. */
export interface ChatNoticeItem {
  /** Its sender, as the lists name them. */
  name: string;
  /** One line of it (`preview`). */
  text: string;
  /** The crew's name, in a crew's chat. */
  crew?: string;
}

/** What one conversation's notice says now. */
export interface GatheredChatNotice {
  conv: string;
  /** Messages worth telling since the player last saw the conversation. */
  count: number;
  /** The newest of them. */
  last: ChatNoticeItem;
  /** A system notification pops up; else the one shown changes its words in place. */
  popup: boolean;
}

/** Where the gathered notices go (social-host.tsx; tests use their own). */
export interface ChatNoticeSink {
  /** How the conversation's notice would go now: the window may have come forward meanwhile. */
  way(conv: string): ChatNoticeWay;
  /** Shows the conversation's notice, or changes the one shown. */
  show(notice: GatheredChatNotice, way: "toast" | "system"): void;
  /** The player saw the conversation: its notices go away. */
  clear(conv: string): void;
}

interface Gathering {
  count: number;
  last: ChatNoticeItem;
  timer: ReturnType<typeof setTimeout> | null;
  /** When its system notification last popped up (ms), or null. */
  poppedAt: number | null;
}

/** One notice per conversation for any number of messages (see the file's head). */
export class ChatNotices {
  private readonly convs = new Map<string, Gathering>();

  constructor(
    private readonly sink: ChatNoticeSink,
    private readonly now: () => number = Date.now,
  ) {}

  /** A message worth telling (its way was not "none" when it came). */
  add(conv: string, item: ChatNoticeItem): void {
    const gathering = this.convs.get(conv) ?? { count: 0, last: item, timer: null, poppedAt: null };
    gathering.count += 1;
    gathering.last = item;
    this.convs.set(conv, gathering);
    if (gathering.timer === null) {
      gathering.timer = setTimeout(() => this.flush(conv), CHAT_GATHER_MS);
    }
  }

  private flush(conv: string): void {
    const gathering = this.convs.get(conv);
    if (!gathering) return;
    gathering.timer = null;
    const way = this.sink.way(conv);
    // On screen now, a game started, do-not-disturb: nothing to say; the count stays until the
    // player sees the conversation.
    if (way === "none") return;
    const now = this.now();
    const popup =
      way === "system" &&
      (gathering.poppedAt === null || now - gathering.poppedAt >= CHAT_REPOP_MS);
    if (popup) gathering.poppedAt = now;
    this.sink.show({ conv, count: gathering.count, last: gathering.last, popup }, way);
  }

  /** The player saw the conversation (on screen, or read on another device): its notices go. */
  seen(conv: string): void {
    const gathering = this.convs.get(conv);
    if (!gathering) return;
    if (gathering.timer !== null) clearTimeout(gathering.timer);
    this.convs.delete(conv);
    this.sink.clear(conv);
  }

  /** Conversations with a notice that waits or shows. */
  open(): string[] {
    return [...this.convs.keys()];
  }

  /** Signed out, or another account: nothing of the old one is said. */
  forget(): void {
    for (const conv of [...this.convs.keys()]) this.seen(conv);
  }
}
