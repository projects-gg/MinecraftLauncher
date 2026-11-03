// Browser preview and tests of voice chat (VOICE-L3): every state the page shows, kept in memory and
// changed by the actions like the app would. Nothing leaves the page. The people are the friends
// mock's (GeceKurdu_ 301, KorAtesi 302, ElmasKazma 303, PixelAvci 304, NetherKral 305) and its
// crew's ("Gece Kartalları": MaviElmas 701, DemirGolem_ 702, TasDevi 703), plus KumFirtinasi (909),
// a friend's friend the lists do not know (named by `profiles`). Pair it with `?social=team` for the
// crew scenarios.
//
// Preview switches, read once from the page URL's query or the route's (`#/…?voice=room`):
// - `voice=<scenario>`: (none) / `ready` on, no room; `off` nothing anywhere; `disabled` the
//   player's own switch is off; `calling` the player's call rings at KorAtesi; `noAnswer` the same,
//   unanswered a moment later ("Cevap yok"); `room` a friend room (the player leads it: GeceKurdu_
//   speaking, ElmasKazma muted, KumFirtinasi, NetherKral ringing); `roomNamed` the same, named;
//   `full` a friend room at its 10; `crew` the crew's "Genel" (`&role=leader|moderator|member`),
//   with "Oda 2" beside it; `crewFull` the crew has its 3 rooms; `ring` GeceKurdu_ calls, two more
//   in his room (`ringLong`: a 16-character tag calls); `missed` two missed calls; `joining`,
//   `reconnecting`, `failed` (the stay ended:
//   "Bağlanamadı"); `denied` / `missing` (the microphone); `weak` / `bad` (the player's
//   connection); `modMuted`, `muted`, `deaf`; VOICE-ROOMS: `crewRooms`, `crewRoomsIn`,
//   `crewRoomsOld`, `crewRoomsBusy` (`keptRooms`; `&change=<refusal>` refuses the crew page's
//   changes); BAN-TYPES: `restricted` the crew's "Genel", left a moment later for a voice chat ban
//   (two days, on the hour; `staffMuted` refusals carry the same end).
// - `speaking=<ids>|none`: who speaks (the scenario's own otherwise); `talking=1`: the player too.
// - `call=answer|noAnswer|<refusal>`: what "Ara" gets (answered after 2.5 s by default).
// - `join=ok|<refusal>`: what "Katıl" gets.
// - `voiceKeys=combo`: shortcuts already set (VOICE-KEYS); `discord=1|skip`: Discord's keys to
//   offer once (VOICE-KEYS).
// - VOICE-CALL: `voice=call1` a call with KorAtesi, live for 4:12; `voice=callingBusy` the call
//   rings at ElmasKazma, who is in another room; `levels=<me>[,<id>:<level>…]` the live meters as
//   still frames (0–1, `me` may end in `!` when it does not go out), else a voice that rises and
//   falls for whoever speaks; `ask=0` "Aramadan önce sor" off; `volume=<percent>` "Ses seviyesi".
import {
  DEFAULT_VOICE_SETTINGS,
  HIDDEN_VOICE_STATE,
  VOICE_OK,
  type VoiceAnswer,
  type VoiceApi,
  type VoiceCrewRooms,
  type VoiceDiscordOffer,
  type VoiceKey,
  type VoiceKeyCheck,
  type VoiceLevel,
  type VoiceLevels,
  type VoiceModAction,
  type VoiceNotice,
  type VoicePerson,
  type VoicePresence,
  type VoiceRefusal,
  type VoiceRole,
  type VoiceRoomView,
  type VoiceSettings,
  type VoiceShortcut,
  type VoiceSpeaking,
  type VoiceState,
} from "./voice-types";

const ME = "100";
const CREW = "0123456789abcdef";
const nowSec = () => Math.floor(Date.now() / 1000);

function param(key: string): string | null {
  if (typeof window === "undefined") return null;
  const fromSearch = new URLSearchParams(window.location.search).get(key);
  const fromHash = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get(key);
  return fromSearch ?? fromHash;
}

const REFUSALS: readonly VoiceRefusal[] = [
  "off",
  "unreachable",
  "rateLimited",
  "accountLimited",
  "staffMuted",
  "roomFull",
  "roomsFull",
  "roomGone",
  "kicked",
  "notMember",
  "notAllowed",
  "notFound",
  "invalidName",
  "nameRejected",
  "failed",
];

const refusalOf = (value: string | null | undefined): VoiceRefusal | null =>
  REFUSALS.find((r) => r === value) ?? null;

/** BAN-TYPES: a voice chat ban's end in the previews: two days on, on the hour. */
const banEnd = () => Math.ceil(nowSec() / 3600) * 3600 + 2 * 86_400;

const refused = (why: VoiceRefusal, retryAfter: number | null = null): VoiceAnswer => ({
  refused: why,
  retryAfter,
  until: why === "staffMuted" ? banEnd() : null,
});

function person(id: string, extra: Partial<VoicePerson> = {}): VoicePerson {
  return {
    id,
    role: "member",
    mute: false,
    deaf: false,
    mutedByModerator: false,
    quality: "good",
    speaking: false,
    volume: 100,
    mutedForMe: false,
    joinedAt: nowSec() - 600,
    can: { mute: true, kick: true, moveTo: false, moderator: true },
    ...extra,
  };
}

/** The friends who may be called, and who is in a room already. */
const PRESENCE: Record<string, VoicePresence> = {
  "301": "ready",
  "302": "ready",
  "303": "in",
  "304": "ready",
  "305": "ready",
  "701": "ready",
  "702": "in",
  "703": "ready",
};

const PROFILES = [
  { id: "909", name: "KumFirtinasi", avatar: "preset:wolf" },
  // The longest a gamer tag gets (16): the ring card's name must never be cut.
  { id: "911", name: "KaranlikSovalye_", avatar: "preset:knight" },
];

function friendRoom(extra: Partial<VoiceRoomView> = {}): VoiceRoomView {
  const start = nowSec() - 1800;
  return {
    id: "f00000000000000a1",
    kind: "friend",
    crew: null,
    name: null,
    general: false,
    max: 10,
    phase: "live",
    me: {
      id: ME,
      role: "leader",
      mute: false,
      deaf: false,
      mutedByModerator: false,
      talking: false,
      quality: "good",
    },
    people: [
      person("301", { joinedAt: start + 10, speaking: true }),
      person("303", { joinedAt: start + 40, mute: true, role: "moderator" }),
      person("909", { joinedAt: start + 300, volume: 140, deaf: true }),
    ],
    ringing: [{ to: "305", until: nowSec() + 24 }],
    canRename: true,
    canClose: true,
    ...extra,
  };
}

function crewRole(): VoiceRole {
  const role = param("role");
  return role === "moderator" || role === "member" ? role : "leader";
}

function crewRoom(role: VoiceRole): VoiceRoomView {
  const start = nowSec() - 3600;
  const lead = role === "leader";
  const mod = role === "moderator";
  // What the player may do to each: a leader to everyone, a moderator to members only.
  const powers = (target: VoiceRole) => {
    const may = lead || (mod && target === "member");
    return { mute: may, kick: may, moveTo: may, moderator: false };
  };
  const others: Array<[string, VoiceRole, Partial<VoicePerson>]> = [
    ["301", lead ? "member" : "leader", { joinedAt: start }],
    ["302", "moderator", { joinedAt: start + 60 }],
    ["701", "member", { joinedAt: start + 600, quality: "weak" }],
    ["702", "member", { joinedAt: start + 900, speaking: true }],
  ];
  return {
    id: `g${CREW}`,
    kind: "crew",
    crew: CREW,
    name: "Genel",
    general: true,
    max: 15,
    phase: "live",
    me: {
      id: ME,
      role,
      mute: false,
      deaf: false,
      mutedByModerator: false,
      talking: false,
      quality: "good",
    },
    people: others.map(([id, r, extra]) => person(id, { role: r, can: powers(r), ...extra })),
    ringing: [],
    canRename: false,
    canClose: false,
  };
}

function crewRooms(full: boolean, inGeneral: boolean): VoiceCrewRooms {
  const general = inGeneral ? [ME, "301", "302", "701", "702"] : ["301", "302", "701", "702"];
  const room = (id: string, name: string, people: string[]) => ({
    id,
    name,
    general: id.startsWith("g"),
    people,
    max: null,
    own: false,
  });
  const rooms = [room(`g${CREW}`, "Genel", general), room("c00000000000000b2", "Oda 2", ["703"])];
  if (full) rooms.push(room("c00000000000000b3", "Taş Ocağı", []));
  return { crew: CREW, rooms, canAdd: !full, manage: false, crewMax: 15 };
}

/**
 * VOICE-ROOMS' previews (`voice=crewRooms`, `crewRoomsIn`, `crewRoomsOld`; `&role=` as for `crew`):
 * the crew arranges its rooms ("Oda yönetimi" on): "Genel" empty, "Taş Ocağı" with two people and
 * a limit of 4, the player's own "Oda 3" empty; `crewRoomsIn` has the player in "Taş Ocağı";
 * `crewRoomsOld` is an older service's list without the empty "Genel" (it must still come first);
 * `crewRoomsBusy` fills them (six in "Genel", "Taş Ocağı" at its 4) and adds a room with a long name.
 */
function keptRooms(scenario: string): VoiceCrewRooms {
  const inside = scenario === "crewRoomsIn";
  const busy = scenario === "crewRoomsBusy";
  const rooms = [
    {
      id: `g${CREW}`,
      name: "Genel",
      general: true,
      people: busy
        ? ["301", "304", "305", "701", "702", "909"]
        : scenario === "crewRoomsOld"
          ? ["301"]
          : [],
      max: null,
      own: false,
    },
    {
      id: "c00000000000000b2",
      name: "Taş Ocağı",
      general: false,
      people: busy ? ["302", "703", "303", "911"] : inside ? ["302", "703", ME] : ["302", "703"],
      max: 4,
      own: false,
    },
    {
      id: "c00000000000000b3",
      name: "Oda 3",
      general: false,
      people: [],
      max: null,
      own: true,
    },
    ...(busy
      ? [
          {
            id: "c00000000000000b4",
            name: "Akşam Turnuvası Takımı",
            general: false,
            people: [],
            max: null,
            own: false,
          },
        ]
      : []),
  ];
  return {
    crew: CREW,
    rooms: scenario === "crewRoomsOld" ? rooms.filter((r) => !r.general || r.people.length) : rooms,
    canAdd: false,
    manage: scenario !== "crewRoomsOld",
    crewMax: 15,
  };
}

function stateFor(scenario: string): VoiceState {
  const base: VoiceState = {
    ...HIDDEN_VOICE_STATE,
    available: true,
    enabled: true,
    mic: "ok",
    devices: {
      inputs: [
        { id: "mic-headset", label: "Kulaklık mikrofonu (HyperX Cloud)" },
        { id: "mic-webcam", label: "Kamera mikrofonu" },
      ],
      outputs: [
        { id: "out-headset", label: "Kulaklık (HyperX Cloud)" },
        { id: "out-speakers", label: "Hoparlör (Realtek)" },
      ],
    },
    profiles: PROFILES,
    presence: PRESENCE,
    crews: [crewRooms(false, false)],
  };
  const withMe = (room: VoiceRoomView, patch: Partial<VoiceRoomView["me"]>): VoiceRoomView => ({
    ...room,
    me: { ...room.me, ...patch },
  });
  switch (scenario) {
    case "off":
      return HIDDEN_VOICE_STATE;
    case "disabled":
      return { ...base, enabled: false };
    case "calling":
    case "noAnswer":
      return {
        ...base,
        room: friendRoom({ people: [], ringing: [] }),
        calling: { friend: "302", until: nowSec() + 28 },
      };
    case "callingBusy":
      return {
        ...base,
        room: friendRoom({ people: [], ringing: [] }),
        calling: { friend: "303", until: nowSec() + 28 },
      };
    case "call1":
      return {
        ...base,
        room: friendRoom({
          people: [person("302", { joinedAt: nowSec() - 252, role: "member" })],
          ringing: [],
        }),
        presence: { ...PRESENCE, "302": "in" },
        togetherSince: nowSec() - 252,
      };
    case "room":
      return {
        ...base,
        room: friendRoom(),
        presence: { ...PRESENCE, "301": "in" },
        togetherSince: nowSec() - 1800,
      };
    case "roomNamed":
      return { ...base, room: friendRoom({ name: "Gece Ekibi" }) };
    case "full":
      return {
        ...base,
        room: friendRoom({
          max: 4,
          ringing: [],
          people: [...friendRoom().people, person("304", { joinedAt: nowSec() - 100 })],
        }),
      };
    case "crew":
    case "crewFull":
    case "restricted":
      return {
        ...base,
        room: crewRoom(crewRole()),
        crews: [crewRooms(scenario === "crewFull", true)],
      };
    case "crewRooms":
    case "crewRoomsOld":
    case "crewRoomsBusy":
      return { ...base, crews: [keptRooms(scenario)] };
    case "crewRoomsIn":
      return {
        ...base,
        room: {
          ...crewRoom(crewRole()),
          id: "c00000000000000b2",
          name: "Taş Ocağı",
          general: false,
          max: 4,
          people: [
            person("302", { role: "moderator", joinedAt: nowSec() - 900 }),
            person("703", { joinedAt: nowSec() - 600, speaking: true }),
          ],
        },
        crews: [keptRooms(scenario)],
      };
    case "ring":
    case "ringLong":
      return {
        ...base,
        rings: [
          {
            room: "f00000000000000c3",
            from: scenario === "ringLong" ? "911" : "301",
            with: ["303", "909"],
            until: nowSec() + 30,
          },
        ],
      };
    case "missed":
      return {
        ...base,
        missed: [
          { room: "f00000000000000d4", from: "302", at: nowSec() - 120, joinable: true },
          { room: "f00000000000000e5", from: "305", at: nowSec() - 3600, joinable: false },
        ],
      };
    case "joining":
      return { ...base, room: { ...friendRoom(), phase: "joining" } };
    case "reconnecting":
      return { ...base, room: { ...friendRoom(), phase: "reconnecting" } };
    case "denied":
    case "missing":
      return {
        ...base,
        mic: scenario,
        room: withMe(friendRoom(), { mute: true }),
      };
    case "weak":
    case "bad":
      return { ...base, room: withMe(friendRoom(), { quality: scenario }) };
    case "modMuted":
      return {
        ...base,
        room: withMe(crewRoom("member"), { mutedByModerator: true }),
        crews: [crewRooms(false, true)],
      };
    case "muted":
      return { ...base, room: withMe(friendRoom(), { mute: true }) };
    case "deaf":
      return { ...base, room: withMe(friendRoom(), { deaf: true }) };
    default:
      return base;
  }
}

/** Who a scenario has speaking (or `speaking=` says). */
function speakingFor(state: VoiceState, override: string | null, talking: boolean): VoiceSpeaking {
  if (override !== null) {
    const ids = override === "none" ? [] : override.split(",").filter(Boolean);
    return { ids: ids.filter((id) => id !== ME), me: talking || ids.includes(ME) };
  }
  const ids = state.room?.people.filter((p) => p.speaking).map((p) => p.id) ?? [];
  return { ids, me: talking };
}

/** The keys the official pack's game uses (crates/pl-mc/data/key-bindings.json, in short). */
const GAME_KEYS = new Set([
  "KeyW",
  "KeyA",
  "KeyS",
  "KeyD",
  "Space",
  "ShiftLeft",
  "ControlLeft",
  "KeyE",
  "KeyQ",
  "KeyF",
  "KeyT",
  "KeyL",
  "Slash",
  "Tab",
  "F1",
  "F2",
  "F3",
  "F5",
  "F11",
  "Escape",
  ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => `Digit${n}`),
]);
const OVERLAY_KEYS = new Set(["F6", "F7", "F9", "KeyC", "KeyX"]);
/** Keys that write while the player types (pl_voice::keys `types`). */
const TYPING_KEYS =
  /^(Key[A-Z]|Digit\d|Space|Enter|Backspace|Minus|Equal|Bracket(Left|Right)|Semicolon|Quote|Backquote|Backslash|Comma|Period|Slash|Intl(Backslash|Ro|Yen))$/;
const MODIFIER_KEYS = /^(Control|Shift|Alt)(Left|Right)$/;

/** As the launcher's `pl_voice::keys::check` (in short). */
function clashOf(
  key: VoiceKey,
  shortcut: VoiceShortcut,
  others: Array<VoiceKey | null>,
): VoiceKeyCheck {
  const mods = key.mods ?? [];
  const sameMods = (a: VoiceKey, b: VoiceKey) =>
    (a.mods ?? []).slice().sort().join() === (b.mods ?? []).slice().sort().join();
  const overlaps = (a: VoiceKey, b: VoiceKey) =>
    (a.kind === "keyboard" && b.kind === "keyboard"
      ? a.code === b.code
      : a.kind === "mouse" && b.kind === "mouse" && a.button === b.button) &&
    (!a.mods?.length || !b.mods?.length || sameMods(a, b));
  if (mods.length && shortcut === "pushToTalk") return { usable: false, clash: null };
  if (key.kind === "keyboard") {
    const { code } = key;
    if (code === "Escape" || code === "MetaLeft" || code === "MetaRight") {
      return { usable: false, clash: null };
    }
    if (mods.length && MODIFIER_KEYS.test(code)) return { usable: false, clash: null };
    if (mods.includes("alt") && (code === "F4" || code === "Tab" || code === "Space")) {
      return { usable: false, clash: null };
    }
  }
  if (others.some((other) => other && overlaps(other, key)))
    return { usable: true, clash: "taken" };
  if (key.kind === "mouse") {
    return { usable: true, clash: key.button === "middle" ? "game" : null };
  }
  const { code } = key;
  if (code === "CapsLock" || code === "KeyV") return { usable: true, clash: "gameVoiceChat" };
  if (OVERLAY_KEYS.has(code)) return { usable: true, clash: "overlay" };
  if (GAME_KEYS.has(code)) return { usable: true, clash: "game" };
  if (code === "AltLeft") return { usable: true, clash: "languageSwitch" };
  if (
    shortcut !== "pushToTalk" &&
    TYPING_KEYS.test(code) &&
    !mods.includes("ctrl") &&
    !mods.includes("alt")
  ) {
    return { usable: true, clash: "typing" };
  }
  return { usable: true, clash: null };
}

export interface VoiceMockOptions {
  scenario?: string;
  /** Who speaks (`speaking=`); null: the scenario's own. */
  speaking?: string | null;
  talking?: boolean;
  /** What "Ara" gets: `answer`, `noAnswer` or a refusal. */
  call?: string | null;
  /** What "Katıl" gets: `ok` or a refusal. */
  join?: string | null;
  /** Timed changes (an answered call, a noAnswer, an invite that joins); off in most tests. */
  timers?: boolean;
  /** Shortcuts already set (`voiceKeys=combo`: push-to-talk on Fare 4, "Ctrl + Shift + M" to
   * mute, N alone to deafen, which brings the typing line). */
  keys?: string | null;
  /** Discord's keys on this computer (`discord=1`: push-to-talk on Fare 4 with Discord in
   * push-to-talk, "Ctrl + Shift + M", "Num *"; `discord=skip`: its deafen is a gamepad button). */
  discord?: string | null;
  /** VOICE-CALL: the live meters as a still frame (`levels=`); none: a moving voice. */
  levels?: string | null;
  /** "Aramadan önce sor" (`ask=0`: off). */
  ask?: boolean;
  /** "Ses seviyesi" (`volume=`). */
  volume?: number | null;
}

/**
 * A `levels=` preview's still frame: "0.7,301:0.85,303:0.4" (the microphone first; "0.5!" heard
 * but not sent).
 */
export function levelsFor(preset: string | null | undefined): VoiceLevels | null {
  if (!preset) return null;
  const [mine = "0", ...rest] = preset.split(",");
  const held = mine.endsWith("!");
  const me = Math.max(0, Math.min(1, Number.parseFloat(held ? mine.slice(0, -1) : mine) || 0));
  const people = rest
    .map((part) => part.split(":"))
    .filter((pair): pair is [string, string] => pair.length === 2 && Boolean(pair[0]))
    .map(([id, level]) => ({ id, level: Math.max(0, Math.min(1, Number.parseFloat(level) || 0)) }));
  return { me, open: me > 0 && !held, people };
}

/** What a `discord=` preview's Discord has. */
function discordFor(preset: string | null | undefined): VoiceDiscordOffer | null {
  if (preset !== "1" && preset !== "skip") return null;
  return {
    pushToTalk: { kind: "mouse", button: "back" },
    mute: { kind: "keyboard", code: "KeyM", label: "M", mods: ["ctrl", "shift"] },
    deafen: preset === "skip" ? null : { kind: "keyboard", code: "NumpadMultiply", label: "*" },
    pushToTalkMode: true,
    skipped: preset === "skip" ? ["deafen"] : [],
  };
}

/** The settings a `voiceKeys=` preview starts with. */
function keysFor(preset: string | null | undefined): Partial<VoiceSettings> {
  if (preset !== "combo") return {};
  return {
    mode: "pushToTalk",
    pushToTalkKey: { kind: "mouse", button: "back" },
    muteKey: { kind: "keyboard", code: "KeyM", label: "M", mods: ["ctrl", "shift"] },
    deafenKey: { kind: "keyboard", code: "KeyN", label: "N" },
  };
}

export interface VoiceMock extends VoiceApi {
  /** Tests: push a notice or a speaking frame as the backend would. */
  emitNotice(notice: VoiceNotice): void;
  emitSpeaking(speaking: VoiceSpeaking): void;
  current(): VoiceState;
}

export function createVoiceMock(options: VoiceMockOptions = {}): VoiceMock {
  const scenario = options.scenario ?? "ready";
  const timers = options.timers ?? true;
  let settings: VoiceSettings = {
    ...DEFAULT_VOICE_SETTINGS,
    enabled: stateFor(scenario).enabled,
    ...keysFor(options.keys),
    askBeforeCall: options.ask ?? true,
    outputVolume: options.volume ?? 100,
  };
  let state: VoiceState = { ...stateFor(scenario), askBeforeCall: settings.askBeforeCall };
  /** Discord's keys until the player answers (then never again, as the launcher keeps it). */
  let discord = discordFor(options.discord);
  const stateListeners = new Set<(s: VoiceState) => void>();
  const speakingListeners = new Set<(s: VoiceSpeaking) => void>();
  const noticeListeners = new Set<(n: VoiceNotice) => void>();
  const levelListeners = new Set<(l: VoiceLevel) => void>();
  const levelsListeners = new Set<(l: VoiceLevels) => void>();
  let speaking = speakingFor(state, options.speaking ?? null, options.talking ?? false);
  let levelTimer: ReturnType<typeof setInterval> | undefined;
  /** The live meters (VOICE-CALL): a still frame, or a voice moving while they are asked for. */
  const stillLevels = levelsFor(options.levels);
  let levelsTimer: ReturnType<typeof setInterval> | undefined;
  /** Who was removed, for the toast's "Geri al". */
  const kicked = new Map<string, VoicePerson | undefined>();

  const later = (ms: number, work: () => void) => {
    if (timers) setTimeout(work, ms);
  };
  const commit = (next: VoiceState) => {
    state = next;
    for (const listener of stateListeners) listener(state);
  };
  const notice = (n: VoiceNotice) => {
    for (const listener of noticeListeners) listener(n);
  };
  const emitSpeaking = (next: VoiceSpeaking) => {
    speaking = next;
    for (const listener of speakingListeners) listener(next);
  };
  const updateRoom = (change: (room: VoiceRoomView) => VoiceRoomView) => {
    if (state.room) commit({ ...state, room: change(state.room) });
  };
  const updatePerson = (id: string, change: Partial<VoicePerson>) =>
    updateRoom((room) => ({
      ...room,
      people: room.people.map((p) => (p.id === id ? { ...p, ...change } : p)),
    }));

  // A scenario that ends right away tells it as the backend would.
  if (scenario === "noAnswer") {
    later(900, () => {
      commit({ ...state, room: null, calling: null });
      notice({ kind: "left", reason: "noAnswer", friend: "302" });
    });
  }
  if (scenario === "failed") {
    later(300, () => notice({ kind: "left", reason: "failed", friend: null }));
  }
  // BAN-TYPES: a voice chat ban takes the player out of the room.
  if (scenario === "restricted") {
    later(1200, () => {
      commit({ ...state, room: null });
      notice({ kind: "left", reason: "restricted", friend: null, until: banEnd() });
    });
  }

  const joinAnswer = (): VoiceAnswer | null => {
    const refusal = refusalOf(options.join);
    return refusal ? refused(refusal) : null;
  };

  return {
    current: () => state,
    emitNotice: notice,
    emitSpeaking,
    async state() {
      return state;
    },
    onState(listener) {
      stateListeners.add(listener);
      return () => stateListeners.delete(listener);
    },
    onSpeaking(listener) {
      speakingListeners.add(listener);
      // The first frame, as the app sends one when the page starts listening.
      if (speaking.ids.length || speaking.me) queueMicrotask(() => listener(speaking));
      return () => speakingListeners.delete(listener);
    },
    onNotice(listener) {
      noticeListeners.add(listener);
      return () => noticeListeners.delete(listener);
    },
    onLevel(listener) {
      levelListeners.add(listener);
      return () => levelListeners.delete(listener);
    },
    onLevels(listener) {
      levelsListeners.add(listener);
      return () => levelsListeners.delete(listener);
    },
    async setLevels(on) {
      clearInterval(levelsTimer);
      levelsTimer = undefined;
      if (!on) return;
      const send = (frame: VoiceLevels) => {
        for (const listener of levelsListeners) listener(frame);
      };
      if (stillLevels) {
        queueMicrotask(() => send(stillLevels));
        return;
      }
      if (!timers) return;
      // Whoever speaks, rising and falling as a voice does, sixteen times a second.
      let t = 0;
      levelsTimer = setInterval(() => {
        t += 1;
        const voice = (k: number) =>
          Math.max(
            0,
            Math.min(1, 0.62 + Math.sin(t / 2.1 + k) * 0.2 + Math.sin(t / 0.8 + k) * 0.08),
          );
        send({
          me: speaking.me ? voice(7) : 0,
          open: speaking.me,
          people: speaking.ids.map((id, i) => ({ id, level: voice(i * 1.7) })),
        });
      }, 60);
    },

    async call(friend) {
      const refusal = refusalOf(options.call);
      if (refusal) return refused(refusal, refusal === "rateLimited" ? 30 : null);
      if (!state.presence?.[friend]) return refused("unreachable");
      commit({
        ...state,
        room: friendRoom({ id: `f${friend.padStart(16, "0")}`, people: [], ringing: [] }),
        calling: { friend, until: nowSec() + 30 },
        togetherSince: null,
      });
      if (options.call === "noAnswer") {
        later(4000, () => {
          commit({ ...state, room: null, calling: null });
          notice({ kind: "left", reason: "noAnswer", friend });
        });
      } else {
        later(2500, () =>
          commit({
            ...state,
            calling: null,
            room: state.room
              ? { ...state.room, people: [person(friend, { joinedAt: nowSec() })] }
              : null,
            togetherSince: nowSec(),
          }),
        );
      }
      return VOICE_OK;
    },
    async answer(room) {
      const no = joinAnswer();
      if (no) return no;
      const ring = state.rings.find((r) => r.room === room);
      const missed = state.missed.find((m) => m.room === room);
      const from = ring?.from ?? missed?.from;
      if (!from) return refused("roomGone");
      const people = [from, ...(ring?.with ?? [])].map((id, i) =>
        person(id, { joinedAt: nowSec() - 300 + i, role: i === 0 ? "leader" : "member" }),
      );
      commit({
        ...state,
        rings: state.rings.filter((r) => r.room !== room),
        missed: state.missed.filter((m) => m.room !== room),
        calling: null,
        room: friendRoom({
          id: room,
          people,
          ringing: [],
          me: { ...friendRoom().me, role: "member" },
          canRename: false,
          canClose: false,
        }),
        togetherSince: nowSec(),
      });
      return VOICE_OK;
    },
    async decline(room) {
      commit({ ...state, rings: state.rings.filter((r) => r.room !== room) });
      return VOICE_OK;
    },
    async setIgnored(ids) {
      // As the launcher does: an ignored person's ring goes quietly into the missed list.
      const quiet = state.rings.filter((r) => ids.includes(r.from));
      if (!quiet.length) return;
      commit({
        ...state,
        rings: state.rings.filter((r) => !ids.includes(r.from)),
        missed: [
          ...quiet.map((r) => ({ room: r.room, from: r.from, at: nowSec(), joinable: true })),
          ...state.missed.filter((m) => !quiet.some((r) => r.room === m.room)),
        ],
      });
    },
    async dismissMissed(room) {
      commit({ ...state, missed: state.missed.filter((m) => m.room !== room) });
    },
    async joinCrew(crew, room) {
      const no = joinAnswer();
      if (no) return no;
      const rooms = state.crews.find((c) => c.crew === crew);
      const target = rooms?.rooms.find((r) => (room ? r.id === room : r.general));
      const view = crewRoom(crewRole());
      const people = (target?.people ?? []).filter((id) => id !== ME);
      commit({
        ...state,
        calling: null,
        room: {
          ...view,
          id: target?.id ?? view.id,
          name: target?.name ?? view.name,
          general: target?.general ?? true,
          people: view.people
            .filter((p) => people.includes(p.id))
            .concat(
              people
                .filter((id) => !view.people.some((p) => p.id === id))
                .map((id) =>
                  person(id, { can: { mute: true, kick: true, moveTo: true, moderator: false } }),
                ),
            ),
        },
        crews: state.crews.map((c) =>
          c.crew === crew
            ? {
                ...c,
                rooms: c.rooms.map((r) => ({
                  ...r,
                  people:
                    r.id === (target?.id ?? `g${crew}`)
                      ? [...r.people.filter((id) => id !== ME), ME]
                      : r.people.filter((id) => id !== ME),
                })),
              }
            : c,
        ),
      });
      return VOICE_OK;
    },
    async newCrewRoom(crew) {
      const rooms = state.crews.find((c) => c.crew === crew);
      if (rooms && !rooms.canAdd) return refused("roomsFull");
      const id = `c${String(Date.now()).padStart(16, "0").slice(-16)}`;
      const name = `Oda ${(rooms?.rooms.length ?? 1) + 1}`;
      const view = crewRoom(crewRole());
      commit({
        ...state,
        room: { ...view, id, name, general: false, people: [], canRename: true, canClose: false },
        crews: state.crews.map((c) =>
          c.crew === crew
            ? {
                ...c,
                rooms: [
                  ...c.rooms.map((r) => ({ ...r, people: r.people.filter((p) => p !== ME) })),
                  { id, name, general: false, people: [ME], max: null, own: c.manage },
                ],
                canAdd: c.rooms.length + 1 < state.crewRoomsMax,
              }
            : c,
        ),
      });
      return VOICE_OK;
    },
    // VOICE-ROOMS: the crew page's changes, as the service would make them (the role is the
    // page's to show; `voice=crewRooms&change=<refusal>` refuses).
    async crewChange(op) {
      const no = refusalOf(param("change"));
      if (no) return refused(no);
      const entry = state.crews.find((c) => c.crew === op.crew);
      if (!entry?.manage) return refused("off");
      const general = entry.rooms.find((r) => r.general);
      const rest = entry.rooms.filter((r) => !r.general);
      let rooms = entry.rooms;
      let room = state.room;
      switch (op.op) {
        case "edit":
          rooms = entry.rooms.map((r) =>
            r.id === op.room ? { ...r, name: op.name.trim(), max: op.max } : r,
          );
          if (room?.id === op.room) room = { ...room, name: op.name.trim(), max: op.max ?? 15 };
          break;
        case "order": {
          const sorted = op.rooms
            .map((id) => rest.find((r) => r.id === id))
            .filter((r): r is (typeof rest)[number] => r !== undefined);
          rooms = [
            ...(general ? [general] : []),
            ...sorted,
            ...rest.filter((r) => !sorted.includes(r)),
          ];
          break;
        }
        case "remove": {
          const gone = rest.find((r) => r.id === op.room);
          if (!gone) return refused("roomGone");
          if (op.empty && gone.people.length) return refused("nothing");
          rooms = entry.rooms
            .filter((r) => r.id !== op.room)
            .map((r) => (r.general ? { ...r, people: [...r.people, ...gone.people] } : r));
          if (room?.id === op.room && general) room = { ...room, id: general.id, name: "Genel" };
          break;
        }
        case "move":
          rooms = entry.rooms.map((r) => ({
            ...r,
            people:
              r.id === op.room
                ? [...r.people.filter((id) => id !== op.person), op.person]
                : r.people.filter((id) => id !== op.person),
          }));
          break;
      }
      commit({
        ...state,
        room,
        crews: state.crews.map((c) =>
          c.crew === op.crew
            ? { ...c, rooms, canAdd: rooms.length < state.crewRoomsMax && c.manage }
            : c,
        ),
      });
      return VOICE_OK;
    },
    async leave() {
      commit({
        ...state,
        room: null,
        calling: null,
        togetherSince: null,
        crews: state.crews.map((c) => ({
          ...c,
          rooms: c.rooms.map((r) => ({ ...r, people: r.people.filter((id) => id !== ME) })),
        })),
      });
    },
    async retry() {
      commit({ ...state, room: friendRoom() });
      return VOICE_OK;
    },
    async setMute(mute) {
      // Opening the microphone brings the sound back too (the launcher's rule, VOICE-L1).
      updateRoom((room) => ({
        ...room,
        me: { ...room.me, mute, deaf: mute ? room.me.deaf : false },
      }));
    },
    async setDeaf(deaf) {
      // Deafening closes the microphone too (the gate, not the choice): undeafening leaves it as
      // it was before (the launcher's rule, VOICE-L1).
      updateRoom((room) => ({ ...room, me: { ...room.me, deaf } }));
    },
    async invite(friend) {
      const room = state.room;
      if (!room) return refused("nothing");
      if (room.people.length + 1 + room.ringing.length >= room.max) return refused("roomFull");
      updateRoom((r) => ({ ...r, ringing: [...r.ringing, { to: friend, until: nowSec() + 30 }] }));
      later(3000, () =>
        updateRoom((r) =>
          r.ringing.some((x) => x.to === friend)
            ? {
                ...r,
                ringing: r.ringing.filter((x) => x.to !== friend),
                people: [...r.people, person(friend, { joinedAt: nowSec() })],
              }
            : r,
        ),
      );
      return VOICE_OK;
    },
    async cancelInvite(friend) {
      updateRoom((r) => ({ ...r, ringing: r.ringing.filter((x) => x.to !== friend) }));
      return VOICE_OK;
    },
    async setVolume(id, volume) {
      updatePerson(id, { volume: Math.max(0, Math.min(200, Math.round(volume))) });
    },
    async setMutedForMe(id, muted) {
      updatePerson(id, { mutedForMe: muted });
    },
    async moderate(action: VoiceModAction) {
      const room = state.room;
      if (!room) return refused("nothing");
      switch (action.act) {
        case "mute":
        case "unmute":
          updatePerson(action.person, { mutedByModerator: action.act === "mute" });
          return VOICE_OK;
        case "kick":
          updateRoom((r) => ({ ...r, people: r.people.filter((p) => p.id !== action.person) }));
          kicked.set(
            action.person,
            room.people.find((p) => p.id === action.person),
          );
          return VOICE_OK;
        case "unkick": {
          const back = kicked.get(action.person);
          kicked.delete(action.person);
          if (back) updateRoom((r) => ({ ...r, people: [...r.people, back] }));
          return VOICE_OK;
        }
        case "move":
          updateRoom((r) => ({ ...r, people: r.people.filter((p) => p.id !== action.person) }));
          commit({
            ...state,
            crews: state.crews.map((c) => ({
              ...c,
              rooms: c.rooms.map((r) => ({
                ...r,
                people:
                  r.id === action.room
                    ? [...r.people, action.person]
                    : r.people.filter((id) => id !== action.person),
              })),
            })),
          });
          return VOICE_OK;
        case "mod":
        case "unmod":
          updatePerson(action.person, { role: action.act === "mod" ? "moderator" : "member" });
          return VOICE_OK;
        case "rename": {
          const name = action.name.trim();
          if (name.length < 3 || name.length > 24) return refused("invalidName");
          if (/salak|aptal/i.test(name)) return refused("nameRejected");
          updateRoom((r) => ({ ...r, name }));
          return VOICE_OK;
        }
        case "close":
          commit({ ...state, room: null, calling: null });
          return VOICE_OK;
      }
    },

    async settings() {
      return settings;
    },
    async saveSettings(next) {
      settings = { ...next };
      if (state.enabled !== next.enabled || state.askBeforeCall !== next.askBeforeCall) {
        commit({ ...state, enabled: next.enabled, askBeforeCall: next.askBeforeCall });
      }
      return settings;
    },
    async checkKey(key: VoiceKey, shortcut: VoiceShortcut) {
      const others = [
        shortcut === "pushToTalk" ? null : settings.pushToTalkKey,
        shortcut === "mute" ? null : settings.muteKey,
        shortcut === "deafen" ? null : settings.deafenKey,
      ];
      return clashOf(key, shortcut, others);
    },
    async discordOffer() {
      return discord;
    },
    async discordAnswer(copy) {
      const offer = discord;
      discord = null;
      if (copy && offer) {
        settings = {
          ...settings,
          ...(offer.pushToTalk ? { pushToTalkKey: offer.pushToTalk } : {}),
          ...(offer.pushToTalk && offer.pushToTalkMode ? { mode: "pushToTalk" as const } : {}),
          ...(offer.mute ? { muteKey: offer.mute } : {}),
          ...(offer.deafen ? { deafenKey: offer.deafen } : {}),
        };
      }
      return settings;
    },
    async testMic(on) {
      clearInterval(levelTimer);
      commit({ ...state, testing: on });
      if (!on || !timers) return;
      // A voice that rises and falls, as a meter would show someone speaking.
      let t = 0;
      levelTimer = setInterval(() => {
        t += 1;
        const level = Math.max(0, Math.sin(t / 3) * 0.55 + Math.sin(t / 1.3) * 0.2);
        const threshold = (settings.sensitivity ?? 18) / 100;
        for (const listener of levelListeners) listener({ level, open: level > threshold });
      }, 80);
    },
    async settingsOpen() {},
    async openMicSettings() {},
    async backup() {
      // As the launcher's: the settings without this computer's devices, the people's choices.
      const people: Record<string, { volume: number; muted: boolean }> = {};
      for (const p of state.room?.people ?? []) {
        if (p.volume !== 100 || p.mutedForMe)
          people[p.id] = { volume: p.volume, muted: p.mutedForMe };
      }
      return { settings: { ...settings, inputDevice: null, outputDevice: null }, people };
    },
    async restore(backup) {
      const saved = backup.settings;
      if (saved && typeof saved === "object") {
        settings = { ...settings, ...(saved as Partial<VoiceSettings>) };
        commit({ ...state, enabled: settings.enabled, backupRev: (state.backupRev ?? 0) + 1 });
      }
    },
  };
}

/** The preview's voice chat, from the page URL. */
export const mockVoice: VoiceMock = createVoiceMock({
  scenario: param("voice") ?? "ready",
  speaking: param("speaking"),
  talking: param("talking") === "1",
  call: param("call"),
  join: param("join"),
  keys: param("voiceKeys"),
  discord: param("discord"),
  levels: param("levels"),
  ask: param("ask") !== "0",
  volume: param("volume") ? Number(param("volume")) : null,
});
