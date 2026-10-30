// @vitest-environment happy-dom
// The desktop console with console looks and dialog cards (AFK-UX): a line in the game's colours,
// a clicked command that waits for the player's word and then goes the typed line's way, and a
// server's dialog as a card whose press goes to pl-afk.
import { TooltipProvider } from "@pl/ui";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { type AfkAccount, type AfkLogLine, api } from "@/lib/api";
import { AfkConsole } from "./afk-console";
import { useAfkPanelSize } from "./panel/side-panel";

const NOW = "2026-10-30T18:00:00Z";

const ACCOUNT: AfkAccount = {
  id: "afk-test",
  nickname: "KasaBekcisi",
  server: "play.projects.gg",
  autoStart: false,
  mcVersion: "26.3",
  mcVersionRecommended: true,
  state: { kind: "connected", since: NOW },
  running: true,
};

const LINES: AfkLogLine[] = [
  { id: 0, time: NOW, text: "Sunucuya bağlanılıyor: play.projects.gg", kind: "chat" },
  {
    id: 1,
    time: NOW,
    text: "[Duyuru] Etkinlik başladı [Katıl]",
    kind: "chat",
    st: [
      [9, "6l"],
      [17, "e"],
      [7, "a", 0],
    ],
    ev: [{ click: { a: "run_command", v: "/etkinlik" } }],
  },
];

const RULES = {
  title: "Sunucu Kuralları",
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

function renderConsole() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <TooltipProvider>
        <AfkConsole account={ACCOUNT} onClose={() => {}} />
      </TooltipProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  useAfkPanelSize.setState({ full: false });
  vi.spyOn(api, "afkLog").mockResolvedValue(LINES);
  vi.spyOn(api, "afkGlobalSettings").mockResolvedValue({ showScriptCommands: false });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("the desktop console's looks", () => {
  it("shows the line's parts and asks before a clicked command runs", async () => {
    vi.spyOn(api, "afkDialog").mockResolvedValue(null);
    const sent = vi.spyOn(api, "afkSend").mockResolvedValue();
    renderConsole();
    const part = await screen.findByRole("button", { name: /\[Katıl\]/ });
    // A plain line stays as it was.
    expect(screen.getByText(/Sunucuya bağlanılıyor/)).toBeTruthy();
    act(() => {
      fireEvent.click(part);
    });
    // Nothing went yet: the card shows the whole command and waits.
    expect(sent).not.toHaveBeenCalled();
    expect(screen.getByText("/etkinlik")).toBeTruthy();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Çalıştır" }));
    });
    expect(sent).toHaveBeenCalledWith("afk-test", "/etkinlik");
  });

  it("shows the server's dialog as a card and presses it through pl-afk", async () => {
    vi.spyOn(api, "afkDialog").mockResolvedValue({
      rev: 4,
      dialog: JSON.stringify(RULES),
      at: NOW,
    });
    const pressed = vi.spyOn(api, "afkDialogPress").mockResolvedValue({ ok: false, why: "stale" });
    renderConsole();
    const accept = await screen.findByRole("button", { name: /Kabul et/ });
    expect(screen.getByText("Sunucu Kuralları")).toBeTruthy();
    await act(async () => {
      fireEvent.click(accept);
    });
    expect(pressed).toHaveBeenCalledWith("afk-test", { rev: 4, i: 0, inputs: {} });
    await waitFor(() => expect(screen.getByText("Sunucu penceresi değişti.")).toBeTruthy());
  });
});
