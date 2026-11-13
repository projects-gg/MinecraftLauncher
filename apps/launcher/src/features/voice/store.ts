// What voice chat's interface keeps (VOICE-L3): the backend's state (`voiceApi`), who speaks now,
// the room panel and its page, and what the title bar's pill says for a moment after a stay ended
// on its own ("Cevap yok" with "Tekrar ara", "Bağlanamadı" with "Tekrar dene").
//
// SQOL-U3: the room panel docked beside the stream's stage takes `dock` px of the window's right
// (the stage leaves them free), the chat's field takes the focus when the panel is opened for it
// (`chatFocus`), and the room's people fold away under their title (`peopleOpen`).
import { useMemo } from "react";
import { create } from "zustand";
import { useSocialUi } from "@/features/social/store";
import { HIDDEN_VOICE_STATE, type VoiceSpeaking, type VoiceState } from "@/lib/api/voice-types";
import i18n from "@/lib/i18n";
import { isTauri } from "@/lib/platform";
import { param } from "@/share/param";
import { toldMissed } from "./quiet";
import { faceOf, type VoiceFace } from "./rules";

/** The panel's pages: the room, and "Davet et" sliding in over it. */
export type VoicePage = "room" | "invite";

/** What the pill keeps after a stay that ended on its own. */
export type VoiceEnded = { kind: "noAnswer"; friend: string } | { kind: "failed" };

interface VoiceUi {
  state: VoiceState;
  setState: (state: VoiceState) => void;
  /** Others who speak now (sorted), and the player; empty while the window is away. */
  speaking: readonly string[];
  meTalking: boolean;
  setSpeaking: (speaking: VoiceSpeaking) => void;
  panel: boolean;
  page: VoicePage;
  /** The last page change: 1 deeper, -1 back. */
  direction: 1 | -1;
  /** Focus goes back here when the panel closes. */
  returnFocus: HTMLElement | null;
  /**
   * `focus: "chat"`: the chat's field takes the focus once it shows (the stage's "Sohbet", a
   * preview's press); the panel already open takes it there too.
   */
  openPanel: (options?: { from?: HTMLElement | null; page?: VoicePage; focus?: "chat" }) => void;
  closePanel: () => void;
  goTo: (page: VoicePage) => void;
  /** Grows when the panel's chat field should take the focus. */
  chatFocus: number;
  /** The last `chatFocus` a field took (a field that comes later does not take an old one). */
  chatFocusTaken: number;
  /** The chat's field on screen takes the focus asked for it (true once per ask). */
  takeChatFocus: () => boolean;
  /** Asks the chat's field for the focus (the panel changed its place while it had it). */
  cueChatFocus: () => void;
  /**
   * SQOL-U3: the px the room panel takes at the window's right while it is docked beside the
   * stage (0 while it is closed, hidden or over the page as a sheet). The stage reads it.
   */
  dock: number;
  setDock: (dock: number) => void;
  /** The room's people show under their title with a chat below (folded: the title alone). */
  peopleOpen: boolean;
  setPeopleOpen: (open: boolean) => void;
  ended: VoiceEnded | null;
  setEnded: (ended: VoiceEnded | null) => void;
  /** Who shared a room with the player lately (this run): person → the room and when they parted.
   * "Rapor et" from their card carries that room for a day (voice.md §8.3). */
  recent: Record<string, { room: string; at: number }>;
  rememberRoom: (room: string, people: readonly string[], at: number) => void;
  /** Missed calls (their rooms) the player has seen in the friends list: they no longer light the
   * Friends entry's mark; the friend's row still says "Cevapsız arama" until it is answered. */
  seenMissed: readonly string[];
  /** The friends list is in front of the player: every missed call now is seen. */
  markMissedSeen: () => void;
}

const same = (a: readonly string[], b: readonly string[]) =>
  a.length === b.length && a.every((id, i) => id === b[i]);

export const useVoiceUi = create<VoiceUi>()((set, get) => ({
  state: HIDDEN_VOICE_STATE,
  setState: (state) =>
    set((s) => {
      // A stay that begins clears what the pill kept of the last one; the panel closes with the
      // room (the toast says why) and opens on the room the next time.
      const begins = (state.room !== null || state.calling !== null) && s.ended !== null;
      const gone = state.room === null && s.state.room !== null;
      return {
        state,
        ...(begins ? { ended: null } : {}),
        ...(gone ? { panel: false, page: "room" as const, speaking: [], meTalking: false } : {}),
      };
    }),
  speaking: [],
  meTalking: false,
  setSpeaking: ({ ids, me }) => {
    const sorted = [...ids].sort();
    const s = get();
    // Ten frames a second at most, most of them the same: nothing renders again for those.
    if (same(sorted, s.speaking) && me === s.meTalking) return;
    set({ speaking: sorted, meTalking: me });
  },
  // The browser preview may start with it open (`roomPanel=1`).
  panel: !isTauri() && param("roomPanel") === "1",
  page: "room",
  direction: 1,
  returnFocus: null,
  // After the menu or the button that asked has let go of the focus. Opened again while open, it
  // keeps its page and the place focus goes back to.
  openPanel: (options) =>
    window.setTimeout(
      () =>
        set((s) => ({
          panel: true,
          page: options?.page ?? (s.panel && !options?.focus ? s.page : "room"),
          direction: 1,
          returnFocus: s.panel && !options?.from ? s.returnFocus : (options?.from ?? null),
          chatFocus: options?.focus === "chat" ? s.chatFocus + 1 : s.chatFocus,
        })),
      0,
    ),
  // An ask for the chat's field nobody took goes with the panel.
  closePanel: () => set((s) => ({ panel: false, chatFocusTaken: s.chatFocus })),
  goTo: (page) => set({ page, direction: page === "room" ? -1 : 1 }),
  chatFocus: 0,
  chatFocusTaken: 0,
  takeChatFocus: () => {
    const { chatFocus, chatFocusTaken } = get();
    if (chatFocus <= chatFocusTaken) return false;
    set({ chatFocusTaken: chatFocus });
    return true;
  },
  cueChatFocus: () => set((s) => ({ chatFocus: s.chatFocus + 1 })),
  dock: 0,
  setDock: (dock) => {
    if (get().dock !== dock) set({ dock });
  },
  peopleOpen: true,
  setPeopleOpen: (peopleOpen) => set({ peopleOpen }),
  ended: null,
  setEnded: (ended) => set({ ended }),
  recent: {},
  rememberRoom: (room, people, at) =>
    set((s) => {
      // Only the day's: older entries go as new ones come.
      const kept = Object.fromEntries(
        Object.entries(s.recent).filter(([, seen]) => at - seen.at < RECENT_MS),
      );
      for (const id of people) kept[id] = { room, at };
      return { recent: kept };
    }),
  seenMissed: [],
  markMissedSeen: () => {
    const s = get();
    const rooms = s.state.missed.map((m) => m.room);
    // Only the calls still listed: answered or dismissed ones leave the list too.
    if (same(rooms, s.seenMissed)) return;
    set({ seenMissed: rooms });
  },
}));

/** Missed calls the player has not seen yet: they light the Friends entry's mark with the other
 * things that wait (a request, an unread message). */
export function unseenMissed(s: Pick<VoiceUi, "state" | "seenMissed">): number {
  if (!voiceOn(s.state)) return 0;
  // An ignored person's call waits in the list without lighting anything (SOC-NEXT §3.7).
  return toldMissed(s.state.missed).filter((m) => !s.seenMissed.includes(m.room)).length;
}

export function useMissedWaiting(): number {
  return useVoiceUi(unseenMissed);
}

/** How long after a call "Rapor et" on someone's card still carries the room (a day). */
export const RECENT_MS = 24 * 60 * 60 * 1000;

/** The room someone shared with the player within the day, for a report from their card. */
export function recentRoom(person: string, now = Date.now()): string | null {
  const seen = useVoiceUi.getState().recent[person];
  return seen && now - seen.at < RECENT_MS ? seen.room : null;
}

/** Whether someone speaks now (the player's own id: the player's microphone sends). */
export function useSpeaking(id: string): boolean {
  return useVoiceUi((s) => (id === s.state.room?.me.id ? s.meTalking : s.speaking.includes(id)));
}

/** Faces for ids, from the friend list, the crews and the voice state's profiles. */
export function useFaces(ids: readonly string[]): VoiceFace[] {
  const friends = useSocialUi((s) => s.view.friends);
  const crews = useSocialUi((s) => s.view.crews);
  const profiles = useVoiceUi((s) => s.state.profiles);
  // The ids as one key: a new array of the same ids works nothing out again.
  const key = ids.join(",");
  return useMemo(
    () =>
      key
        .split(",")
        .filter(Boolean)
        .map((id) => faceOf(id, { friends, crews }, { profiles }, i18n.t("voice.someone"))),
    [key, friends, crews, profiles],
  );
}

/** One face. */
export function useFace(id: string): VoiceFace {
  const [face] = useFaces([id]);
  return face ?? { id, name: i18n.t("voice.someone"), friend: false };
}

/** A face read outside React (toasts, announcements). */
export function faceNow(id: string): VoiceFace {
  const { friends, crews } = useSocialUi.getState().view;
  return faceOf(id, { friends, crews }, useVoiceUi.getState().state, i18n.t("voice.someone"));
}

/** Voice chat shows at all: offered, and the player's switch is on. */
export function voiceOn(state: Pick<VoiceState, "available" | "enabled">): boolean {
  return state.available && state.enabled;
}
