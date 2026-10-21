// How a new chat message is told (CHAT1), pure rules (notify.test.ts): a direct message gets a toast
// (a system notification while the launcher is not in front); crew chat only counts unread unless
// the player turned its notifications on. Nothing while the game runs, on do-not-disturb, for a
// conversation muted here or a crew muted with "Sessize al", or for one already on screen.
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
}

export type ChatNoticeWay = "none" | "toast" | "system";

export function chatNoticeWay(ctx: ChatNoticeContext): ChatNoticeWay {
  if (ctx.status === "dnd" || ctx.gameBusy || ctx.mutedHere) return "none";
  if (ctx.kind === "crew" && (!ctx.crewChatOn || ctx.crewMuted)) return "none";
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
