// SOC-NEXT's choices kept on this computer: friends' nicknames (never in the settings backup:
// children's real names stay off the cloud), the people the player ignores and the conversations
// pinned to the top of "Mesajlar" (both travel with the settings backup, ids only), conversations
// marked unread here, a timed "Rahatsız etmeyin" or "Görünmez", and "Yayıncı modu".
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { cleanNickname } from "./nickname";

/** "Kapalı", "Açık" or "Yayın programı açıkken" (the default). */
export type StreamerMode = "off" | "on" | "auto";

/** A status chosen for a while: the launcher goes back to "Çevrimiçi" at `until` (Unix ms). */
export interface StatusTimer {
  status: "dnd" | "invisible";
  until: number;
}

/** Conversations pinned to the top of "Mesajlar". */
export const MAX_PINNED = 3;
/** People one may ignore (what a backup may carry). */
export const MAX_IGNORED = 200;

interface SocNextPrefs {
  /** Friend id → nickname (this computer only). */
  nicknames: Record<string, string>;
  /** Ignored user ids, oldest first. */
  ignored: string[];
  /** Of them, those whose voice "Sustur (yalnız sen)" came with "Yok say" (it goes with it). */
  ignoreMuted: string[];
  /** Pinned conversation ids, in the order they show. */
  pinnedChats: string[];
  /** Conversation → the time of the message marked unread here (Unix ms). */
  unreadMarks: Record<string, number>;
  statusTimer: StatusTimer | null;
  streamer: StreamerMode;
  /** Sets a nickname (null or an empty one takes it away); false when it is too long. */
  setNickname: (id: string, name: string | null) => boolean;
  setIgnored: (id: string, on: boolean) => void;
  setIgnoreMuted: (id: string, on: boolean) => void;
  /** Pins or unpins a conversation; false when three are pinned already. */
  setPinned: (conv: string, on: boolean) => boolean;
  markUnread: (conv: string, at: number) => void;
  clearUnread: (conv: string) => void;
  clearAllUnread: () => void;
  setStatusTimer: (timer: StatusTimer | null) => void;
  setStreamer: (mode: StreamerMode) => void;
  /** A settings backup's lists (restore on a new computer). */
  restore: (lists: { ignored?: string[]; pinnedChats?: string[] }) => void;
}

const userId = (value: unknown): value is string =>
  typeof value === "string" && /^\d{1,20}$/.test(value);
const conversationId = (value: unknown): value is string =>
  typeof value === "string" && /^(c:[0-9a-z]{1,40}|d:\d{1,20}:\d{1,20})$/i.test(value);

/** The backup's ignored ids: user ids only, at most 200, once each. */
export function cleanIgnored(value: unknown): string[] {
  return Array.isArray(value) ? [...new Set(value.filter(userId))].slice(0, MAX_IGNORED) : [];
}

/** The backup's pinned conversations: conversation ids only, at most three, once each. */
export function cleanPinned(value: unknown): string[] {
  return Array.isArray(value)
    ? [...new Set(value.filter(conversationId))].slice(0, MAX_PINNED)
    : [];
}

export const useSocNextPrefs = create<SocNextPrefs>()(
  persist(
    (set, get) => ({
      nicknames: {},
      ignored: [],
      ignoreMuted: [],
      pinnedChats: [],
      unreadMarks: {},
      statusTimer: null,
      streamer: "auto",
      setNickname: (id, name) => {
        const clean = name === null ? null : cleanNickname(name);
        // Words that do not fit are refused (an empty name takes the nickname away).
        if (name?.trim() && clean === null) return false;
        set((s) => {
          const nicknames = { ...s.nicknames };
          if (clean) nicknames[id] = clean;
          else delete nicknames[id];
          return { nicknames };
        });
        return true;
      },
      setIgnored: (id, on) =>
        set((s) => {
          const has = s.ignored.includes(id);
          if (has === on) return s;
          return {
            ignored: on
              ? [...s.ignored, id].slice(-MAX_IGNORED)
              : s.ignored.filter((one) => one !== id),
          };
        }),
      setIgnoreMuted: (id, on) =>
        set((s) =>
          s.ignoreMuted.includes(id) === on
            ? s
            : {
                ignoreMuted: on
                  ? [...s.ignoreMuted, id].slice(-MAX_IGNORED)
                  : s.ignoreMuted.filter((one) => one !== id),
              },
        ),
      setPinned: (conv, on) => {
        const pinned = get().pinnedChats;
        if (pinned.includes(conv) === on) return true;
        if (on && pinned.length >= MAX_PINNED) return false;
        set({ pinnedChats: on ? [...pinned, conv] : pinned.filter((one) => one !== conv) });
        return true;
      },
      markUnread: (conv, at) => set((s) => ({ unreadMarks: { ...s.unreadMarks, [conv]: at } })),
      clearUnread: (conv) =>
        set((s) => {
          if (!Object.hasOwn(s.unreadMarks, conv)) return s;
          const unreadMarks = { ...s.unreadMarks };
          delete unreadMarks[conv];
          return { unreadMarks };
        }),
      clearAllUnread: () =>
        set((s) => (Object.keys(s.unreadMarks).length ? { unreadMarks: {} } : s)),
      setStatusTimer: (statusTimer) => set({ statusTimer }),
      setStreamer: (streamer) => set({ streamer }),
      restore: ({ ignored, pinnedChats }) =>
        set((s) => ({
          ignored: ignored ? cleanIgnored(ignored) : s.ignored,
          pinnedChats: pinnedChats ? cleanPinned(pinnedChats) : s.pinnedChats,
        })),
    }),
    { name: "pl.socnext", version: 1 },
  ),
);
