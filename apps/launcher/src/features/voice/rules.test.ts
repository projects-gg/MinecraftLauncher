import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import "@/lib/i18n";
import type { Friend, SocialTeam, TeamMember } from "@/lib/api/social-types";
import type { VoicePerson, VoiceRoomView, VoiceState } from "@/lib/api/voice-types";
import { HIDDEN_VOICE_STATE } from "@/lib/api/voice-types";
import i18n from "@/lib/i18n";
import {
  callOffer,
  clashText,
  codeLabel,
  crewRoomName,
  faceOf,
  invitables,
  keyFromEvent,
  keyFromMouse,
  keyLabel,
  leftText,
  namesLine,
  peopleChanges,
  presenceOf,
  refusalText,
  roomFull,
  roomTitle,
  snapVolume,
  toneNow,
} from "./rules";

const t = i18n.getFixedT("tr") as TFunction;

const friend = (id: string, name: string): Friend => ({
  id,
  name,
  username: name.toLowerCase(),
  avatar: "preset:fox",
  state: "launcher",
  dnd: false,
  discordUrl: "",
});

const member = (id: string, name: string, extra: Partial<TeamMember> = {}): TeamMember => ({
  id,
  name,
  username: name.toLowerCase(),
  avatar: "preset:wolf",
  role: "member",
  moderator: false,
  me: false,
  friend: false,
  blocked: false,
  state: "launcher",
  dnd: false,
  discordUrl: "",
  joinedAt: 0,
  ...extra,
});

const crew = (id: string, members: TeamMember[]): SocialTeam =>
  ({ id, name: "Gece Kartalları", members }) as SocialTeam;

const lists = {
  friends: [friend("301", "GeceKurdu_"), friend("302", "KorAtesi"), friend("305", "NetherKral")],
  crews: [
    crew("c1", [
      member("100", "EjderAvcisi", { me: true }),
      member("301", "GeceKurdu_", { friend: true }),
      member("701", "MaviElmas"),
      member("702", "DemirGolem_", { blocked: true }),
    ]),
  ],
};

const person = (id: string, extra: Partial<VoicePerson> = {}): VoicePerson => ({
  id,
  role: "member",
  mute: false,
  deaf: false,
  mutedByModerator: false,
  quality: "good",
  speaking: false,
  volume: 100,
  mutedForMe: false,
  joinedAt: 0,
  can: { mute: false, kick: false, moveTo: false, moderator: false },
  ...extra,
});

const room = (extra: Partial<VoiceRoomView> = {}): VoiceRoomView => ({
  id: "f1",
  kind: "friend",
  crew: null,
  name: null,
  general: false,
  max: 10,
  phase: "live",
  me: {
    id: "100",
    role: "leader",
    mute: false,
    deaf: false,
    mutedByModerator: false,
    talking: false,
    quality: "good",
  },
  people: [],
  ringing: [],
  canRename: true,
  canClose: true,
  ...extra,
});

const on = (extra: Partial<VoiceState> = {}): VoiceState => ({
  ...HIDDEN_VOICE_STATE,
  available: true,
  enabled: true,
  presence: { "301": "ready", "302": "ready", "305": "in", "701": "ready", "702": "ready" },
  ...extra,
});

const crewMembers = (id: string) =>
  lists.crews.find((c) => c.id === id)?.members.map((m) => m.id) ?? [];

describe("faces", () => {
  it("names a friend, a crewmate, a profile, else someone (never an id)", () => {
    const state = { profiles: [{ id: "909", name: "KumFirtinasi", avatar: "preset:wolf" }] };
    expect(faceOf("301", lists, state, "Bir oyuncu")).toMatchObject({
      name: "GeceKurdu_",
      friend: true,
    });
    expect(faceOf("701", lists, state, "Bir oyuncu")).toMatchObject({
      name: "MaviElmas",
      friend: false,
    });
    expect(faceOf("909", lists, state, "Bir oyuncu").name).toBe("KumFirtinasi");
    expect(faceOf("123", lists, state, "Bir oyuncu")).toEqual({
      id: "123",
      name: "Bir oyuncu",
      friend: false,
    });
  });
});

describe("room titles", () => {
  const names: Record<string, string> = { a: "Kaan_07", b: "Ece_x", c: "Mert", d: "Zeyno" };
  const nameOf = (id: string) => names[id] ?? "?";

  it("says the name, the crew's Genel in the player's words, or who is in it", () => {
    expect(roomTitle(room({ name: "Gece Ekibi" }), nameOf, t)).toBe("Gece Ekibi");
    expect(roomTitle(room({ kind: "crew", name: "Genel", general: true }), nameOf, t)).toBe(
      "Genel",
    );
    expect(roomTitle(room({ people: [person("a"), person("b")] }), nameOf, t)).toBe(
      "Kaan_07, Ece_x",
    );
    expect(
      roomTitle(room({ people: ["a", "b", "c", "d"].map((id) => person(id)) }), nameOf, t),
    ).toBe("Kaan_07, Ece_x ve 2 kişi daha");
  });

  it("names the friend being called while the player waits, else the player alone", () => {
    expect(roomTitle(room(), nameOf, t, "a")).toBe("Kaan_07");
    expect(roomTitle(room(), nameOf, t)).toBe("Yalnız sen");
  });

  it("keeps lines of three names whole", () => {
    expect(namesLine(["A", "B", "C"], t)).toBe("A, B, C");
    expect(crewRoomName({ name: "Oda 2", general: false }, t)).toBe("Oda 2");
  });
});

describe("what a card offers", () => {
  it("offers nothing while voice chat is off or the friend cannot be called", () => {
    expect(callOffer(HIDDEN_VOICE_STATE, "301", true, crewMembers)).toBeNull();
    expect(callOffer(on({ enabled: false }), "301", true, crewMembers)).toBeNull();
    expect(callOffer(on(), "303", true, crewMembers)).toBeNull();
  });

  it("calls a friend from no room, never someone who is not a friend", () => {
    expect(callOffer(on(), "301", true, crewMembers)).toBe("call");
    expect(callOffer(on(), "701", false, crewMembers)).toBeNull();
  });

  it("invites into the player's friend room; rings and a full room show it off", () => {
    expect(callOffer(on({ room: room() }), "302", true, crewMembers)).toBe("invite");
    expect(callOffer(on({ room: room() }), "701", false, crewMembers)).toBeNull();
    expect(
      callOffer(
        on({ room: room({ ringing: [{ to: "302", until: 0 }] }) }),
        "302",
        true,
        crewMembers,
      ),
    ).toBe("ringing");
    expect(callOffer(on({ calling: { friend: "302", until: 0 } }), "302", true, crewMembers)).toBe(
      "ringing",
    );
    expect(callOffer(on({ room: room({ max: 1 }) }), "302", true, crewMembers)).toBe("full");
    expect(
      callOffer(on({ room: room({ people: [person("302")] }) }), "302", true, crewMembers),
    ).toBeNull();
  });

  it("in a crew room invites its crewmates and calls other friends (leaving the room)", () => {
    const crewRoom = room({ kind: "crew", crew: "c1", name: "Genel", general: true });
    expect(callOffer(on({ room: crewRoom }), "701", false, crewMembers)).toBe("invite");
    expect(callOffer(on({ room: crewRoom }), "301", true, crewMembers)).toBe("invite");
    expect(callOffer(on({ room: crewRoom }), "302", true, crewMembers)).toBe("call");
  });

  it("counts the player and the invites still ringing in a room's places", () => {
    expect(roomFull({ people: [person("a")], ringing: [{ to: "b", until: 0 }], max: 3 })).toBe(
      true,
    );
    expect(roomFull({ people: [person("a")], ringing: [], max: 3 })).toBe(false);
  });
});

describe("Davet et", () => {
  it("lists the player's friends who may be called, never someone in the room", () => {
    const state = on({
      room: room({ people: [person("302")], ringing: [{ to: "305", until: 0 }] }),
    });
    const list = invitables(state, lists, "");
    expect(list.map((p) => [p.name, p.ringing])).toEqual([
      ["GeceKurdu_", false],
      ["NetherKral", true],
    ]);
  });

  it("lists the crew's members in a crew room (not blocked ones, not the player)", () => {
    const state = on({ room: room({ kind: "crew", crew: "c1", general: true }) });
    expect(invitables(state, lists, "").map((p) => p.name)).toEqual(["GeceKurdu_", "MaviElmas"]);
  });

  it("filters by what is typed, Turkish letters folded", () => {
    const state = on({ room: room() });
    expect(invitables(state, lists, "nether").map((p) => p.name)).toEqual(["NetherKral"]);
    expect(invitables(state, lists, "zzz")).toEqual([]);
    expect(invitables(on(), lists, "")).toEqual([]);
  });
});

describe("VOICE-INT: what the launcher now tells", () => {
  // As the launcher sends it: a friend's own voice chat, nothing for crewmates out of a room.
  const told = (extra: Partial<VoiceState> = {}) => on({ presence: { "301": "ready" }, ...extra });
  const crewRoom = room({ kind: "crew", crew: "c1", name: "Genel", general: true });
  const online = (id: string) =>
    lists.crews.some((c) => c.members.some((m) => m.id === id && m.state !== "offline"));

  it("names a friend's friend by the room's card, with their badges", () => {
    const state = {
      profiles: [
        {
          id: "909",
          name: "KumFirtinasi",
          avatar: "discord",
          avatarUrl: null,
          marks: ["verified"],
        },
      ],
    };
    expect(faceOf("909", lists, state, "Bir oyuncu")).toEqual({
      id: "909",
      name: "KumFirtinasi",
      avatar: "discord",
      marks: ["verified"],
      friend: false,
    });
  });

  it("invites an online crewmate who is no friend into the player's room of that crew", () => {
    const offer = (state: VoiceState, isOnline: (id: string) => boolean) =>
      callOffer(state, "701", false, crewMembers, lists.friends, isOnline);
    expect(offer(told({ room: crewRoom }), online)).toBe("invite");
    expect(offer(told({ room: crewRoom }), () => false)).toBeNull();
    expect(offer(told(), online)).toBeNull();
    expect(offer(told({ room: room() }), online)).toBeNull();
    // A friend's own voice chat decides for them (off: no invite).
    expect(callOffer(told({ room: crewRoom }), "302", true, crewMembers, [], online)).toBeNull();
  });

  it("lists online crewmates who are no friends in a crew room's Davet et", () => {
    const state = told({ room: crewRoom });
    expect(invitables(state, lists, "").map((p) => p.name)).toEqual(["GeceKurdu_", "MaviElmas"]);
    const away = {
      ...lists,
      crews: [
        crew("c1", [
          member("100", "EjderAvcisi", { me: true }),
          member("701", "MaviElmas", { state: "offline" }),
        ]),
      ],
    };
    expect(invitables(state, away, "")).toEqual([]);
  });
});

describe("words", () => {
  it("says a refusal, nothing when it went through", () => {
    const answer = (refused: string | null, retryAfter: number | null = null) =>
      ({ refused, retryAfter, until: null }) as Parameters<typeof refusalText>[0];
    expect(refusalText(answer(null), t)).toBeNull();
    expect(refusalText(answer("nothing"), t)).toBeNull();
    expect(refusalText(answer("roomFull"), t)).toBe("Oda dolu.");
    expect(refusalText(answer("unreachable"), t, "KorAtesi")).toBe("KorAtesi şu an aranamıyor.");
    expect(refusalText(answer("rateLimited", 30), t)).toBe("Biraz bekleyip tekrar dene.");
    expect(refusalText(answer("rateLimited", 7200), t)).toBe("Bugünlük arama hakkın doldu.");
    expect(refusalText(answer("accountLimited", 60), t)).toBe("Bugünlük arama hakkın doldu.");
    expect(refusalText(answer("off"), t)).toBe("Sesli sohbet şu an kullanılamıyor.");
  });

  it("says why a stay ended", () => {
    expect(leftText("alone", t)).toBe("Odada kimse kalmadı.");
    expect(leftText("idle", t)).toBe("Uzun süre hareketsiz kaldığın için ayrıldın.");
    expect(leftText("noAnswer", t)).toBe("Cevap yok");
  });
});

describe("shortcut keys", () => {
  it("labels keys by their cap, mouse buttons by number", () => {
    expect(keyLabel({ kind: "keyboard", code: "CapsLock", label: "" }, t)).toBe("Caps Lock");
    expect(keyLabel({ kind: "keyboard", code: "KeyV", label: "V" }, t)).toBe("V");
    expect(keyLabel({ kind: "keyboard", code: "KeyQ", label: "" }, t)).toBe("Q");
    expect(keyLabel({ kind: "mouse", button: "back" }, t)).toBe("Fare 4");
    expect(codeLabel("Numpad1")).toBe("Num 1");
    expect(codeLabel("F8")).toBe("F8");
  });

  it("takes the layout's own letter, upper-cased in the interface's language", () => {
    expect(keyFromEvent({ code: "Quote", key: "i" }, "tr")).toEqual({
      kind: "keyboard",
      code: "Quote",
      label: "İ",
    });
    expect(keyFromEvent({ code: "Space", key: " " }, "tr")).toEqual({
      kind: "keyboard",
      code: "Space",
      label: "Space",
    });
    expect(keyFromEvent({ code: "Escape", key: "Escape" }, "tr")).toBeNull();
    expect(keyFromEvent({ code: "MetaLeft", key: "Meta" }, "tr")).toBeNull();
  });

  it("takes the wheel and the side buttons, never the left or right one", () => {
    expect(keyFromMouse(0)).toBeNull();
    expect(keyFromMouse(2)).toBeNull();
    expect(keyFromMouse(1)).toEqual({ kind: "mouse", button: "middle" });
    expect(keyFromMouse(3)).toEqual({ kind: "mouse", button: "back" });
    expect(keyFromMouse(4)).toEqual({ kind: "mouse", button: "forward" });
  });

  it("says one line about a clash", () => {
    expect(clashText({ usable: true, clash: null }, t)).toBeNull();
    expect(clashText({ usable: true, clash: "gameVoiceChat" }, t)).toBe(
      "Bu tuşu oyundaki sesli sohbet de kullanıyor.",
    );
    expect(clashText({ usable: false, clash: null }, t)).toBe("Bu tuş seçilemez.");
  });
});

describe("presence and tones", () => {
  it("reads a friend's voice chat from the friend list first", () => {
    const friends = [{ ...friend("306", "KizilEjder"), voice: "in" as const }];
    expect(presenceOf("306", on(), friends)).toBe("in");
    expect(presenceOf("301", on(), friends)).toBe("ready");
    expect(presenceOf("999", on(), friends)).toBeUndefined();
    expect(callOffer(on(), "306", true, crewMembers, friends)).toBe("call");
  });

  it("rings for a call, keeps the caller company, and is quiet away or off", () => {
    const ring = { room: "f2", from: "301", with: [], until: 0 };
    expect(toneNow(on({ rings: [ring], calling: { friend: "302", until: 0 } }), false)).toBe(
      "ring",
    );
    expect(toneNow(on({ calling: { friend: "302", until: 0 } }), false)).toBe("ringback");
    expect(toneNow(on({ rings: [ring] }), true)).toBeNull();
    expect(toneNow({ ...on({ rings: [ring] }), enabled: false }, false)).toBeNull();
    expect(toneNow(on(), false)).toBeNull();
  });
});

describe("volume and changes", () => {
  it("lands a drag on 100 % near the notch and keeps 0–200", () => {
    expect(snapVolume(97)).toBe(100);
    expect(snapVolume(104)).toBe(100);
    expect(snapVolume(106)).toBe(106);
    expect(snapVolume(-5)).toBe(0);
    expect(snapVolume(250)).toBe(200);
  });

  it("says who came and who left the same room only", () => {
    const before = { id: "r", people: [person("a"), person("b")] };
    const after = { id: "r", people: [person("b"), person("c")] };
    expect(peopleChanges(before, after)).toEqual({ joined: ["c"], left: ["a"] });
    expect(peopleChanges(before, { ...after, id: "other" })).toEqual({ joined: [], left: [] });
    expect(peopleChanges(null, after)).toEqual({ joined: [], left: [] });
  });
});
