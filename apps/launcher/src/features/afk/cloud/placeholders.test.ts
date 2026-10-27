import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  captureCount,
  insertToken,
  type PlaceholderRow,
  placeholderChips,
  placeholderProblem,
} from "./placeholders";
import { looksSensitive } from "./sensitive";
import { textProblem } from "./settings-model";

interface Template {
  name: string;
  kind: "reply" | "command" | "console";
  text: string | null;
  secret?: boolean;
  rule?: { match: string; regex: boolean; from: string };
  problem: { reason: string; part?: number } | null;
  placeholderCommands?: string[];
  captures?: number;
}

const vectors = JSON.parse(
  readFileSync(
    new URL(
      "../../../../../../crates/pl-cloud-afk/tests/vectors/rule-placeholder-v1.json",
      import.meta.url,
    ),
    "utf8",
  ),
) as {
  placeholderCommands: string[];
  templates: Template[];
  captures: Array<{ pattern: string; count: number }>;
};

describe("rule placeholders (the contract's shared cases)", () => {
  it("reads every template as the save does", () => {
    let checked = 0;
    for (const t of vectors.templates) {
      if (t.text === null) continue;
      const commands = t.placeholderCommands ?? vectors.placeholderCommands;
      if (t.problem?.reason === "secret") {
        // A password command typed plain: the launcher seals it before it is sent.
        expect(looksSensitive(t.text), t.name).toBe(true);
        checked++;
        continue;
      }
      const text = textProblem(t.text);
      let found: { reason: string; part?: number } | null;
      if (text) found = { reason: text };
      else if (t.secret) found = null;
      else {
        const row: PlaceholderRow =
          t.kind === "reply" && t.rule
            ? { kind: "reply", text: t.text, rule: t.rule }
            : { kind: t.kind === "command" ? "command" : "console", text: t.text };
        found = placeholderProblem(row, commands);
      }
      expect(found, t.name).toEqual(t.problem);
      if (typeof t.captures === "number" && t.rule) {
        expect(captureCount(t.rule.match), t.name).toBe(t.captures);
      }
      checked++;
    }
    expect(checked).toBeGreaterThanOrEqual(120);
  });

  it("counts every pattern's groups as Rust numbers them", () => {
    for (const { pattern, count } of vectors.captures) {
      expect(captureCount(pattern), pattern).toBe(count);
    }
  });
});

describe("placeholder chips and typing", () => {
  it("offers the sender and the pattern's parts", () => {
    expect(placeholderChips({ pattern: "selam", regex: false, from: "player" })).toEqual(["u"]);
    expect(
      placeholderChips({ pattern: "^!davet (\\w+) (\\d+)$", regex: true, from: "any" }),
    ).toEqual(["u", "1", "2"]);
    expect(placeholderChips({ pattern: "^Ödül: (.+)$", regex: true, from: "server" })).toEqual([
      "1",
    ]);
  });

  it("puts a token where the caret is, apart from the words around it", () => {
    expect(insertToken("Selam", 5, 5, "{u}")).toEqual({ text: "Selam {u}", caret: 9 });
    expect(insertToken("Selam, AFK'yım", 5, 5, "{u}")).toEqual({
      text: "Selam {u}, AFK'yım",
      caret: 9,
    });
    expect(insertToken("", 0, 0, "{1}")).toEqual({ text: "{1}", caret: 3 });
    expect(insertToken("/msg  selam", 5, 5, "{u}")).toEqual({ text: "/msg {u} selam", caret: 8 });
    expect(insertToken("abc", 0, 3, "{u}")).toEqual({ text: "{u}", caret: 3 });
  });
});
