// SOC-NOTE's previews on top of mock-social's and mock-chat's data (`&socNote=demo` with a chat
// preview, e.g. `?social=chat2&socNext=demo&socNote=demo`): status notes on (NetherKral's "Sınav
// haftası, akşam bakarım" in place of the line under the name, KorAtesi's on the card while they
// play), the player's own "Bedwars turnuvasına hazırlanıyorum" for the day, and pins on in the
// crew's chat: the meeting time KorAtesi pinned this evening ("KorAtesi bir mesajı sabitledi.")
// and Saturday's season start. `&socNote=full` puts a third pin there (so "Sabitle" asks first);
// `&socNote=off` keeps both switches off (the service's state before the lead turns them on).
import type {
  ChatMessage,
  ChatPin,
  SocialSnapshot,
  SocNoteState,
  StatusNote,
} from "./social-types";

const ME = "100";
const NETHER = "305";
const KOR = "302";
const CREW_CONV = "c:0123456789abcdef";

export function socNoteDemo(param: (key: string) => string | null): boolean {
  const mode = param("socNote");
  return mode === "demo" || mode === "full" || mode === "off";
}

export interface SocNoteHost {
  param: (key: string) => string | null;
  pick: (tr: string, en: string) => string;
  view: () => SocialSnapshot;
  commit: (change: (draft: SocialSnapshot) => SocialSnapshot) => void;
  /** The crew chat's messages as this preview holds them. */
  messages: (conv: string) => readonly ChatMessage[];
}

const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** The end of the player's day (local midnight), in Unix seconds. */
function tonight(nowMs: number): number {
  const end = new Date(nowMs);
  end.setHours(24, 0, 0, 0);
  return Math.floor(end.getTime() / 1000);
}

function pinOf(message: ChatMessage | undefined, pinnedAt: number): ChatPin | null {
  if (!message || message.id === undefined) return null;
  return { id: message.id, from: message.from, text: message.text, pinnedAt };
}

/** The snapshot with SOC-NOTE's preview data (only with `&socNote=`). */
export function withSocNote(snapshot: SocialSnapshot, host: SocNoteHost): SocialSnapshot {
  if (!socNoteDemo(host.param) || !snapshot.chat) return snapshot;
  const mode = host.param("socNote");
  const on = mode !== "off";
  const now = Date.now();
  const crew = host.messages(CREW_CONV);
  const find = (words: string) => crew.find((m) => m.text.includes(words));
  const meeting = find(host.pick("7:45", "7:45"));
  const season = find(host.pick("cumartesi", "Saturday"));
  const third = find(host.pick("turnuva var", "tournament at 8"));
  const pins = [
    pinOf(season, now - 26 * 3_600_000),
    pinOf(meeting, now - 8 * 60_000),
    mode === "full" ? pinOf(third, now - 2 * 60_000) : null,
  ].filter((pin): pin is ChatPin => pin !== null);
  const own: StatusNote = {
    text: host.pick("Bedwars turnuvasına hazırlanıyorum", "Getting ready for the Bedwars cup"),
    until: tonight(now),
  };
  const notes: Record<string, StatusNote> = {
    [NETHER]: {
      text: host.pick("Sınav haftası, akşam bakarım", "Exam week, back tonight"),
      until: 0,
    },
    [KOR]: {
      text: host.pick("Bedwars'ta, mesaj atın", "In Bedwars, text me"),
      until: tonight(now),
    },
  };
  const socNote: SocNoteState = {
    notes: on,
    note: on ? own : undefined,
    pinsOn: on,
    pins: on && pins.length ? { [CREW_CONV]: pins } : {},
    pinLines:
      on && meeting?.id !== undefined
        ? [{ conv: CREW_CONV, by: KOR, id: meeting.id, at: now - 8 * 60_000 }]
        : [],
  };
  return {
    ...snapshot,
    friends: snapshot.friends.map((friend) =>
      on && notes[friend.id] ? { ...friend, note: notes[friend.id] } : friend,
    ),
    chat: { ...snapshot.chat, socNote },
  };
}

function withState(
  snapshot: SocialSnapshot,
  change: (state: SocNoteState) => SocNoteState,
): SocialSnapshot {
  const state = snapshot.chat?.socNote;
  if (!snapshot.chat || !state) return snapshot;
  return { ...snapshot, chat: { ...snapshot.chat, socNote: change(state) } };
}

class MockNoteError extends Error {
  readonly code: string;
  readonly details: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
    this.details = `Kod: ${code}\nÖnizleme verisi`;
  }
}

/** The preview's ways to set a note and to pin (as the service answers them). */
export function socNoteApi(host: SocNoteHost) {
  return {
    async setStatusNote(text: string, until: number) {
      await delay(260);
      if (/\d{3}\s?\d{3}\s?\d{2}\s?\d{2}/.test(text)) {
        throw new MockNoteError(
          "SOSYAL-DURUM-NOT",
          host.pick("Notta telefon numarası olamaz.", "A note can't have a phone number."),
        );
      }
      host.commit((draft) =>
        withState(draft, (state) => ({
          ...state,
          note: text ? { text, until } : undefined,
        })),
      );
    },
    async chatPin(conv: string, id: number, replace?: number) {
      await delay(240);
      const message = host.messages(conv).find((m) => m.id === id);
      if (!message) {
        throw new MockNoteError(
          "SOSYAL-SOHBET-MESAJ",
          host.pick("Bu mesaj artık yok.", "This message is gone."),
        );
      }
      host.commit((draft) =>
        withState(draft, (state) => {
          const kept = (state.pins[conv] ?? []).filter((pin) => pin.id !== replace);
          if (kept.some((pin) => pin.id === id)) return state;
          const pin = pinOf(message, Date.now());
          const next = pin ? [...kept, pin].slice(-3) : kept;
          return {
            ...state,
            pins: { ...state.pins, [conv]: next },
            pinLines: [...state.pinLines, { conv, by: ME, id, at: Date.now() }],
          };
        }),
      );
    },
    async chatUnpin(conv: string, id: number) {
      await delay(200);
      host.commit((draft) =>
        withState(draft, (state) => {
          const next = (state.pins[conv] ?? []).filter((pin) => pin.id !== id);
          const pins = { ...state.pins };
          if (next.length) pins[conv] = next;
          else delete pins[conv];
          return { ...state, pins };
        }),
      );
    },
  };
}
