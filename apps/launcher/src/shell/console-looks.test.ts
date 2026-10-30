// CONSOLE-COLOR: a console line's parts and events as the view checks them again (the contract:
// the Studio repository's services/cloud-afk README, "Console looks").
import {
  hoverEntityLines,
  hoverTextLines,
  lineRuns,
  safeClick,
  safeEvents,
  safeHover,
  safeUrl,
} from "@pl/ui/console";
import { describe, expect, it } from "vitest";
import { readableColor } from "../../../../packages/ui/src/components/inventory/text";

const joined = (runs: { text: string }[] | null) => runs?.map((run) => run.text).join("");

describe("a line's parts", () => {
  it("shows the game's colours and flags, the words exactly the line's", () => {
    const text = "[Parti]  FrostyOtter88: lobiye   geliyorum";
    const runs = lineRuns(text, [
      [9, "9l"],
      [13, "b"],
      [20, ""],
    ]);
    expect(joined(runs)).toBe(text);
    // The game's blue reads too dark on the console: lifted in its own hue.
    expect(runs?.[0]).toMatchObject({
      text: "[Parti]  ",
      color: readableColor("#5555ff"),
      bold: true,
    });
    expect(readableColor("#5555ff")).not.toBe("#5555ff");
    expect(runs?.[1]).toMatchObject({ text: "FrostyOtter88", color: "#55ffff" });
    // No colour of its own: the line's (its kind's tone).
    expect(runs?.[2]).toEqual({ text: ": lobiye   geliyorum" });
  });

  it("lifts a colour too dark to read, in its own hue, and keeps obfuscated words still", () => {
    const runs = lineRuns("[Klan] gizli", [
      [7, "1"],
      [5, "#120000k"],
    ]);
    expect(runs?.[0]?.color).not.toBe("#0000aa");
    expect(runs?.[0]?.color).toMatch(/^#[0-9a-f]{6}$/);
    expect(runs?.[1]).toMatchObject({ text: "gizli" });
    expect(runs?.[1]).not.toHaveProperty("obfuscated");
  });

  it("joins neighbours of one look and one event", () => {
    const events = safeEvents([{ click: { a: "copy_to_clipboard", v: "kod" } }]);
    const runs = lineRuns(
      "aabbcc",
      [
        [2, "a"],
        [2, "a"],
        [1, "a", 0],
        [1, "a", 0],
      ],
      events,
    );
    expect(runs).toEqual([
      { text: "aabb", color: "#55ff55" },
      { text: "cc", color: "#55ff55", event: 0 },
    ]);
  });

  it("keeps none that are not the contract's: the line shows as today", () => {
    const text = "selam";
    expect(lineRuns(text, undefined)).toBeNull();
    expect(lineRuns(text, [])).toBeNull();
    expect(lineRuns(text, [[4, "a"]])).toBeNull();
    expect(lineRuns(text, [[6, "a"]])).toBeNull();
    expect(lineRuns(text, [[5, "z"]])).toBeNull();
    expect(lineRuns(text, [[5, "<b>"]])).toBeNull();
    expect(lineRuns(text, [[5, "red"]])).toBeNull();
    expect(
      lineRuns(text, [
        [0, "a"],
        [5, "a"],
      ]),
    ).toBeNull();
    expect(
      lineRuns(text, [
        [2.5, "a"],
        [2.5, "a"],
      ]),
    ).toBeNull();
    expect(lineRuns(text, [[5, "a", 0, 1]])).toBeNull();
    expect(lineRuns(text, '[[5,"a"]]')).toBeNull();
    expect(
      lineRuns(
        "x".repeat(65),
        Array.from({ length: 65 }, () => [1, "a"]),
      ),
    ).toBeNull();
    expect(
      lineRuns(
        "x".repeat(64),
        Array.from({ length: 64 }, () => [1, "a"]),
      ),
    ).not.toBeNull();
    // Never between a character's halves.
    expect(
      lineRuns("a😀b", [
        [2, "a"],
        [2, "b"],
      ]),
    ).toBeNull();
    expect(
      joined(
        lineRuns("a😀b", [
          [3, "a"],
          [1, "b"],
        ]),
      ),
    ).toBe("a😀b");
  });

  it("gives a part its event only when the event is the contract's", () => {
    const events = safeEvents([
      { click: { a: "run_command", v: "say hi" } },
      { click: { a: "run_command", v: "/party accept FrostyOtter88" } },
    ]);
    expect(events[0]).toBeNull();
    const runs = lineRuns(
      "ab",
      [
        [1, "a", 0],
        [1, "b", 1],
      ],
      events,
    );
    expect(runs?.[0]).not.toHaveProperty("event");
    expect(runs?.[1]?.event).toBe(1);
    // An index past the events, or none of them at all.
    expect(lineRuns("ab", [[2, "a", 5]], events)?.[0]).not.toHaveProperty("event");
    expect(lineRuns("ab", [[2, "a", 0]])?.[0]).not.toHaveProperty("event");
  });
});

describe("a part's click", () => {
  it("is one of four kinds, never cut, never hiding what it is", () => {
    expect(safeClick({ a: "run_command", v: "/party accept FrostyOtter88" })).toEqual({
      a: "run_command",
      v: "/party accept FrostyOtter88",
    });
    expect(safeClick({ a: "suggest_command", v: "/msg Mod_Kirpi " })?.a).toBe("suggest_command");
    expect(safeClick({ a: "suggest_command", v: "merhaba" })?.v).toBe("merhaba");
    expect(safeClick({ a: "copy_to_clipboard", v: "KOD-123" })?.v).toBe("KOD-123");
    expect(safeClick({ a: "open_url", v: "https://projects.gg/kurallar" })?.a).toBe("open_url");
    for (const odd of [
      { a: "run_command", v: "party accept" },
      { a: "run_command", v: "/" },
      { a: "run_command", v: `/${"x".repeat(256)}` },
      { a: "run_command", v: "/pay LavaPanda_7 1000\u202e" },
      { a: "run_command", v: "/a\nb" },
      { a: "copy_to_clipboard", v: "" },
      { a: "copy_to_clipboard", v: "a\u200fb" },
      { a: "change_page", v: "2" },
      { a: "show_dialog", v: "minecraft:x" },
      { a: "custom", v: "x" },
      { a: "open_file", v: "C:/x" },
      { a: "open_url", v: "http://projects.gg" },
      { a: "open_url", v: "javascript:alert(1)" },
      { a: "open_url", v: "https://user:pass@projects.gg" },
      { a: "open_url", v: "https://projects.gg/a b" },
      { a: "open_url", v: "HTTPS://projects.gg" },
      { a: "run_command" },
      "run_command",
      null,
    ]) {
      expect(safeClick(odd), JSON.stringify(odd)).toBeNull();
    }
  });

  it("opens only https addresses with a host", () => {
    expect(safeUrl("https://forum.projects.gg/t/1")).toBe(true);
    expect(safeUrl("https://")).toBe(false);
    expect(safeUrl("https:/projects.gg")).toBe(false);
  });
});

describe("a part's hover", () => {
  it("keeps the server's text in its looks, line by line, white without a colour", () => {
    const hover = safeHover({
      text: "Partiye katıl\nDavet 60 saniye",
      st: [
        [14, "a"],
        [15, ""],
      ],
    });
    expect(hover).not.toBeNull();
    const lines = hoverTextLines(hover as { text: string });
    expect(lines.map((line) => line.text)).toEqual(["Partiye katıl", "Davet 60 saniye"]);
    expect(lines[0]?.runs?.[0]).toMatchObject({ text: "Partiye katıl", color: "#55ff55" });
    expect(lines[1]?.runs?.[0]).toMatchObject({ text: "Davet 60 saniye", color: "#ffffff" });
    // A blank line between is the game's spacer.
    const spaced = hoverTextLines(safeHover({ text: "a\n\nb" }) as { text: string });
    expect(spaced.map((line) => line.text)).toEqual(["a", "", "b"]);
  });

  it("drops parts that are not the contract's, and a text that is not", () => {
    expect(safeHover({ text: "abc", st: [[2, "a"]] })).toEqual({ text: "abc", st: null });
    expect(safeHover({ text: "abc", st: [[3, "a", 0]] })).toEqual({
      text: "abc",
      st: [[3, "a", 0]],
    });
    expect(safeHover({ text: "x".repeat(257) })).toBeNull();
    expect(safeHover({ text: Array.from({ length: 9 }, () => "a").join("\n") })).toBeNull();
    expect(safeHover({ text: "a\u202eb" })).toBeNull();
    expect(safeHover({ text: "" })).toBeNull();
    expect(safeHover("metin")).toBeNull();
  });

  it("shows an entity's name and its type as the game does", () => {
    const hover = safeHover({
      entity: { label: { tr: "Zombi", en: "Zombie" }, name: "Kemik_Kral", st: [[10, "c"]] },
    });
    expect(hover).not.toBeNull();
    const lines = hoverEntityLines(hover as never, "tr", "Tür");
    expect(lines.map((line) => line.text)).toEqual(["Kemik_Kral", "Tür: Zombi"]);
    expect(lines[0]?.runs?.[0]?.color).toBe("#ff5555");
    expect(hoverEntityLines(hover as never, "en", "Type")[1]?.text).toBe("Type: Zombie");
    expect(safeHover({ entity: { label: { tr: "a\u0000" } } })).toBeNull();
    expect(safeHover({ entity: {} })).toBeNull();
  });

  it("passes an item on for the host to draw", () => {
    expect(safeHover({ item: { id: "minecraft:diamond_sword", count: 1 } })).toEqual({
      item: { id: "minecraft:diamond_sword", count: 1 },
    });
    expect(safeHover({ item: "minecraft:diamond_sword" })).toBeNull();
  });
});

describe("a line's events", () => {
  it("holds at most eight, each in its place", () => {
    expect(
      safeEvents(Array.from({ length: 9 }, () => ({ click: { a: "copy_to_clipboard", v: "x" } }))),
    ).toEqual([]);
    const events = safeEvents([
      { click: { a: "open_url", v: "javascript:x" } },
      { hover: { text: "ipucu" } },
      { click: { a: "copy_to_clipboard", v: "x" }, hover: { text: "" } },
      "x",
    ]);
    expect(events).toEqual([
      null,
      { hover: { text: "ipucu", st: null } },
      { click: { a: "copy_to_clipboard", v: "x" } },
      null,
    ]);
    expect(safeEvents(undefined)).toEqual([]);
    expect(safeEvents({ 0: {} })).toEqual([]);
  });
});
