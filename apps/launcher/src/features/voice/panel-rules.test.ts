import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import "@/lib/i18n";
import type { VoicePerson } from "@/lib/api/voice-types";
import i18n from "@/lib/i18n";
import {
  closeInMenu,
  connectionWords,
  fitNames,
  headerLine,
  namesLines,
  panelTitle,
  renameBesideTitle,
  renameInMenu,
  titleNames,
} from "./panel-rules";

const t = i18n.getFixedT("tr") as TFunction;

const names = ["KorAtesi", "x7Ender", "MaviTilki", "Zumrut42"];
/** A line `width` letters long. */
const upTo = (width: number) => (line: string) => line.length <= width;

describe("an unnamed room's names on a short line", () => {
  it("says fewer names and how many more, then the first name, never stopping inside one", () => {
    expect(namesLines(names, t)).toEqual([
      "KorAtesi, x7Ender ve 2 kişi daha",
      "KorAtesi ve 3 kişi daha",
      "KorAtesi…",
    ]);
    expect(namesLines(["KorAtesi", "x7Ender", "MaviTilki"], t)).toEqual([
      "KorAtesi, x7Ender, MaviTilki",
      "KorAtesi, x7Ender ve 1 kişi daha",
      "KorAtesi ve 2 kişi daha",
      "KorAtesi…",
    ]);
    expect(namesLines(["KorAtesi"], t)).toEqual(["KorAtesi"]);
    expect(namesLines([], t)).toEqual([]);
  });

  it("takes the longest line that fits, the shortest when none does", () => {
    expect(fitNames(names, upTo(40), t)).toBe("KorAtesi, x7Ender ve 2 kişi daha");
    expect(fitNames(names, upTo(30), t)).toBe("KorAtesi ve 3 kişi daha");
    expect(fitNames(names, upTo(12), t)).toBe("KorAtesi…");
    expect(fitNames(names, upTo(4), t)).toBe("KorAtesi…");
    expect(fitNames(["KorAtesi"], upTo(4), t)).toBe("KorAtesi");
  });

  it("fits only a title made of who is in the room", () => {
    const people = [{ id: "a" }, { id: "b" }] as VoicePerson[];
    const nameOf = (id: string) => (id === "a" ? "KorAtesi" : "x7Ender");
    expect(titleNames({ name: null, general: false, people }, nameOf)).toEqual([
      "KorAtesi",
      "x7Ender",
    ]);
    expect(titleNames({ name: "Taş Ocağı", general: false, people }, nameOf)).toBeNull();
    expect(titleNames({ name: "Genel", general: true, people }, nameOf)).toBeNull();
    expect(titleNames({ name: null, general: false, people: [] }, nameOf)).toBeNull();
  });
});

describe("a crew room's header", () => {
  const people = [{ id: "a" }] as VoicePerson[];
  const nameOf = () => "KorAtesi";
  const crewRoom = { kind: "crew" as const, name: "Genel", general: true, people };

  it("is titled by the crew, its chips saying which room", () => {
    expect(panelTitle(crewRoom, "Ateş Ekibi", nameOf, t)).toBe("Ateş Ekibi");
    // The crew not known yet: the room's own name.
    expect(panelTitle(crewRoom, undefined, nameOf, t)).toBe("Genel");
    expect(
      panelTitle({ ...crewRoom, kind: "friend", general: false, name: null }, "X", nameOf, t),
    ).toBe("KorAtesi");
  });

  it("does not count the people again under the title, only says a full room", () => {
    const quiet = { stage: false, counted: false, full: false };
    expect(headerLine(crewRoom, quiet, t)).toBeNull();
    expect(headerLine(crewRoom, { ...quiet, full: true }, t)).toBe("Oda dolu.");
    expect(headerLine({ kind: "friend", people }, quiet, t)).toBe("2 kişi");
    expect(headerLine({ kind: "friend", people }, { ...quiet, counted: true }, t)).toBeNull();
    expect(headerLine({ kind: "friend", people }, { ...quiet, stage: true }, t)).toBe(
      t("voice.call.kind"),
    );
  });

  it('renames a crew\'s room from the menu, never "Genel", and a friend room beside its title', () => {
    expect(renameInMenu({ canRename: true, kind: "crew", general: false })).toBe(true);
    expect(renameInMenu({ canRename: true, kind: "crew", general: true })).toBe(false);
    expect(renameInMenu({ canRename: false, kind: "crew", general: false })).toBe(false);
    expect(renameInMenu({ canRename: true, kind: "friend", general: false })).toBe(false);
    const three = [{ id: "a" }, { id: "b" }] as VoicePerson[];
    expect(renameBesideTitle({ canRename: true, kind: "crew", people: three })).toBe(false);
    expect(renameBesideTitle({ canRename: true, kind: "friend", people: three })).toBe(true);
    expect(renameBesideTitle({ canRename: false, kind: "friend", people: three })).toBe(false);
  });

  it("shows no pencil in a call of two: a name means something from three people on", () => {
    const two = [{ id: "a" }] as VoicePerson[];
    expect(renameBesideTitle({ canRename: true, kind: "friend", people: two })).toBe(false);
    expect(renameBesideTitle({ canRename: true, kind: "friend", people: [] })).toBe(false);
  });
});

describe("a connection's bars", () => {
  it("say what they show in words", () => {
    expect(connectionWords("good", t)).toBe("Bağlantı: İyi");
    expect(connectionWords("weak", t)).toBe("Bağlantı: Orta");
    expect(connectionWords("bad", t)).toBe("Bağlantı: Zayıf");
  });
});

describe("the header's room menu", () => {
  const people = [{ id: "a" }] as VoicePerson[];

  it("offers closing the room to whoever may, with others in it and not in a call of two", () => {
    expect(closeInMenu({ canClose: true, people }, false)).toBe(true);
    expect(closeInMenu({ canClose: false, people }, false)).toBe(false);
    // Alone, closing is leaving.
    expect(closeInMenu({ canClose: true, people: [] }, false)).toBe(false);
    // A call of two: leaving is hanging up.
    expect(closeInMenu({ canClose: true, people }, true)).toBe(false);
  });
});
