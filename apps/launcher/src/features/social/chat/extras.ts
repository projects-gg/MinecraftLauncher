// CHAT2's extras as chat may offer them now (chat2.md §2): the backend took the capability from
// the service and Studio has it on. An older service has none: chat is then exactly CHAT1.
import type { ChatExtras, ChatLinkRules } from "@/lib/api/social-types";
import { useSocialUi } from "../store";

export const NO_EXTRAS: ChatExtras = {
  replies: false,
  stickers: false,
  reactions: false,
  typing: false,
  reads: false,
  links: false,
};

/** What chat offers beyond CHAT1 now (all off while chat is off). */
export function useChatExtras(): ChatExtras {
  return useSocialUi((s) => (s.view.chat?.on ? s.view.chat.extras : undefined) ?? NO_EXTRAS);
}

/** The service speaks CHAT2 at all (it sends `extras`, maybe all off). */
export function useChatHasExtras(): boolean {
  return useSocialUi((s) => Boolean(s.view.chat?.extras));
}

/** The link rules while links are on. */
export function useLinkRules(): ChatLinkRules | undefined {
  return useSocialUi((s) =>
    s.view.chat?.on && s.view.chat.extras?.links ? s.view.chat.links : undefined,
  );
}

/** The sticker sets the service takes ("pl1"); none without the extra. */
export function useStickerSets(): readonly string[] {
  return useSocialUi((s) =>
    s.view.chat?.on && s.view.chat.extras?.stickers ? (s.view.chat.stickerSets ?? EMPTY) : EMPTY,
  );
}

const EMPTY: readonly string[] = [];
