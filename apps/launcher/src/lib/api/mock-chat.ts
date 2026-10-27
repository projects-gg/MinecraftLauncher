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
// - `chat2` (CHAT2): the extras on (`&chat2=1` in any other scenario; `&extras=replies,stickers`
//   keeps only those, `&extras=none` a service with none of them on): replies, reactions, a
//   sticker, links with their cards, "Görüldü" under the player's message to KorAteşi;
//   `chat2Live`: Ayşe writes ("yazıyor…") and sends a sticker, then reacts, a crewmate writes too.
// Long histories (CHAT-PROD): `&crewHistory=<n>` puts n older lines in the crew's chat (84 by
// default), `&crewUnread=<n>` leaves the last n of it unread (3 by default).
// A message with "hata" or "fail" in it does not go (`&sendFails=<reason>` fails every one with
// that reason); a friend online answers a direct message (`&noReply=1` keeps them quiet): with
// CHAT2 they see it first ("Görüldü") and write ("yazıyor…"). A message deleted for everyone stays
// as its tombstone.
import { pick } from "./mock-lang";
import type {
  ChatCard,
  ChatChange,
  ChatExtras,
  ChatMessage,
  ChatPage,
  ChatReaction,
  ChatState,
  ChatTyping,
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
  "chat2",
  "chat2Live",
];

const EXTRA_KEYS = ["replies", "stickers", "reactions", "typing", "reads", "links"] as const;

/** The link rules the preview service applies (the contract's defaults). */
const PREVIEW_LINKS = {
  rules: 2,
  trusted: [
    "projects.gg",
    "youtube.com",
    "youtu.be",
    "twitch.tv",
    "kick.com",
    "modrinth.com",
    "minecraft.net",
    "minecraft.wiki",
  ],
  blocked: ["youtube.com/@dolandirici"],
  invites: true,
};

/** A picture for a preview card (an SVG; the real hub sends a small JPEG). */
function picture(top: string, bottom: string, label: string): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180" viewBox="0 0 320 180"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${top}"/><stop offset="1" stop-color="${bottom}"/></linearGradient></defs><rect width="320" height="180" fill="url(#g)"/><text x="22" y="150" font-family="Tektur, sans-serif" font-size="44" font-weight="700" fill="#fbf6ec" opacity="0.92">${label}</text></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/** The card the preview hub makes for a link. */
function previewCard(url: string): ChatCard {
  const host = new URL(url).hostname.replace(/^www\./, "");
  if (host === "youtu.be" || host.endsWith("youtube.com")) {
    return {
      state: "ready",
      url,
      site: "YouTube",
      icon: "youtube",
      kind: "video",
      title: pick(
        "Bedwars'ta 1'e 4 kazandık: son raunt",
        "We won a 1v4 in Bedwars: the last round",
      ),
      author: "KorAteşi",
      image: picture("#3f0a0a", "#c9741a", "1v4"),
      w: 320,
      h: 180,
    };
  }
  if (host.endsWith("twitch.tv")) {
    return {
      state: "ready",
      url,
      site: "Twitch",
      icon: "twitch",
      kind: "stream",
      title: pick("Projects turnuvası canlı", "The Projects tournament, live"),
      image: picture("#1b1820", "#7a2416", "CANLI"),
      w: 320,
      h: 180,
    };
  }
  if (host.endsWith("modrinth.com")) {
    return {
      state: "ready",
      url,
      site: "Modrinth",
      icon: "modrinth",
      kind: "project",
      title: "Sodium",
      desc: pick(
        "Oyunu çok daha akıcı çalıştıran bir çizim motoru.",
        "A rendering engine that makes the game run much smoother.",
      ),
      image: picture("#27222c", "#5fcb8e", "S"),
      w: 96,
      h: 96,
    };
  }
  if (host.includes("kotu"))
    return { state: "refused", url, site: host, icon: "globe", kind: "page", rule: "blocked" };
  if (host.includes("bos")) return { state: "plain", url, site: host, icon: "globe", kind: "page" };
  return {
    state: "ready",
    url,
    site: host,
    icon: "globe",
    kind: "page",
    title: pick("Skyblock için en iyi ada düzenleri", "The best island layouts for Skyblock"),
    desc: pick("Başlangıçtan sona adım adım rehber.", "A step-by-step guide from start to end."),
  };
}

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
  | "chatSendSticker"
  | "chatReact"
  | "chatTyping"
  | "chatUnfurl"
  | "chatCard"
  | "onChatTyping"
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
  const typingListeners = new Set<(typing: ChatTyping) => void>();
  const peerReads = new Map<string, number>();
  // CHAT2's extras: on in its scenarios (or with `&chat2=1`), `&extras=` picks them.
  const chat2 = host.scenario.startsWith("chat2") || host.param("chat2") !== null;
  const extras = (): ChatExtras | undefined => {
    if (!chat2) return undefined;
    const only = host.param("extras");
    const on = (key: string) => !only || only.split(",").includes(key);
    return Object.fromEntries(EXTRA_KEYS.map((key) => [key, on(key)])) as unknown as ChatExtras;
  };
  // A limited account (SPAM1) has chat too: its daily message limit is listed only with chat.
  const on =
    CHAT_SCENARIOS.includes(host.scenario) ||
    chat2 ||
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
    const history = Math.min(5_000, Math.max(0, Number(host.param("crewHistory") ?? 84) || 0));
    const start = history === 84 ? t0 - 2 * DAY - 6 * HOUR : t0 - 22 * HOUR - history * 23 * MINUTE;
    for (let i = 0; i < history; i += 1) {
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
      unread.set(CREW_CONV, Math.max(0, Number(host.param("crewUnread") ?? 3) || 0));
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

    if (chat2) seedChat2(t0);

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

  /** CHAT2's preview: a reply, a sticker, reactions, links with cards, a read mark. */
  function seedChat2(t0: number) {
    const ayse = direct("301");
    const dm = threads.get(ayse) ?? [];
    const asked = dm.find((m) => m.text.startsWith(pick("Bugün de", "Up for")));
    const reply = (target: ChatMessage | undefined) =>
      target?.id !== undefined
        ? {
            id: target.id,
            from: target.from,
            key: target.key,
            text: target.text,
            deleted: false,
            missing: false,
          }
        : undefined;
    const r = (e: string, by: string[]): ChatReaction => ({ e, by, mine: by.includes(ME) });
    const mineReply = message(
      ayse,
      ME,
      pick("Varım, 10 dakikaya geliyorum", "I'm in, coming in 10 minutes"),
      t0 - 12 * MINUTE,
      {
        reply: reply(asked),
        reactions: [r("👍", ["301"])],
      },
    );
    const sticker = message(ayse, "301", "", t0 - 10 * MINUTE, { sticker: "pl1/come" });
    const link = message(
      ayse,
      ME,
      pick(
        "Dünkü maçı izle: https://youtu.be/kor1v4 son raunt efsane",
        "Watch yesterday's match: https://youtu.be/kor1v4 the last round is legendary",
      ),
      t0 - 9 * MINUTE,
      { card: previewCard("https://youtu.be/kor1v4") },
    );
    const answer = message(
      ayse,
      "301",
      pick("Efsane olmuş 😂", "That's legendary 😂"),
      t0 - 7 * MINUTE,
      {
        reply: reply(link),
        reactions: [r("😂", [ME, "301"]), r("🔥", [ME])],
      },
    );
    const lobby = dm.at(-1);
    threads.set(
      ayse,
      [
        ...dm.filter((m) => m !== lobby),
        mineReply,
        sticker,
        link,
        answer,
        ...(lobby ? [lobby] : []),
      ].sort((a, b) => (a.id ?? 0) - (b.id ?? 0)),
    );
    // KorAteşi saw the player's last message (no answer yet): "Görüldü" under it.
    const kor = direct("302");
    const korMine = (threads.get(kor) ?? []).filter((m) => m.mine).at(-1);
    if (korMine?.id !== undefined) peerReads.set(kor, korMine.id);

    // The crew: reactions, a reply, a sticker, two links.
    const crew = threads.get(CREW_CONV);
    if (crew) {
      const asking = crew.find((m) => m.text.startsWith(pick("Akşam turnuvaya", "Who's coming")));
      const tail = crew
        .slice(-6)
        .map((m, i) =>
          i === 1
            ? { ...m, reactions: [r("👍", ["701", "302", "703", ME]), r("🔥", ["702", ME])] }
            : m,
        );
      const more = [
        message(CREW_CONV, "702", "", t0 - 30 * MINUTE, { sticker: "pl1/gg" }),
        message(
          CREW_CONV,
          "302",
          pick("Ben de geliyorum, kılıcı getiriyorum", "I'm coming too, bringing the sword"),
          t0 - 28 * MINUTE,
          {
            reply: reply(asking),
            reactions: [r("⚔️", ["701"])],
          },
        ),
        message(
          CREW_CONV,
          "703",
          pick(
            "Gelmeden şunu kurun: https://modrinth.com/mod/sodium oyun çok akıcı oluyor",
            "Install this first: https://modrinth.com/mod/sodium the game gets so smooth",
          ),
          t0 - 26 * MINUTE,
          { card: previewCard("https://modrinth.com/mod/sodium") },
        ),
        message(
          CREW_CONV,
          "701",
          pick(
            "Ada planı burada: https://www.skyblockrehber.com/adalar",
            "The island plan is here: https://www.skyblockguide.net/islands",
          ),
          t0 - 24 * MINUTE,
        ),
      ];
      threads.set(
        CREW_CONV,
        [...crew.slice(0, -6), ...tail, ...more].sort((a, b) => (a.id ?? 0) - (b.id ?? 0)),
      );
    }
  }

  function typing(conv: string, by: string) {
    for (const listener of typingListeners) listener({ conv, by });
  }

  /** The message a reply answers, as this computer holds it. */
  function replyOf(conv: string, id: number | undefined): ChatMessage["reply"] {
    if (id === undefined) return undefined;
    const target = threads.get(conv)?.find((m) => m.id === id);
    return target
      ? {
          id,
          from: target.from,
          key: target.key,
          text: target.text,
          sticker: target.sticker,
          deleted: Boolean(target.deleted),
          missing: false,
        }
      : { id, from: "", deleted: false, missing: true };
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
          sticker: newest.sticker,
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
        peerRead: peerReads.get(conv),
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
          if (chat2) {
            setTimeout(() => {
              peerReads.set(conv, id);
              refresh();
            }, 900);
            setTimeout(() => typing(conv, friend.id), 1300);
            setTimeout(() => typing(conv, friend.id), 3300);
          }
          setTimeout(
            () => arrive(conv, friend.id, pick("Tamamdır, görüşürüz!", "Sounds good, see you!")),
            chat2 ? 4600 : 2200,
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
      extras: extras(),
      stickerSets: chat2 ? ["pl1"] : undefined,
      links: chat2 ? PREVIEW_LINKS : undefined,
    };
    const me =
      chat2 && snapshot.me
        ? {
            ...snapshot.me,
            privacy: {
              shareRead: true,
              shareTyping: true,
              ...snapshot.me.privacy,
            },
          }
        : snapshot.me;
    return {
      ...snapshot,
      me,
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
    if (host.scenario === "chat2Live") {
      // Ayşe and two crewmates write: a frame every 3 s, as a writer's launcher sends them,
      // until Ayşe's sticker comes (`chatAt` ms after the start; the crew stops then too).
      const at = Number(host.param("chatAt")) || 9000;
      const ayse = direct("301");
      const write = () => {
        typing(ayse, "301");
        typing(CREW_CONV, "702");
        typing(CREW_CONV, "302");
      };
      write();
      const writing = setInterval(write, 3000);
      setTimeout(() => {
        clearInterval(writing);
        const next = message(ayse, "301", "", Date.now(), { sticker: "pl1/hi" });
        unread.set(ayse, (unread.get(ayse) ?? 0) + 1);
        put(ayse, next);
        const view = host.view();
        host.tell({
          kind: "chatMessage",
          conv: ayse,
          message: next,
          name: view.friends.find((f) => f.id === "301")?.name,
        });
      }, at);
      setTimeout(() => {
        const mine = (threads.get(ayse) ?? []).filter((m) => m.mine && m.id !== undefined).at(-1);
        if (mine) react(ayse, mine, "301", "❤️", true);
      }, at + 1400);
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

  /** Someone's reaction on a message, as the service keeps them (5 each, 10 kinds). */
  function react(conv: string, target: ChatMessage, by: string, emoji: string, add: boolean) {
    const list = (target.reactions ?? []).map((x) => ({ ...x, by: [...x.by] }));
    const same = (a: string, b: string) =>
      a.replace(/\u{FE0F}/gu, "") === b.replace(/\u{FE0F}/gu, "");
    const at = list.findIndex((x) => same(x.e, emoji));
    if (add) {
      if (
        list.filter((x) => x.by.includes(by)).length >= 5 &&
        !(at >= 0 && list[at]?.by.includes(by))
      ) {
        throw new MockChatError(
          "SOSYAL-TEPKI-DOLU",
          pick(
            "Bir mesaja en çok 5 tepki verebilirsin.",
            "You can give a message at most 5 reactions.",
          ),
        );
      }
      if (at < 0 && list.length >= 10) {
        throw new MockChatError(
          "SOSYAL-TEPKI-TUR",
          pick(
            "Bu mesajda yeni bir tepkiye yer yok.",
            "This message has no room for a new reaction.",
          ),
        );
      }
      if (at < 0) list.push({ e: emoji, by: [by], mine: by === ME });
      else if (!list[at]?.by.includes(by)) list[at]?.by.push(by);
    } else if (at >= 0) {
      const kind = list[at];
      if (kind) kind.by = kind.by.filter((id) => id !== by);
    }
    const reactions = list
      .filter((x) => x.by.length > 0)
      .map((x) => ({ ...x, mine: x.by.includes(ME) }));
    put(conv, { ...target, reactions: reactions.length ? reactions : undefined });
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
    async chatSend(conv, text, reply) {
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
        reply: replyOf(conv, reply),
      });
      return key;
    },
    async chatSendSticker(conv, sticker, reply) {
      await delay(30);
      nonce += 1;
      const key = `n:preview${nonce}`;
      deliver(conv, {
        key,
        from: ME,
        mine: true,
        text: "",
        sticker,
        at: Date.now(),
        edited: false,
        state: "sending",
        reply: replyOf(conv, reply),
      });
      return key;
    },
    async chatReact(conv, id, emoji, add) {
      await delay(260);
      react(conv, byId(conv, id), ME, emoji, add);
    },
    chatTyping() {},
    async chatUnfurl(conv, key) {
      await delay(700);
      const found = threads.get(conv)?.find((m) => m.key === key);
      const url = found?.text.match(/https:\/\/\S+/u)?.[0];
      if (!found || !url) return;
      put(conv, { ...found, card: previewCard(url) });
    },
    async chatCard(url) {
      await delay(500);
      const card = previewCard(url);
      return card.state === "ready" ? card : null;
    },
    onChatTyping(listener) {
      typingListeners.add(listener);
      return () => typingListeners.delete(listener);
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
