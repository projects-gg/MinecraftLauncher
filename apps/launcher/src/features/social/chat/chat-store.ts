// What the open conversations show: their messages (pages read from this computer's history and
// the changes that come live), drafts that wait in the composer, and edits and deletions shown
// before the service answers. A message deleted for everyone comes back as its tombstone, under
// the same key (FIX2): its text leaves this store with it. Only conversations the player opened
// are kept here; the list's summaries come with the snapshot.
//
// CHAT2: the message a conversation's next one answers ("Yanıtlanıyor"), and the player's own
// reactions shown at once until the service's answer brings the message back. A message from
// someone ends their "yazıyor…".
import { create } from "zustand";
import { api } from "@/lib/api";
import type { ChatChange, ChatMessage, ChatPage, ChatReaction } from "@/lib/api/social-types";
import { sortMessages } from "./conversations";
import { useTyping } from "./typing";

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
  /** Messages the player deleted here: shown as their tombstone until the service answers. */
  deleting: Record<string, true>;
  /** CHAT2: the message (key) each conversation's next message answers. */
  replying: Record<string, string>;
  /**
   * CHAT2: a message's reactions with the player's changes on their way (key → reactions); it
   * stays until every change on that message is answered, then the message's own show.
   */
  reacting: Record<string, ChatReaction[]>;
  load: (conv: string) => Promise<void>;
  loadOlder: (conv: string) => Promise<void>;
  apply: (change: ChatChange) => void;
  setDraft: (conv: string, text: string) => void;
  setEdit: (key: string, text: string | null) => void;
  setDeleting: (key: string, deleting: boolean) => void;
  setReplying: (conv: string, key: string | null) => void;
  /** The player's reaction change goes: `reactions` show at once. */
  startReacting: (key: string, reactions: ChatReaction[]) => void;
  /** The service answered one change (or refused it): once none waits, the message's own show. */
  endReacting: (key: string) => void;
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

/** The player's reaction changes on their way, per message key (no need to draw them). */
const reactionsOnTheirWay = new Map<string, number>();

/** A message this recent that the thread does not hold is a new one (it ends "yazıyor…"). */
const NEW_MESSAGE_MS = 30_000;

/** The page's messages with those already shown (kept ones win: they came later). */
function mergeInto(page: readonly ChatMessage[], kept: readonly ChatMessage[]): ChatMessage[] {
  const keys = new Set(kept.map((m) => m.key));
  return sortMessages([...page.filter((m) => !keys.has(m.key)), ...kept]);
}

export const useChat = create<ChatStore>()((set, get) => ({
  threads: {},
  drafts: {},
  edits: {},
  deleting: {},
  replying: {},
  reacting: {},
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
  apply: (change) => {
    // Someone's new message ends their "yazıyor…", wherever it shows; a change to one they sent
    // before (a reaction, an edit, a tombstone) does not. A thread not loaded here cannot tell:
    // a message of the last half minute counts as new.
    const held = get().threads[change.conv];
    const nowMs = Date.now();
    for (const message of change.upsert) {
      if (message.mine || message.deleted || message.edited) continue;
      const known = held?.loaded
        ? held.messages.some((m) => m.key === message.key)
        : nowMs - message.at > NEW_MESSAGE_MS;
      if (!known) useTyping.getState().stop(change.conv, message.from);
    }
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
      // cleaned the edited text: its version wins). A tombstone takes any edit on its way along.
      // A message that comes back brings its reactions as the service has them.
      const edits = { ...s.edits };
      const deleting = { ...s.deleting };
      const reacting = { ...s.reacting };
      for (const message of change.upsert) {
        if (!reactionsOnTheirWay.get(message.key) || message.deleted) delete reacting[message.key];
        if (message.deleted) {
          delete edits[message.key];
          delete deleting[message.key];
        } else if (message.edited || edits[message.key] === message.text) {
          delete edits[message.key];
        }
      }
      for (const key of change.remove) {
        delete edits[key];
        delete deleting[key];
        delete reacting[key];
      }
      // A reply to a message that went away (deleted, the history cleared) stops waiting.
      const replying = { ...s.replying };
      const answered = replying[change.conv];
      if (
        answered &&
        (change.reset ||
          change.remove.includes(answered) ||
          change.upsert.some((m) => m.key === answered && m.deleted))
      ) {
        delete replying[change.conv];
      }
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
        deleting,
        reacting,
        replying,
      };
    });
  },
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
  setDeleting: (key, deleting) =>
    set((s) => {
      const next = { ...s.deleting };
      if (deleting) next[key] = true;
      else delete next[key];
      return { deleting: next };
    }),
  setReplying: (conv, key) =>
    set((s) => {
      if ((s.replying[conv] ?? null) === key) return s;
      const replying = { ...s.replying };
      if (key) replying[conv] = key;
      else delete replying[conv];
      return { replying };
    }),
  startReacting: (key, reactions) => {
    reactionsOnTheirWay.set(key, (reactionsOnTheirWay.get(key) ?? 0) + 1);
    set((s) => ({ reacting: { ...s.reacting, [key]: reactions } }));
  },
  endReacting: (key) => {
    const left = (reactionsOnTheirWay.get(key) ?? 1) - 1;
    if (left > 0) {
      reactionsOnTheirWay.set(key, left);
      return;
    }
    reactionsOnTheirWay.delete(key);
    set((s) => {
      if (!(key in s.reacting)) return s;
      const reacting = { ...s.reacting };
      delete reacting[key];
      return { reacting };
    });
  },
  forget: () => {
    reactionsOnTheirWay.clear();
    useTyping.getState().forget();
    set({ threads: {}, drafts: {}, edits: {}, deleting: {}, replying: {}, reacting: {} });
  },
}));
