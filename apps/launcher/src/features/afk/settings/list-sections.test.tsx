// @vitest-environment happy-dom
// "ProBot AI ile Kalıp Kur" in a desktop rule (PATTERN-L): offered only while the service has it for
// this player, the same box as a 7/24 Bulut rule's; only the player's words are asked about, "Bu
// kalıbı kullan" puts the trigger in the draft's rule, and the sentence is kept for the account.
import { TooltipProvider } from "@pl/ui";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import type { AfkSettingsView, SettingsPage } from "@/lib/api";
import type { PatternSuggestion } from "../cloud/api";
import { savedSentence } from "../cloud/rule-assist";
import { RepliesSection } from "./list-sections";
import { type Draft, toDraft } from "./model";
import type { RulePatterns } from "./sections";

const MADE: PatternSuggestion = {
  regex: true,
  match: "(?i)\\b(tp|[ıi][şs][ıi]nla)\\b",
  summary: 'Mesajda "tp" ya da "ışınla" geçerse yanıt verir.',
  matches: ["tp atar mısın", "beni ışınla"],
  misses: ["tpa geldim"],
  source: "ai",
};

const page = (): SettingsPage => ({
  serverHost: "play.projects.gg",
  serverPort: 0,
  minecraftVersion: "26.3",
  autoStart: false,
  allowResourcePacks: false,
  loginCommands: { enabled: false, onlyFirstLogin: false, delaySeconds: 2, commands: [] },
  scheduler: { enabled: false, tasks: [] },
  antiAfk: {
    enabled: true,
    delayMin: 60,
    delayMax: 90,
    command: "",
    useSneak: true,
    useTerrainHandling: false,
    walkRange: 5,
    walkRetries: 20,
  },
  autoRelog: {
    enabled: true,
    delayMin: 5,
    delayMax: 10,
    retries: -1,
    ignoreKickMessage: true,
    kickMessages: [],
  },
  autoEat: { enabled: false, threshold: 6 },
  autoRespond: {
    enabled: true,
    soundEnabled: false,
    matchColors: false,
    rules: [
      {
        pattern: "",
        reply: "Şu an AFK'yım",
        privateReply: "",
        otherReply: "",
        ownersOnly: false,
        cooldownSeconds: 30,
        isRegex: false,
      },
    ],
  },
  chatLog: { enabled: false, addDateTime: true, showDate: false, filter: "messages" },
  alerts: {
    enabled: false,
    beepEnabled: false,
    triggerByWords: true,
    logToFile: true,
    matches: [],
    excludes: [],
  },
});

function Section({ patterns }: { patterns?: RulePatterns }) {
  const [draft, setDraft] = useState<Draft>(() => toDraft(page()));
  const rule = draft.rules[0];
  // The rule's delete button has a tooltip: the app gives every screen its provider (main.tsx).
  return (
    <TooltipProvider>
      <RepliesSection
        draft={draft}
        setDraft={(update) => setDraft(update)}
        errorFor={() => null}
        view={{} as AfkSettingsView}
        onOpenFolder={() => {}}
        revision={0}
        patterns={patterns}
      />
      <output data-testid="rule">{`${rule?.isRegex ? "regex" : "plain"}:${rule?.pattern}`}</output>
    </TooltipProvider>
  );
}

// The box fades: happy-dom leaves a cancelled animation's `finished` rejection unhandled (a browser
// marks it handled), as in rule-assist.test.tsx.
const animate = Element.prototype.animate;
beforeAll(() => {
  Element.prototype.animate = function handledAnimate(
    this: Element,
    keyframes: Keyframe[] | PropertyIndexedKeyframes | null,
    options?: number | KeyframeAnimationOptions,
  ) {
    const animation = animate.call(this, keyframes, options);
    const cancel = animation.cancel;
    animation.cancel = function handledCancel(this: Animation) {
      this.finished.catch(() => undefined);
      cancel.call(this);
    };
    return animation;
  };
});
afterAll(() => {
  Element.prototype.animate = animate;
});

beforeEach(() => localStorage.clear());
afterEach(() => cleanup());

describe("ProBot AI in a desktop rule's trigger", () => {
  it("is not offered while the service has it not for this player", () => {
    render(<Section />);
    expect(screen.queryByText("ProBot AI ile Kalıp Kur")).toBeNull();
    expect(screen.getByText("Kalıbı Kendim Yazacağım")).toBeTruthy();
  });

  it("asks with the player's words alone and puts the pattern it made in the rule", async () => {
    const suggest = vi.fn<RulePatterns["suggest"]>(async () => MADE);
    render(<Section patterns={{ accountId: "desktop:afk-1", suggest }} />);
    // With ProBot AI, writing a pattern by hand waits in its box (as in a 7/24 Bulut rule).
    expect(screen.queryByText("Kalıbı Kendim Yazacağım")).toBeNull();
    fireEvent.click(screen.getByText("ProBot AI ile Kalıp Kur"));
    fireEvent.change(screen.getByLabelText("Hangi mesajlara yanıt versin? Kendi sözlerinle yaz."), {
      target: { value: " biri tp ya da ışınla yazınca " },
    });
    fireEvent.click(screen.getByRole("button", { name: "Oluştur" }));
    expect(suggest).toHaveBeenCalledWith("biri tp ya da ışınla yazınca", false);
    expect(await screen.findByText(MADE.summary)).toBeTruthy();
    fireEvent.click(screen.getByText("Bu Kalıbı Kullan"));
    expect(screen.getByTestId("rule").textContent).toBe(`regex:${MADE.match}`);
    expect(screen.getByText("Ne zaman yanıt verir?")).toBeTruthy();
    // Kept for the account and the pattern: a desktop rule has no lasting id.
    expect(savedSentence("desktop:afk-1", "rule", MADE.match)).toBe(MADE.summary);
  });
});
