// Browser preview of chat (CHAT1), joined into the friends preview (mock-social.ts): direct
// messages and the crew's chat, sending (on its way, then sent), editing, deleting for everyone,
// "Geçmişi temizle", live messages and the states the composer can be in.
//
// Scenarios (`?social=<scenario>`, read once; the player leads the preview crew in all of them):
// - `chat`: chat on, conversations with friends and the crew, some unread (`&chat=1` turns chat
//   on in any other scenario);
// - `chatLive`: a friend's message arrives shortly after start, a crewmate's after it
//   (`&chatAt=<ms>`);
// - `chatFailed`: one of the player's messages did not go;
// - `chatEmpty`: chat on, no conversations yet;
// - `chatMuted`: moderators muted the player's chat for a day; `chatOff`: the service turned chat
//   off (what this computer holds stays readable); `chatMailbox`: messages reach only those online.
// - `chatTombstones` (FIX2): deleted messages of every kind in Ayşe's conversation and the crew's,
//   which Ayşe leads here (the player's, the sender's, the leader's, a moderator's, Projects'
//   moderators', an account that is gone); `chatDeleteLive`: Ayşe deletes her newest message
//   shortly after start, then Projects' moderators one in the crew (`&deleteAt=<ms>`).
// A message with "hata" or "fail" in it does not go (`&sendFails=<reason>` fails every one with
// that reason); a friend online answers a direct message (`&noReply=1` keeps them quiet). A
// message deleted for everyone stays as its tombstone.
import { pick } from "./mock-lang";
import type {
  ChatChange,
  ChatMessage,
  ChatPage,
  ChatState,
  Conversation,
  SendFailure,
  SocialApi,
  SocialNotice,
  SocialSnapshot,
} from "./social-types";

export const CHAT_SCENARIOS: readonly string[] = [
  "chat",
  "chatLive",
  "chatFailed",
  "chatEmpty",
  "chatMuted",
  "chatOff",
  "chatMailbox",
  "chatTombstones",
  "chatDeleteLive",
];

/** A message deleted for everyone: its tombstone (its text gone, nothing to do with it). */
function tombstoneOf(message: ChatMessage, by: string, at = Date.now()): ChatMessage {
  return {
    ...message,
    text: "",
    edited: false,
    editUntil: undefined,
    deleteUntil: undefined,
    deleted: { by, at },
  };
}

/** What the chat preview needs of the friends preview. */
export interface ChatHost {
  scenario: string;
  param(key: string): string | null;
  view(): SocialSnapshot;
  commit(change: (draft: SocialSnapshot) => SocialSnapshot): void;
  tell(notice: SocialNotice): void;
}

type ChatMethods = Pick<
  SocialApi,
  | "onChat"
  | "chatPage"
  | "chatSend"
  | "chatRetry"
  | "chatDiscard"
  | "chatEdit"
  | "chatDelete"
  | "chatRead"
  | "chatClear"
>;

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const EDIT_WINDOW = 15 * MINUTE;
const DELETE_WINDOW = 7 * DAY;
const PAGE = 60;
const ME = "100";
/** The preview crew (mock-social.ts `previewTeam`). */
const CREW = "0123456789abcdef";
const CREW_CONV = `c:${CREW}`;
/** The preview's ids all have three digits: the player's is the smaller one. */
const direct = (peer: string) => `d:${ME}:${peer}`;

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

class MockChatError extends Error {
  readonly code: string;
  readonly details: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
    this.details = `Kod: ${code}\nÖnizleme verisi`;
  }
}

const SEND_FAILURES: readonly SendFailure[] = [
  "link",
  "phone",
  "words",
  "invalid",
  "tooFast",
  "dailyLimit",
  "muted",
  "off",
  "notAllowed",
  "connection",
];

/** Lines the crew wrote over the last days (older pages to scroll back to). */
const CREW_LINES: ReadonlyArray<readonly [string, string]> = [
  ["Bugün etkinlik var mı?", "Any event today?"],
  ["Akşam 8'de turnuva var", "There's a tournament at 8 tonight"],
  ["Ben varım", "I'm in"],
  ["Kim skyblock'a geliyor?", "Who's coming to Skyblock?"],
  ["Adada yer açtım, gelin", "I made room on the island, come over"],
  ["Yeni sezon ne zaman başlıyor?", "When does the new season start?"],
  ["Duyurularda yazıyordu, cumartesi", "The news said Saturday"],
  ["Kılıç setini kim aldı?", "Who took the sword set?"],
  ["Bende kaldı, akşam veririm", "I have it, I'll give it back tonight"],
  ["Tamamdır", "Alright"],
  ["Sunucu bugün çok akıcı", "The server runs really smooth today"],
  ["Haftaya PvP antrenmanı yapalım mı?", "PvP practice next week?"],
  ["Olur, ben hazırım", "Sure, I'm ready"],
  ["Bugün gelemem, yarın varım", "Can't make it today, tomorrow I'm in"],
  ["Ekip kodunu Bora'ya da attım", "I sent the crew code to Bora too"],
];
const CREW_SPEAKERS = ["701", "702", "302", "703", "301", ME, "306"];

export function mockChat(host: ChatHost) {
  const threads = new Map<string, ChatMessage[]>();
  const unread = new Map<string, number>();
  const titles = new Map<string, string>();
  const listeners = new Set<(change: ChatChange) => void>();
  // A limited account (SPAM1) has chat too: its daily message limit is listed only with chat.
  const on =
    CHAT_SCENARIOS.includes(host.scenario) ||
    host.scenario === "limited" ||
    host.param("chat") !== null;
  let nonce = 0;

  function message(
    conv: string,
    from: string,
    text: string,
    at: number,
    extra: Partial<ChatMessage> = {},
  ): ChatMessage {
    const mine = from === ME;
    // The preview crew is led by the player: they may delete any of its messages for a week (not
    // in `chatTombstones`, where Ayşe leads it).
    const deletes = mine || (conv === CREW_CONV && host.scenario !== "chatTombstones");
    return {
      key: `i:${at}`,
      id: at,
      from,
      mine,
      text,
      at,
      edited: false,
      state: "sent",
      editUntil: mine ? at + EDIT_WINDOW : undefined,
      deleteUntil: deletes ? at + DELETE_WINDOW : undefined,
      ...extra,
    };
  }

  function seed() {
    const t0 = Date.now();
    const yesterday = t0 - 26 * HOUR;
    const ayse = direct("301");
    threads.set(ayse, [
      message(
        ayse,
        "301",
        pick("Akşam bedwars'a girecek misin?", "Are you playing Bedwars tonight?"),
        yesterday,
      ),
      message(ayse, ME, pick("Girerim, 9 gibi", "Sure, around 9"), yesterday + 2 * MINUTE),
      message(
        ayse,
        "301",
        pick("Süper, KorAteşi de geliyormuş", "Great, KorAteşi is coming too"),
        yesterday + 3 * MINUTE,
      ),
      message(
        ayse,
        ME,
        pick(
          "Takım kuralım o zaman. Ben yatağı korurum, siz ortaya koşun",
          "Let's team up then. I'll guard the bed, you two rush the middle",
        ),
        yesterday + 4 * MINUTE,
        { edited: true },
      ),
      message(ayse, "301", pick("Bugün de var mısın?", "Up for it again today?"), t0 - 14 * MINUTE),
      message(
        ayse,
        "301",
        pick("Lobideyim, seni bekliyorum", "I'm in the lobby, waiting for you"),
        t0 - 3 * MINUTE,
      ),
    ]);
    unread.set(ayse, 2);

    const kor = direct("302");
    threads.set(kor, [
      message(
        kor,
        "302",
        pick("Dünkü maç efsaneydi", "Yesterday's match was legendary"),
        t0 - 3 * HOUR,
      ),
      message(
        kor,
        ME,
        pick("Son raundu nasıl çevirdin anlamadım", "No idea how you turned that last round"),
        t0 - 3 * HOUR + MINUTE,
      ),
      message(kor, "302", pick("Yılların tecrübesi :)", "Years of practice :)"), t0 - 2 * HOUR),
      message(kor, ME, pick("Tamam, yarın bakarız", "Alright, let's see tomorrow"), t0 - 2 * HOUR),
    ]);

    const elif = direct("305");
    threads.set(elif, [
      message(
        elif,
        "305",
        pick("Haritadaki köyü buldun mu?", "Did you find the village on the map?"),
        t0 - 3 * DAY,
      ),
      message(
        elif,
        ME,
        pick("Buldum, koordinatları birazdan atarım", "Found it, I'll send the coordinates soon"),
        t0 - 3 * DAY + 5 * MINUTE,
      ),
    ]);

    // Two days of the crew's chat, then this evening's plan, the last three unread.
    const crew: ChatMessage[] = [];
    const start = t0 - 2 * DAY - 6 * HOUR;
    for (let i = 0; i < 84; i += 1) {
      const [tr, en] = CREW_LINES[i % CREW_LINES.length] ?? ["", ""];
      const from = CREW_SPEAKERS[(i * 3) % CREW_SPEAKERS.length] ?? "701";
      crew.push(message(CREW_CONV, from, pick(tr, en), start + i * 23 * MINUTE));
    }
    crew.push(
      message(
        CREW_CONV,
        "701",
        pick("Akşam turnuvaya kim geliyor?", "Who's coming to the tournament tonight?"),
        t0 - 40 * MINUTE,
      ),
      message(CREW_CONV, "702", pick("Ben varım!", "I'm in!"), t0 - 38 * MINUTE),
      message(
        CREW_CONV,
        ME,
        pick("Ben de. Saat kaçta başlıyor?", "Me too. What time does it start?"),
        t0 - 35 * MINUTE,
      ),
      message(
        CREW_CONV,
        "302",
        pick("8'de, 7:45'te lobide buluşalım", "At 8, let's meet in the lobby at 7:45"),
        t0 - 9 * MINUTE,
      ),
      message(CREW_CONV, "703", pick("Geliyorum", "On my way"), t0 - 6 * MINUTE),
      message(
        CREW_CONV,
        "701",
        pick("5 dakikaya oradayım", "I'll be there in 5 minutes"),
        t0 - 2 * MINUTE,
      ),
    );
    // The limited preview has no crew.
    if (host.scenario !== "limited") {
      threads.set(CREW_CONV, crew);
      unread.set(CREW_CONV, 3);
    }

    if (host.scenario === "chatTombstones") {
      // Ayşe's conversation: hers, the player's, and one Projects' moderators took away.
      const dm = threads.get(ayse) ?? [];
      threads.set(
        ayse,
        dm.map((m, i) =>
          i === 2
            ? tombstoneOf(m, "301", m.at + MINUTE)
            : i === 3
              ? tombstoneOf(m, ME, m.at + MINUTE)
              : i === 5
                ? tombstoneOf(m, "staff", m.at + MINUTE)
                : m,
        ),
      );
      // The crew's newest: the leader's (Ayşe) deletion and a moderator's (KorAteşi) of others'
      // messages, the player's and a sender's of their own, and one of an account that is gone.
      const tail = crew.length;
      const by: Record<number, string> = {
        [tail - 1]: "301",
        [tail - 2]: "302",
        [tail - 4]: ME,
        [tail - 5]: "",
        [tail - 6]: "701",
      };
      threads.set(
        CREW_CONV,
        crew.map((m, i) => {
          const who = by[i];
          return who === undefined ? m : tombstoneOf(m, who, m.at + MINUTE);
        }),
      );
      unread.set(ayse, 1);
      unread.set(CREW_CONV, 1);
    }

    if (host.scenario === "chatFailed") {
      const failed: ChatMessage = {
        key: "n:preview-failed",
        from: ME,
        mine: true,
        text: pick("9'da girerim, beni bekleyin", "I'll join at 9, wait for me"),
        at: t0 - MINUTE,
        edited: false,
        state: "failed",
        failure: "connection",
      };
      threads.set(ayse, [...(threads.get(ayse) ?? []), failed]);
    }
  }

  function last(conv: string): Conversation["last"] {
    const messages = threads.get(conv) ?? [];
    const newest = messages[messages.length - 1];
    return newest
      ? {
          from: newest.from,
          mine: newest.mine,
          text: newest.text,
          at: newest.at,
          deletedBy: newest.deleted?.by,
        }
      : undefined;
  }

  function conversations(): Conversation[] {
    const list: Conversation[] = [];
    for (const conv of threads.keys()) {
      const crew = conv.startsWith("c:") ? conv.slice(2) : undefined;
      const peer = crew ? undefined : conv.split(":").find((id, i) => i > 0 && id !== ME);
      list.push({
        id: conv,
        peer,
        crew,
        title: titles.get(conv),
        last: last(conv),
        unread: unread.get(conv) ?? 0,
        writable: true,
        failed: (threads.get(conv) ?? []).some((m) => m.state === "failed"),
      });
    }
    return list.sort((a, b) => (b.last?.at ?? 0) - (a.last?.at ?? 0));
  }

  function emit(change: Partial<ChatChange> & { conv: string }) {
    const full: ChatChange = { upsert: [], remove: [], reset: false, ...change };
    for (const listener of listeners) listener(full);
  }

  /** The list in the snapshot follows the messages. */
  function refresh() {
    host.commit((v) =>
      v.chat ? { ...v, chat: { ...v.chat, conversations: conversations() } } : v,
    );
  }

  function put(conv: string, next: ChatMessage) {
    const messages = threads.get(conv) ?? [];
    const at = messages.findIndex((m) => m.key === next.key);
    const updated = at >= 0 ? messages.map((m, i) => (i === at ? next : m)) : [...messages, next];
    threads.set(conv, updated);
    emit({ conv, upsert: [next] });
    refresh();
  }

  function find(key: string): [string, ChatMessage] | undefined {
    for (const [conv, messages] of threads) {
      const found = messages.find((m) => m.key === key);
      if (found) return [conv, found];
    }
    return undefined;
  }

  function byId(conv: string, id: number): ChatMessage {
    const found = threads.get(conv)?.find((m) => m.id === id);
    if (!found) {
      throw new MockChatError(
        "SOSYAL-MESAJ-YOK",
        pick("Bu mesaj artık yok.", "This message is gone."),
      );
    }
    return found;
  }

  /** Someone else writes: the list counts it, the launcher is told. */
  function arrive(conv: string, from: string, text: string) {
    const at = Date.now();
    const next = message(conv, from, text, at);
    unread.set(conv, (unread.get(conv) ?? 0) + 1);
    put(conv, next);
    const view = host.view();
    const crew = conv === CREW_CONV ? CREW : undefined;
    const name =
      view.friends.find((f) => f.id === from)?.name ??
      view.crews.flatMap((c) => c.members).find((m) => m.id === from)?.name;
    host.tell({ kind: "chatMessage", conv, message: next, name, crew });
  }

  /** On its way, then sent (or not, with why). */
  function deliver(conv: string, pending: ChatMessage) {
    put(conv, pending);
    const forced = host.param("sendFails") as SendFailure | null;
    const fails =
      forced && SEND_FAILURES.includes(forced)
        ? forced
        : /hata|fail/i.test(pending.text)
          ? "connection"
          : null;
    setTimeout(
      () => {
        if (fails) {
          put(conv, { ...pending, state: "failed", failure: fails });
          return;
        }
        const id = Date.now();
        put(conv, {
          ...pending,
          id,
          at: id,
          state: "sent",
          failure: undefined,
          editUntil: id + EDIT_WINDOW,
          deleteUntil: id + DELETE_WINDOW,
        });
        // A friend online answers a direct message (not with `&noReply=1`).
        const peer = conv.startsWith("d:")
          ? conv.split(":").find((p, i) => i > 0 && p !== ME)
          : null;
        const friend = host.view().friends.find((f) => f.id === peer);
        if (friend && friend.state !== "offline" && !fails && host.param("noReply") === null) {
          setTimeout(
            () => arrive(conv, friend.id, pick("Tamamdır, görüşürüz!", "Sounds good, see you!")),
            2200,
          );
        }
      },
      fails ? 700 : 420,
    );
  }

  /** The snapshot with chat as the scenario has it. */
  function prepare(snapshot: SocialSnapshot): SocialSnapshot {
    if (!on) return snapshot;
    if (threads.size === 0 && host.scenario !== "chatEmpty") seed();
    const state: ChatState = {
      on: host.scenario !== "chatOff",
      mailbox: host.scenario !== "chatMailbox",
      perDay: snapshot.me?.limited ? 100 : 500,
      mutedUntil: host.scenario === "chatMuted" ? Math.floor((Date.now() + DAY) / 1000) : undefined,
      conversations: conversations(),
    };
    return {
      ...snapshot,
      chatAllowed: true,
      chat: snapshot.auth === "signedIn" ? state : undefined,
    };
  }

  /** Someone else deletes a message: its tombstone comes as a change (`chatDeleteLive`). */
  function deleteLive(conv: string, from: string, by: string) {
    const found = [...(threads.get(conv) ?? [])].reverse().find((m) => m.from === from);
    if (!found || found.deleted) return;
    if (!found.mine && (unread.get(conv) ?? 0) > 0) unread.set(conv, (unread.get(conv) ?? 1) - 1);
    put(conv, tombstoneOf(found, by));
  }

  /** Live messages of `chatLive`, live deletions of `chatDeleteLive`. */
  function start() {
    if (typeof window === "undefined") return;
    if (host.scenario === "chatDeleteLive") {
      const at = Number(host.param("deleteAt")) || 2400;
      setTimeout(() => deleteLive(direct("301"), "301", "301"), at);
      setTimeout(() => deleteLive(CREW_CONV, "702", "staff"), at + 2600);
      return;
    }
    if (host.scenario !== "chatLive") return;
    const at = Number(host.param("chatAt")) || 1600;
    setTimeout(
      () => arrive(direct("301"), "301", pick("Hazır mısın? Başlıyoruz", "Ready? We're starting")),
      at,
    );
    setTimeout(
      () => arrive(CREW_CONV, "702", pick("Lobideyim, kapıdayım", "In the lobby, at the gate")),
      at + 2600,
    );
  }

  const api: ChatMethods = {
    onChat(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    async chatPage(conv, before): Promise<ChatPage> {
      await delay(140);
      const all = threads.get(conv) ?? [];
      const older =
        before === undefined ? all : all.filter((m) => m.id !== undefined && m.id < before);
      const page = older.slice(-PAGE);
      return { conv, messages: page, more: older.length > page.length };
    },
    async chatSend(conv, text) {
      await delay(30);
      const chat = host.view().chat;
      if (!chat?.on) {
        throw new MockChatError(
          "SOSYAL-SOHBET-KAPALI",
          pick("Sohbet şu an kapalı.", "Chat is off right now."),
        );
      }
      nonce += 1;
      const at = Date.now();
      const key = `n:preview${nonce}`;
      deliver(conv, {
        key,
        from: ME,
        mine: true,
        text: text.trim(),
        at,
        edited: false,
        state: "sending",
      });
      return key;
    },
    async chatRetry(key) {
      await delay(60);
      const found = find(key);
      if (!found) return;
      const [conv, failed] = found;
      // "hata" in the text fails again; anything else goes this time.
      deliver(conv, { ...failed, state: "sending", failure: undefined });
    },
    async chatDiscard(key) {
      await delay(60);
      const found = find(key);
      if (!found) return;
      const [conv] = found;
      threads.set(
        conv,
        (threads.get(conv) ?? []).filter((m) => m.key !== key),
      );
      emit({ conv, remove: [key] });
      refresh();
    },
    async chatEdit(conv, id, text) {
      await delay(320);
      const found = byId(conv, id);
      if ((found.editUntil ?? 0) < Date.now()) {
        throw new MockChatError(
          "SOSYAL-MESAJ-ESKI",
          pick("Bu mesajı düzenleme süresi doldu.", "This message can no longer be edited."),
        );
      }
      put(conv, { ...found, text: text.trim(), edited: true });
    },
    async chatDelete(conv, id) {
      await delay(300);
      const found = byId(conv, id);
      if ((found.deleteUntil ?? 0) < Date.now()) {
        throw new MockChatError(
          "SOSYAL-MESAJ-ESKI",
          pick(
            "Bu mesaj artık herkesten silinemez.",
            "This message can no longer be deleted for everyone.",
          ),
        );
      }
      // Its tombstone stays in its place, under the same key.
      put(conv, tombstoneOf(found, ME));
    },
    async chatRead(conv) {
      if (!unread.get(conv)) return;
      unread.set(conv, 0);
      refresh();
    },
    async chatClear(conv) {
      await delay(200);
      threads.set(conv, []);
      unread.set(conv, 0);
      emit({ conv, reset: true });
      refresh();
    },
  };

  return { prepare, start, api };
}
