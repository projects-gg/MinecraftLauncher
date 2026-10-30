// @vitest-environment happy-dom
// "ProBot AI ile kalıp kur" in a 7/24 Bulut rule: offered only while the service's model can answer;
// the box asks with the player's words, shows the sentence and examples, "Kullan" puts the trigger
// in the rule and the field then reads as that sentence; "Kalıbı kendim yazacağım" waits in the box;
// a refusal shows the command's own words.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { CommandError } from "@/lib/api/tauri";
import { TriggerField } from "../settings/controls";
import type { PatternSuggestion } from "./api";
import { saveSentence, savedSentence, useRuleAssist } from "./rule-assist";

const MADE: PatternSuggestion = {
  regex: true,
  match: "(?i)\\b(tp|[ıi][şs][ıi]nla)\\b",
  summary: 'Mesajda "tp" ya da "ışınla" geçerse yanıt verir.',
  matches: ["tp atar mısın", "beni ışınla"],
  misses: ["tpa geldim"],
  source: "ai",
};

type Suggest = (text: string, fresh: boolean) => Promise<PatternSuggestion>;

function Rule({
  available,
  suggest,
  start = { pattern: "", regex: false },
}: {
  available: boolean;
  suggest: Suggest;
  start?: { pattern: string; regex: boolean };
}) {
  const [rule, setRule] = useState(start);
  const { assist, box } = useRuleAssist({
    accountId: "a_TEST1",
    ruleId: "r1",
    fieldId: "rule-match",
    pattern: rule.pattern,
    regex: rule.regex,
    available,
    suggest,
    onChange: (pattern, regex) => setRule({ pattern, regex }),
  });
  return (
    <div>
      <TriggerField
        id="rule-match"
        value={rule.pattern}
        regex={rule.regex}
        onChange={(pattern, regex) => setRule({ pattern, regex })}
        placeholder="selam"
        error={null}
        maxLength={256}
        assist={assist}
      />
      {box}
      <output data-testid="rule">{`${rule.regex ? "regex" : "plain"}:${rule.pattern}`}</output>
    </div>
  );
}

const never: Suggest = () => new Promise(() => {});

beforeEach(() => localStorage.clear());
afterEach(() => cleanup());

describe("ProBot AI in a cloud rule's trigger", () => {
  it("is not offered while the service's model cannot answer", () => {
    render(<Rule available={false} suggest={never} />);
    expect(screen.queryByText("ProBot AI ile kalıp kur")).toBeNull();
    expect(screen.getByText("Kalıbı kendim yazacağım")).toBeTruthy();
  });

  it("asks with the player's words and puts the pattern it made in the rule", async () => {
    const suggest = vi.fn<Suggest>(async () => MADE);
    render(<Rule available suggest={suggest} />);
    // With ProBot AI, writing a pattern by hand waits in its box.
    expect(screen.queryByText("Kalıbı kendim yazacağım")).toBeNull();
    fireEvent.click(screen.getByText("ProBot AI ile kalıp kur"));
    const words = screen.getByLabelText("Hangi mesajlara yanıt versin? Kendi sözlerinle yaz.");
    fireEvent.change(words, { target: { value: "  biri tp  ya da ışınla yazınca " } });
    fireEvent.click(screen.getByText("Oluştur"));
    expect(suggest).toHaveBeenCalledWith("biri tp ya da ışınla yazınca", false);
    expect(await screen.findByText(MADE.summary)).toBeTruthy();
    expect(screen.getByText("beni ışınla")).toBeTruthy();
    expect(screen.getByText("tpa geldim")).toBeTruthy();

    fireEvent.click(screen.getByText("Tekrar dene"));
    await waitFor(() => expect(suggest).toHaveBeenLastCalledWith("biri tp ya da ışınla yazınca", true));

    fireEvent.click(await screen.findByText("Kullan"));
    expect(screen.getByTestId("rule").textContent).toBe(`regex:${MADE.match}`);
    // The field reads as the sentence, kept on this computer for this rule's pattern.
    expect(screen.getByText("Ne zaman yanıt verir?")).toBeTruthy();
    expect(screen.getAllByText(MADE.summary).length).toBeGreaterThan(0);
    expect(savedSentence("a_TEST1", "r1", MADE.match)).toBe(MADE.summary);
    expect(screen.getByText("Değiştir")).toBeTruthy();
    expect(screen.getByText("Düz söze dön")).toBeTruthy();
  });

  it("offers no new answer for a plain phrase, which needs no model", async () => {
    const plain: PatternSuggestion = {
      regex: false,
      match: "selam",
      summary: 'Mesajda "selam" geçerse yanıt verir.',
      matches: ["selam", "Selam"],
      misses: ["merhaba"],
      source: "plain",
    };
    render(<Rule available suggest={async () => plain} />);
    fireEvent.click(screen.getByText("ProBot AI ile kalıp kur"));
    fireEvent.change(
      screen.getByLabelText("Hangi mesajlara yanıt versin? Kendi sözlerinle yaz."),
      { target: { value: "selam" } },
    );
    fireEvent.click(screen.getByText("Oluştur"));
    expect(await screen.findByText(plain.summary)).toBeTruthy();
    expect(screen.queryByText("Tekrar dene")).toBeNull();
    fireEvent.click(screen.getByText("Kullan"));
    expect(screen.getByTestId("rule").textContent).toBe("plain:selam");
    expect(screen.getByText("Mesajda geçen söz")).toBeTruthy();
  });

  it("shows a refusal in the command's own words", async () => {
    const unclear = new CommandError({
      code: "BULUT-KALIP",
      message: "ProBot AI bundan bir kalıp çıkaramadı. Başka sözlerle anlatmayı dene.",
      details: "HTTP 422 pattern_failed",
    });
    render(
      <Rule
        available
        suggest={async () => {
          throw unclear;
        }}
      />,
    );
    fireEvent.click(screen.getByText("ProBot AI ile kalıp kur"));
    fireEvent.change(
      screen.getByLabelText("Hangi mesajlara yanıt versin? Kendi sözlerinle yaz."),
      { target: { value: "bir şeyler yazınca" } },
    );
    fireEvent.click(screen.getByText("Oluştur"));
    expect((await screen.findByRole("alert")).textContent).toBe(unclear.message);
    expect(screen.getByTestId("rule").textContent).toBe("plain:");
  });

  it("keeps a too long description on this side", () => {
    const suggest = vi.fn<Suggest>(never);
    render(<Rule available suggest={suggest} />);
    fireEvent.click(screen.getByText("ProBot AI ile kalıp kur"));
    fireEvent.change(
      screen.getByLabelText("Hangi mesajlara yanıt versin? Kendi sözlerinle yaz."),
      { target: { value: "ş".repeat(201) } },
    );
    fireEvent.click(screen.getByText("Oluştur"));
    expect(screen.getByRole("alert").textContent).toBe("En çok 200 karakter yazabilirsin.");
    expect(suggest).not.toHaveBeenCalled();
  });

  it("lets the player write the pattern by hand from the box", () => {
    render(<Rule available suggest={never} start={{ pattern: "tp", regex: false }} />);
    fireEvent.click(screen.getByText("ProBot AI ile kalıp kur"));
    fireEvent.click(screen.getByText("Kalıbı kendim yazacağım"));
    expect(screen.getByTestId("rule").textContent).toBe("regex:tp");
    expect(screen.getByLabelText("Özel kalıp")).toBeTruthy();
  });

  it("reads a pattern it made earlier on this computer as its sentence", () => {
    render(<Rule available suggest={never} start={{ pattern: MADE.match, regex: true }} />);
    // Not one it made here: the pattern shows as itself.
    expect(screen.getByLabelText("Özel kalıp")).toBeTruthy();
    cleanup();
    saveSentence("a_TEST1", "r1", MADE.match, MADE.summary);
    render(<Rule available suggest={never} start={{ pattern: MADE.match, regex: true }} />);
    expect(screen.getByText("Ne zaman yanıt verir?")).toBeTruthy();
    expect(screen.getByText(MADE.summary)).toBeTruthy();
    // Another rule, or another pattern, is not that one.
    expect(savedSentence("a_TEST1", "r2", MADE.match)).toBeNull();
    expect(savedSentence("a_TEST1", "r1", "(?i)tp")).toBeNull();
  });
});
