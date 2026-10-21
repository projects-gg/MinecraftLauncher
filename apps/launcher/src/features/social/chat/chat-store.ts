// What the open conversations show: their messages (pages read from this computer's history and
// the changes that come live), drafts that wait in the composer, and edits shown before the
// service answers. Only conversations the player opened are kept here; the list's summaries come
// with the snapshot.
import { create } from "zustand";
import { api } from "@/lib/api";
import type { ChatChange, ChatMessage, ChatPage } from "@/lib/api/social-types";
import { sortMessages } from "./conversations";

export interface ChatThread {
  messages: ChatMessage[];
  /** Older messages remain on this computer. */
  more: boolean;
  /** The first page arrived. */
  loaded: boolean;
  /** A page is on its way; a change that came meanwhile asks for it again. */
  loading: boolean;
  stale: boolean;
  /** An older page is on its way. */
  older: boolean;
}

interface ChatStore {
  threads: Record<string, ChatThread>;
  /** What the player typed and did not send, per conversation (this session). */
  drafts: Record<string, string>;
  /** Edits on their way: the new text shows until the service answers (key → text). */
  edits: Record<string, string>;
  /** Messages deleted here, hidden until the service answers. */
  hidden: Record<string, true>;
  load: (conv: string) => Promise<void>;
  loadOlder: (conv: string) => Promise<void>;
  apply: (change: ChatChange) => void;
  setDraft: (conv: string, text: string) => void;
  setEdit: (key: string, text: string | null) => void;
  setHidden: (key: string, hidden: boolean) => void;
  /** Signed out or another account: nothing of the old one stays. */
  forget: () => void;
}

const EMPTY: ChatThread = {
  messages: [],
  more: false,
  loaded: false,
  loading: false,
  stale: false,
  older: false,
};

/** The page's messages with those already shown (kept ones win: they came later). */
function mergeInto(page: readonly ChatMessage[], kept: readonly ChatMessage[]): ChatMessage[] {
  const keys = new Set(kept.map((m) => m.key));
  return sortMessages([...page.filter((m) => !keys.has(m.key)), ...kept]);
}

export const useChat = create<ChatStore>()((set, get) => ({
  threads: {},
  drafts: {},
  edits: {},
  hidden: {},
  load: async (conv) => {
    const current = get().threads[conv];
    if (current?.loading) return;
    set((s) => ({
      threads: {
        ...s.threads,
        [conv]: { ...(s.threads[conv] ?? EMPTY), loading: true, stale: false },
      },
    }));
    let page: ChatPage;
    try {
      page = await api.social.chatPage(conv);
    } catch {
      set((s) => ({
        threads: { ...s.threads, [conv]: { ...(s.threads[conv] ?? EMPTY), loading: false } },
      }));
      return;
    }
    const again = get().threads[conv]?.stale ?? false;
    set((s) => {
      const thread = s.threads[conv] ?? EMPTY;
      // Older pages already shown stay; the newest page replaces the newest part.
      const oldest = page.messages.find((m) => m.id !== undefined)?.id;
      const older =
        thread.loaded && oldest !== undefined
          ? thread.messages.filter((m) => m.id !== undefined && m.id < oldest)
          : [];
      return {
        threads: {
          ...s.threads,
          [conv]: {
            messages: sortMessages([...older, ...page.messages]),
            more: older.length ? thread.more : page.more,
            loaded: true,
            loading: false,
            stale: false,
            older: false,
          },
        },
      };
    });
    // A change came while the page was on its way: the page may miss it.
    if (again) await get().load(conv);
  },
  loadOlder: async (conv) => {
    const thread = get().threads[conv];
    if (!thread?.loaded || !thread.more || thread.older) return;
    const first = thread.messages.find((m) => m.id !== undefined)?.id;
    if (first === undefined) return;
    set((s) => ({ threads: { ...s.threads, [conv]: { ...thread, older: true } } }));
    try {
      const page = await api.social.chatPage(conv, first);
      set((s) => {
        const now = s.threads[conv] ?? EMPTY;
        return {
          threads: {
            ...s.threads,
            [conv]: {
              ...now,
              messages: mergeInto(page.messages, now.messages),
              more: page.more,
              older: false,
            },
          },
        };
      });
    } catch {
      set((s) => ({
        threads: { ...s.threads, [conv]: { ...(s.threads[conv] ?? EMPTY), older: false } },
      }));
    }
  },
  apply: (change) =>
    set((s) => {
      const thread = s.threads[change.conv];
      // Never opened here: the list's summary says what it needs to.
      if (!thread) return s;
      if (thread.loading) {
        return { threads: { ...s.threads, [change.conv]: { ...thread, stale: true } } };
      }
      const removed = new Set(change.remove);
      const messages = change.reset ? [] : thread.messages.filter((m) => !removed.has(m.key));
      for (const message of change.upsert) {
        const at = messages.findIndex((m) => m.key === message.key);
        if (at >= 0) messages[at] = message;
        else messages.push(message);
      }
      // An edit or a delete the service answered needs no stand-in any more (the service may have
      // cleaned the edited text: its version wins).
      const edits = { ...s.edits };
      const hidden = { ...s.hidden };
      for (const message of change.upsert) {
        if (message.edited || edits[message.key] === message.text) delete edits[message.key];
      }
      for (const key of change.remove) delete hidden[key];
      return {
        threads: {
          ...s.threads,
          [change.conv]: {
            ...thread,
            messages: sortMessages(messages),
            more: change.reset ? false : thread.more,
          },
        },
        edits,
        hidden,
      };
    }),
  setDraft: (conv, text) =>
    set((s) => {
      if ((s.drafts[conv] ?? "") === text) return s;
      const drafts = { ...s.drafts };
      if (text) drafts[conv] = text;
      else delete drafts[conv];
      return { drafts };
    }),
  setEdit: (key, text) =>
    set((s) => {
      const edits = { ...s.edits };
      if (text === null) delete edits[key];
      else edits[key] = text;
      return { edits };
    }),
  setHidden: (key, hidden) =>
    set((s) => {
      const next = { ...s.hidden };
      if (hidden) next[key] = true;
      else delete next[key];
      return { hidden: next };
    }),
  forget: () => set({ threads: {}, drafts: {}, edits: {}, hidden: {} }),
}));
