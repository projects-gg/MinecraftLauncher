// @vitest-environment happy-dom
// SMALLS-L3: the 7/24 Bulut console with console kinds off, as client 4.2.5's side events run
// (afk-host 0.3.4): the classic lines in the game's colours and the server's dialog as a card,
// each by its own switch, as afk.projects.gg shows them; with their switches off the lines stay
// plain and no card shows.
import { TooltipProvider } from "@pl/ui";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { CloudAccountFeatures, CloudConsoleLine } from "@/bindings";
import "@/lib/i18n";
import { useAfkPanelSize } from "../panel/side-panel";
import { type CloudAccount, type CloudLiveEvent, cloudApi } from "./api";
import { CloudConsole } from "./console";

// happy-dom rejects a cancelled animation's `finished` (motion's own, at unmount) and nobody
// listens there (as afk-console.test.tsx).
const cancel = Animation.prototype.cancel;
beforeAll(() => {
  Animation.prototype.cancel = function (this: Animation) {
    this.finished.catch(() => undefined);
    cancel.call(this);
  };
});

const NOW = Math.floor(Date.now() / 1000);
const ID = "a_TEST0000000000001";

/** Studio's switches as they are now: console kinds off, looks and dialog cards on. */
const SIDE: CloudAccountFeatures = {
  inventory: { mode: "off", on: false },
  consoleKinds: false,
  consoleLooks: true,
  dialogCards: true,
  placeholders: false,
  legacyRules: false,
  movement: null,
};

function account(features: CloudAccountFeatures): CloudAccount {
  return {
    id: ID,
    name: "Madenci07",
    server: { host: "mc.madenciler.net", port: 25570 },
    version: "26.3",
    lang: "tr",
    host: "kz1",
    createdAt: NOW - 12 * 86_400,
    expiresAt: NOW + 18 * 86_400,
    expired: false,
    graceUntil: NOW + 48 * 86_400,
    renewal: { open: true, opensAt: NOW - 86_400 },
    desired: "run",
    hold: null,
    live: { state: "running", since: NOW - 3_600, reason: null, attempt: null, retryAt: null },
    everJoined: true,
    config: {
      v: 1,
      owner: "",
      antiAfk: { enabled: false, delayMin: 60, delayMax: 90 },
      commands: [],
      rules: [],
      inventory: null,
      movement: null,
    },
    rev: 1,
    serverChanges: { used: 0, limit: 6, resetAt: null },
    tier: "plain",
    plusUntil: null,
    ai: null,
    features,
    moderation: null,
  };
}

/** A classic line as the client prints it, its looks riding on it (no kind, no other player). */
const LINE: CloudConsoleLine = {
  at: NOW,
  text: "[Duyuru] Etkinlik başladı [Katıl]",
  k: "n",
  c: null,
  who: null,
  goto: null,
  st: [
    [9, "6l"],
    [17, "e"],
    [7, "a", 0],
  ],
  ev: [{ click: { a: "run_command", v: "/etkinlik" } }],
};

const RULES = {
  title: "Sunucu kuralları",
  body: [{ text: "1. Saygılı ol." }],
  inputs: [],
  buttons: [
    { i: 0, label: "Kabul et", a: "custom" },
    { i: 1, label: "Kapat", a: "close" },
  ],
  cancel: true,
  play: true,
  cols: 2,
};

let live: ((event: CloudLiveEvent) => void) | null = null;
const emit = (event: CloudLiveEvent) => act(() => live?.(event));

function renderConsole(features: CloudAccountFeatures) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <TooltipProvider>
        <CloudConsole
          account={account(features)}
          skewMs={0}
          onStart={() => {}}
          onRestart={() => {}}
          onOpenSettings={() => {}}
          onClose={() => {}}
        />
      </TooltipProvider>
    </QueryClientProvider>,
  );
}

/** The part of the line drawn in the game's gold and bold ("6l"), if any. */
const goldPart = () =>
  [...document.querySelectorAll<HTMLElement>("[role=log] span")].find(
    (span) => span.textContent === "[Duyuru] " && span.style.fontWeight === "700",
  );

beforeEach(() => {
  useAfkPanelSize.setState({ full: false });
  vi.spyOn(cloudApi, "onLive").mockImplementation((listener) => {
    live = listener;
    return () => {
      live = null;
    };
  });
  vi.spyOn(cloudApi, "watch").mockResolvedValue();
  vi.spyOn(cloudApi, "unwatch").mockResolvedValue();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  live = null;
});

describe("the cloud console with console kinds off", () => {
  it("draws the lines in the game's colours and the server's dialog as a card", async () => {
    renderConsole(SIDE);
    emit({ kind: "lines", acct: ID, lines: [LINE], skipped: 0, snapshot: true });
    await screen.findByText(/Etkinlik başladı/);
    expect(goldPart()).toBeTruthy();
    expect(goldPart()?.style.color).not.toBe("");

    emit({ kind: "dialog", acct: ID, rev: 2, dialog: JSON.stringify(RULES), wait: false });
    expect(await screen.findByText("Sunucu kuralları")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Kabul et/ })).toBeTruthy();
  });

  it("keeps the lines plain and shows no card while their switches are off", async () => {
    renderConsole({ ...SIDE, consoleLooks: false, dialogCards: false });
    emit({ kind: "lines", acct: ID, lines: [LINE], skipped: 0, snapshot: true });
    await screen.findByText(/Etkinlik başladı/);
    expect(goldPart()).toBeUndefined();

    emit({ kind: "dialog", acct: ID, rev: 2, dialog: JSON.stringify(RULES), wait: false });
    expect(screen.queryByText("Sunucu kuralları")).toBeNull();
  });
});
