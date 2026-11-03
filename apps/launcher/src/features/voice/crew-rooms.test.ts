// VOICE-ROOMS: the crew page's room rules: "Genel" first, who may do what to which room, moving
// crewmates, the order's steps, the edit dialog's limits and name, the pending order, the menu.
import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import type { VoiceCrewRoom } from "@/lib/api/voice-types";
import i18n from "@/lib/i18n";
import { roomMenuItems } from "./crew-room-menu";
import { withPendingOrder } from "./crew-rooms-store";
import {
  type CrewStanding,
  crewRoomFull,
  crewRoomList,
  limitChoices,
  mayMovePerson,
  moveRoom,
  roomNameFits,
  roomPowers,
} from "./rules";

const t = i18n.getFixedT("tr") as TFunction;
const CREW = "0123456789abcdef";
const GENERAL = `g${CREW}`;

const room = (id: string, extra: Partial<VoiceCrewRoom> = {}): VoiceCrewRoom => ({
  id,
  name: id.startsWith("g") ? "Genel" : "Oda 2",
  general: id.startsWith("g"),
  people: [],
  max: null,
  own: false,
  ...extra,
});

const leader: CrewStanding = { role: "founder", moderator: false };
const moderator: CrewStanding = { role: "member", moderator: true };
const member: CrewStanding = { role: "member", moderator: false };

describe("the crew's room list", () => {
  it("puts Genel first, also when an older service left an empty one out", () => {
    const b = room("c00000000000000bb");
    const a = room("c00000000000000aa");
    expect(crewRoomList(CREW, { rooms: [b, a] }).map((r) => r.id)).toEqual([GENERAL, b.id, a.id]);
    expect(crewRoomList(CREW, { rooms: [b, room(GENERAL, { people: ["7"] })] })[0]).toMatchObject({
      id: GENERAL,
      people: ["7"],
    });
    // No entry at all: "Genel" alone, empty.
    expect(crewRoomList(CREW, undefined)).toEqual([room(GENERAL)]);
    // A room being deleted (waiting for "Geri al") is not listed.
    expect(crewRoomList(CREW, { rooms: [room(GENERAL), b, a] }, [b.id]).map((r) => r.id)).toEqual([
      GENERAL,
      a.id,
    ]);
  });

  it("keeps the order the player gave until the service says it", () => {
    const rooms = [room("c1"), room("c2"), room("c3")];
    expect(withPendingOrder(rooms, ["c3", "c1"]).map((r) => r.id)).toEqual(["c3", "c1", "c2"]);
    expect(withPendingOrder(rooms, ["c9", "c2"]).map((r) => r.id)).toEqual(["c2", "c1", "c3"]);
    expect(withPendingOrder(rooms, undefined).map((r) => r.id)).toEqual(["c1", "c2", "c3"]);
  });
});

describe("who may do what", () => {
  it("lets the leader and moderators change, order and delete; an opener their own room", () => {
    const kept = room("c00000000000000aa");
    const all = { edit: true, order: true, remove: true };
    expect(roomPowers(leader, kept, true)).toEqual(all);
    expect(roomPowers(moderator, kept, true)).toEqual(all);
    expect(roomPowers(member, kept, true)).toEqual({ edit: false, order: false, remove: false });
    expect(roomPowers(member, { ...kept, own: true }, true)).toEqual({
      edit: true,
      order: false,
      remove: true,
    });
    // "Genel" never; nothing at all while the service does not let crews arrange rooms.
    expect(roomPowers(leader, room(GENERAL), true)).toEqual({
      edit: false,
      order: false,
      remove: false,
    });
    expect(roomPowers(leader, kept, false)).toEqual({ edit: false, order: false, remove: false });
  });

  it("moves crewmates by the room's rule: leaders anyone, moderators members only", () => {
    const them = (role: "founder" | "member", isMod = false, me = false) => ({
      role,
      moderator: isMod,
      me,
    });
    expect(mayMovePerson(leader, them("member", true), true)).toBe(true);
    expect(mayMovePerson(leader, them("founder", false, true), true)).toBe(false);
    expect(mayMovePerson(moderator, them("member"), true)).toBe(true);
    expect(mayMovePerson(moderator, them("member", true), true)).toBe(false);
    expect(mayMovePerson(moderator, them("founder"), true)).toBe(false);
    expect(mayMovePerson(member, them("member"), true)).toBe(false);
    expect(mayMovePerson(leader, them("member"), false)).toBe(false);
    expect(mayMovePerson(leader, undefined, true)).toBe(false);
  });
});

describe("the edit dialog and the order", () => {
  it("steps a room up and down, never past an end", () => {
    expect(moveRoom(["a", "b", "c"], "b", -1)).toEqual(["b", "a", "c"]);
    expect(moveRoom(["a", "b", "c"], "b", 1)).toEqual(["a", "c", "b"]);
    expect(moveRoom(["a", "b"], "a", -1)).toBeNull();
    expect(moveRoom(["a", "b"], "b", 1)).toBeNull();
    expect(moveRoom(["a"], "z", 1)).toBeNull();
  });

  it("offers limits from 2 to the crew's maximum, and checks a name's length", () => {
    expect(limitChoices(15)).toEqual([null, ...Array.from({ length: 14 }, (_, i) => i + 2)]);
    expect(limitChoices(2)).toEqual([null, 2]);
    expect(crewRoomFull(room("c1", { people: ["1", "2", "3", "4"], max: 4 }), 15)).toBe(true);
    expect(crewRoomFull(room("c1", { people: ["1", "2", "3"], max: 4 }), 15)).toBe(false);
    expect(crewRoomFull(room("c1", { people: ["1", "2"], max: null }), 2)).toBe(true);
    expect(roomNameFits("ab")).toBe(false);
    expect(roomNameFits("  Taş  ")).toBe(true);
    expect(roomNameFits("x".repeat(24))).toBe(true);
    expect(roomNameFits("x".repeat(25))).toBe(false);
  });
});

describe("the room's menu", () => {
  const words = (items: ReturnType<typeof roomMenuItems>) =>
    items.map((item) => (item === null ? "—" : `${item.label}${item.disabled ? " (off)" : ""}`));
  const noop = () => {};

  it("lists the room's own action first, then by role, and deleting last", () => {
    const kept = room("c00000000000000aa");
    const menu = roomMenuItems(
      {
        crew: CREW,
        room: kept,
        here: false,
        powers: roomPowers(leader, kept, true),
        order: [kept.id, "c00000000000000bb"],
        onEdit: noop,
        onRemove: noop,
      },
      t,
    );
    expect(words(menu)).toEqual([
      "Katıl",
      "—",
      "Odayı düzenle",
      "Yukarı taşı (off)",
      "Aşağı taşı",
      "—",
      "Odayı sil",
    ]);
    expect(menu[6]?.danger).toBe(true);
    expect(menu[4]?.shortcut).toBe("Alt ↓");
  });

  it("gives Genel and a crewmate's room only their own action; the opener edit and delete", () => {
    const general = room(GENERAL);
    expect(
      words(
        roomMenuItems(
          {
            crew: CREW,
            room: general,
            here: true,
            powers: roomPowers(leader, general, true),
            order: [],
            onEdit: noop,
            onRemove: noop,
          },
          t,
        ),
      ),
    ).toEqual(["Ayrıl"]);
    const mine = room("c00000000000000aa", { own: true });
    expect(
      words(
        roomMenuItems(
          {
            crew: CREW,
            room: mine,
            here: false,
            powers: roomPowers(member, mine, true),
            order: [mine.id],
            onEdit: noop,
            onRemove: noop,
          },
          t,
        ),
      ),
    ).toEqual(["Katıl", "—", "Odayı düzenle", "—", "Odayı sil"]);
  });
});
