// What a screen reader hears of the room chat (SQOL §1.3 "Erişilebilirlik"): its new messages,
// politely and together. Messages that come close together wait a moment and are told once: one
// message as itself ("KorAtesi: selam"), more as their count ("3 yeni mesaj"). Out of sight the wait
// is longer (the CHAT-CALM moment) and the words say where they are.

/** In sight, messages this close together are told together. */
export const SHOWN_GATHER_MS = 1_200;
/** Out of sight (the previews over the stage), as chat's notices gather (CHAT-CALM). */
export const HIDDEN_GATHER_MS = 4_000;

export interface Heard {
  name: string;
  /** The message in one line (a sticker in words). */
  line: string;
}

type T = (key: string, options?: Record<string, unknown>) => string;

/** The words a gathered burst is told with. */
export function burstWords(heard: readonly Heard[], shown: boolean, t: T): string {
  const [one] = heard;
  if (heard.length === 1 && one) {
    return shown
      ? t("chat.notices.direct", { name: one.name, text: one.line })
      : t("roomChat.announce.one", { name: one.name, text: one.line });
  }
  return shown
    ? t("chatCalm.newMessages", { count: heard.length })
    : t("roomChat.announce.many", { count: heard.length });
}

/**
 * Gathers what comes within `wait` of its first and hands it over once. `clear` drops what waits
 * (the room changed, the chat came on screen).
 */
export function createGather<T>(flush: (items: T[]) => void) {
  let items: T[] = [];
  let timer: ReturnType<typeof setTimeout> | undefined;
  return {
    add(item: T, wait: number) {
      items.push(item);
      if (timer !== undefined) return;
      timer = setTimeout(() => {
        const out = items;
        items = [];
        timer = undefined;
        flush(out);
      }, wait);
    },
    clear() {
      clearTimeout(timer);
      timer = undefined;
      items = [];
    },
  };
}
