import { describe, expect, it } from "vitest";
import { messageMenu } from "../menus";
import { reasonKey, reasonsFor } from "../report";
import {
  cleanNote,
  lengthOf,
  NOTE_MAX,
  nextNoteEnd,
  noteLength,
  noteProblem,
  noteShows,
  noteUntil,
} from "./notes";
import { canPinIn, nextShown, PIN_WINDOW_MS, pinChoice, pinnable } from "./pins";

// 3 Nov 2026, 15:20 local time.
const NOW = new Date(2026, 10, 3, 15, 20, 0).getTime();

describe("status notes (SOC-NOTE §3.5)", () => {
  it("go as one cleaned line of at most 40 characters, never a link or a phone number", () => {
    expect(cleanNote("  Sınavım   var,\nakşam gelirim ")).toBe("Sınavım var, akşam gelirim");
    expect(noteLength("🎮".repeat(NOTE_MAX))).toBe(NOTE_MAX);
    expect(noteProblem("a".repeat(NOTE_MAX))).toBeNull();
    expect(noteProblem("a".repeat(NOTE_MAX + 1))).toBe("long");
    expect(noteProblem("discord.gg/abc gel")).toBe("link");
    expect(noteProblem("0532 123 45 67")).toBe("phone");
    expect(noteProblem("   ")).toBeNull();
    // Our own server's address may be written.
    expect(noteProblem("play.projects.gg'deyim")).toBeNull();
  });

  it("last an hour, until the end of the day, or until taken off", () => {
    expect(noteUntil("hour", NOW)).toBe(Math.floor(NOW / 1000) + 3_600);
    expect(noteUntil("today", NOW)).toBe(new Date(2026, 10, 4, 0, 0, 0).getTime() / 1000);
    expect(noteUntil("keep", NOW)).toBe(0);
    expect(lengthOf(undefined, NOW)).toBe("today");
    expect(lengthOf({ text: "x", until: 0 }, NOW)).toBe("keep");
    expect(lengthOf({ text: "x", until: noteUntil("hour", NOW) }, NOW)).toBe("hour");
    expect(lengthOf({ text: "x", until: noteUntil("today", NOW) }, NOW)).toBe("today");
  });

  it("show until their end, and the clock waits for the soonest one", () => {
    const hour = { text: "x", until: Math.floor(NOW / 1000) + 3_600 };
    expect(noteShows(hour, NOW)).toBe(true);
    expect(noteShows(hour, NOW + 3_600_000)).toBe(false);
    expect(noteShows({ text: "x", until: 0 }, NOW + 1e12)).toBe(true);
    expect(noteShows(undefined, NOW)).toBe(false);
    const later = { text: "y", until: Math.floor(NOW / 1000) + 7_200 };
    expect(nextNoteEnd([later, undefined, { text: "z", until: 0 }, hour], NOW)).toBe(
      hour.until * 1000,
    );
    expect(nextNoteEnd([{ text: "z", until: 0 }], NOW)).toBeNull();
  });

  it("take 'Uygunsuz durum notu' in a friend's report, beside the other 'Uygunsuz' reasons", () => {
    const target = { kind: "player" as const, id: "305", note: true };
    expect(reasonsFor(target)).toEqual([
      "name",
      "profile",
      "inappropriate",
      "harassment",
      "spam",
      "scam",
      "other",
    ]);
    expect(reasonsFor({ kind: "player", id: "305" })).not.toContain("inappropriate");
    expect(reasonKey("player", "inappropriate", false, false, true)).toBe("socNote.report.reason");
    expect(reasonKey("player", "inappropriate", true)).toBe("fix2.report.noteReason");
  });
});

describe("pinned messages (SOC-NOTE §3.8)", () => {
  const message = {
    key: `i:${NOW - 60_000}`,
    id: NOW - 60_000,
    from: "302",
    mine: false,
    text: "Buluşma 21:00",
    at: NOW - 60_000,
    edited: false,
    state: "sent" as const,
  };

  it("are the leader's and the moderators' to pin, text messages of the week", () => {
    expect(canPinIn({ role: "founder", moderator: false })).toBe(true);
    expect(canPinIn({ role: "member", moderator: true })).toBe(true);
    expect(canPinIn({ role: "member", moderator: false })).toBe(false);
    expect(canPinIn(undefined)).toBe(false);
    expect(pinnable(message, NOW)).toBe(true);
    expect(pinnable({ ...message, sticker: "pl1/gg", text: "" }, NOW)).toBe(false);
    expect(pinnable({ ...message, state: "sending" }, NOW)).toBe(false);
    expect(pinnable({ ...message, at: NOW - PIN_WINDOW_MS - 1 }, NOW)).toBe(false);
    const pin = { id: message.id, from: "302", text: "Buluşma 21:00", pinnedAt: NOW };
    expect(pinChoice(message, [], true, NOW)).toBe("pin");
    expect(pinChoice(message, [pin], true, NOW)).toBe("unpin");
    expect(pinChoice(message, [pin], false, NOW)).toBeNull();
  });

  it("go into the message menu beside copying, away from deleting", () => {
    const plan = messageMenu({
      reactable: false,
      repliable: true,
      copyable: true,
      link: false,
      editable: true,
      deletable: true,
      report: false,
      pin: "pin",
    });
    expect(plan.map((entry) => entry?.action ?? null)).toEqual([
      "reply",
      null,
      "copy",
      "pin",
      null,
      "edit",
      "delete",
    ]);
  });

  it("step back one by one in the line over the chat, the newest after the oldest", () => {
    expect(nextShown(2, 3)).toBe(1);
    expect(nextShown(1, 3)).toBe(0);
    expect(nextShown(0, 3)).toBe(2);
    expect(nextShown(0, 1)).toBe(0);
  });
});
