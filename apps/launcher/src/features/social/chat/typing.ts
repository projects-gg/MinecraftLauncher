// "Yazıyor…" (CHAT2 §4): who writes where, for 6 seconds after their last frame; their message
// arriving ends it at once, as does their going offline (the line leaves out who is offline).
// One timer, for the earliest end, only while someone writes: nothing runs otherwise. The player's
// own writing goes out at most every 4 seconds, only while the box has text and the focus, not
// while editing and not while the window is away (the backend also drops it when nobody else in
// the conversation is online).
import { useEffect } from "react";
import { create } from "zustand";
import { api } from "@/lib/api";
import { isWindowAway } from "@/lib/window-away";

/** A writer shows this long after their last frame. */
export const TYPING_SHOW_MS = 6_000;
/** The player's own frames go at most this often. */
export const TYPING_SEND_MS = 4_000;

interface TypingStore {
  /** conv → writer → until (Unix ms). */
  writers: Record<string, Record<string, number>>;
  saw: (conv: string, by: string, nowMs?: number) => void;
  stop: (conv: string, by: string) => void;
  forget: () => void;
}

let timer: ReturnType<typeof setTimeout> | null = null;

/** Drops the writers whose time is up and waits for the next one (or for nothing). */
function settle() {
  if (timer !== null) clearTimeout(timer);
  timer = null;
  const now = Date.now();
  const { writers } = useTyping.getState();
  let next = Number.POSITIVE_INFINITY;
  let changed = false;
  const kept: Record<string, Record<string, number>> = {};
  for (const [conv, people] of Object.entries(writers)) {
    const alive = Object.entries(people).filter(([, until]) => until > now);
    if (alive.length !== Object.keys(people).length) changed = true;
    if (alive.length) kept[conv] = Object.fromEntries(alive);
    for (const [, until] of alive) next = Math.min(next, until);
  }
  if (changed) useTyping.setState({ writers: kept });
  if (next !== Number.POSITIVE_INFINITY) timer = setTimeout(settle, next - now + 20);
}

export const useTyping = create<TypingStore>()((set) => ({
  writers: {},
  saw: (conv, by, nowMs = Date.now()) => {
    set((s) => ({
      writers: { ...s.writers, [conv]: { ...s.writers[conv], [by]: nowMs + TYPING_SHOW_MS } },
    }));
    settle();
  },
  stop: (conv, by) =>
    set((s) => {
      if (!s.writers[conv]?.[by]) return s;
      const people = { ...s.writers[conv] };
      delete people[by];
      const writers = { ...s.writers };
      if (Object.keys(people).length) writers[conv] = people;
      else delete writers[conv];
      return { writers };
    }),
  forget: () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
    set({ writers: {} });
  },
}));

let feeds = 0;
let unsubscribe: (() => void) | null = null;

/** Listens to "yazıyor" frames while some part of chat is on screen (counted: one listener). */
export function useTypingFeed(): void {
  useEffect(() => {
    feeds += 1;
    if (feeds === 1) {
      unsubscribe = api.social.onChatTyping(({ conv, by }) => useTyping.getState().saw(conv, by));
    }
    return () => {
      feeds -= 1;
      if (feeds === 0) {
        unsubscribe?.();
        unsubscribe = null;
      }
    };
  }, []);
}

/** The writers of a conversation now (in the order they started), the offline left out. */
/**
 * Whether someone is online now, by the friend list and the crews (a writer who went offline
 * stops "yazıyor" at once).
 */
export function onlineIn(
  friends: readonly { id: string; state: string }[],
  crews: readonly { members: readonly { id: string; me?: boolean; state: string }[] }[],
): (id: string) => boolean {
  return (id) =>
    friends.some((f) => f.id === id && f.state !== "offline") ||
    crews.some((c) => c.members.some((m) => m.id === id && !m.me && m.state !== "offline"));
}

export function writersOf(
  writers: Record<string, Record<string, number>>,
  conv: string,
  online: (id: string) => boolean,
): string[] {
  return Object.keys(writers[conv] ?? {}).filter(online);
}

/**
 * The line's words (without the dots, which the line draws): "Ayşe yazıyor", "Ayşe ve Mert
 * yazıyor", "Ayşe, Mert ve 2 kişi daha yazıyor". Names never take a suffix.
 */
export function typingWords(
  names: readonly string[],
  t: (key: string, options?: Record<string, unknown>) => string,
): string | null {
  const [first, second] = names;
  if (first === undefined) return null;
  if (second === undefined) return t("chat2.typing.one", { name: first });
  if (names.length === 2) return t("chat2.typing.two", { first, second });
  return t("chat2.typing.more", { first, second, count: names.length - 2 });
}

/** The player's own frames: `wrote()` on every change of the box, sent at most every 4 s. */
export function typingSender(conv: string) {
  let last = 0;
  return {
    wrote(state: { text: string; focused: boolean; editing: boolean; on: boolean }) {
      if (!state.on || !state.text.trim() || !state.focused || state.editing) return;
      if (isWindowAway()) return;
      const now = Date.now();
      if (now - last < TYPING_SEND_MS) return;
      last = now;
      api.social.chatTyping(conv);
    },
    /** A message went: the next key starts a new frame at once. */
    sent() {
      last = 0;
    },
  };
}
