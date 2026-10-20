import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// "Discord'dan mesaj at" with the backend and the browser opener stubbed.
const openInApp = vi.fn<(id: string) => Promise<boolean>>();
const openWebLink = vi.fn<(href: string) => void>();
vi.mock("@/lib/api", () => ({ api: { social: { openInDiscord: (id: string) => openInApp(id) } } }));
vi.mock("@/lib/links", () => ({ openWebLink: (href: string) => openWebLink(href) }));

const { openInDiscord, REPEAT_MS } = await import("./discord-link");

const ayse = { id: "80351110224678912", discordUrl: "https://discord.com/users/80351110224678912" };
const mert = { id: "80351110224678913", discordUrl: "https://discord.com/users/80351110224678913" };

describe("openInDiscord", () => {
  let clock = Date.parse("2026-10-20T12:00:00Z");
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    // Every test starts well after the previous one's presses.
    clock += 60_000;
    vi.setSystemTime(clock);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("opens the profile in the Discord app when it is installed", async () => {
    openInApp.mockResolvedValue(true);
    await openInDiscord(ayse);
    expect(openInApp).toHaveBeenCalledWith(ayse.id);
    expect(openWebLink).not.toHaveBeenCalled();
  });

  it("opens the web profile when the app is not installed", async () => {
    openInApp.mockResolvedValue(false);
    await openInDiscord(ayse);
    expect(openWebLink).toHaveBeenCalledWith(ayse.discordUrl);
  });

  it("opens the web profile when asking the backend fails", async () => {
    openInApp.mockRejectedValue(new Error("ipc channel closed"));
    await openInDiscord(ayse);
    expect(openWebLink).toHaveBeenCalledWith(ayse.discordUrl);
  });

  it("ignores a double click on the same person, not a later press", async () => {
    openInApp.mockResolvedValue(false);
    await openInDiscord(ayse);
    await openInDiscord(ayse);
    expect(openInApp).toHaveBeenCalledTimes(1);

    vi.setSystemTime(Date.now() + REPEAT_MS);
    await openInDiscord(ayse);
    expect(openInApp).toHaveBeenCalledTimes(2);
    expect(openWebLink.mock.calls).toEqual([[ayse.discordUrl], [ayse.discordUrl]]);
  });

  it("opens someone else at once", async () => {
    openInApp.mockResolvedValue(true);
    await openInDiscord(ayse);
    await openInDiscord(mert);
    expect(openInApp.mock.calls).toEqual([[ayse.id], [mert.id]]);
  });
});
