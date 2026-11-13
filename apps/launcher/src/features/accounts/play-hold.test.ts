import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AccountSummary } from "@/lib/api";

const accounts = vi.fn<() => Promise<AccountSummary[]>>();
vi.mock("@/lib/api", () => ({ api: { accounts: () => accounts() } }));

const { HOLD_MS, dropHeldPlay, heldPlay, holdPlay, holdStep, noteSelected, waitsForAccount } =
  await import("./play-hold");
const { useAccountsUi } = await import("./store");

const account = (id: string, active: boolean) => ({ id, active }) as unknown as AccountSummary;
const play = { instanceId: "official", options: { autoJoin: true } };

describe("holdStep", () => {
  it("an account wait goes on once any account is there", () => {
    const wait = { kind: "account" } as const;
    expect(holdStep(wait, 0, 1, { kind: "fresh", account: "a" })).toBe("resume");
    expect(holdStep(wait, 0, 1, { kind: "selected", account: "a" })).toBe("resume");
    expect(holdStep(wait, 0, 1, { kind: "selected", account: null })).toBe("keep");
    expect(holdStep(wait, 0, 1, { kind: "closed" })).toBe("drop");
  });

  it("a sign-in wait goes on only once that account is signed in again", () => {
    const wait = { kind: "signIn", account: "a" } as const;
    expect(holdStep(wait, 0, 1, { kind: "fresh", account: "a" })).toBe("resume");
    expect(holdStep(wait, 0, 1, { kind: "fresh", account: "b" })).toBe("keep");
    expect(holdStep(wait, 0, 1, { kind: "selected", account: "b" })).toBe("keep");
    expect(holdStep(wait, 0, 1, { kind: "closed" })).toBe("drop");
  });

  it("a launch waits at most ten minutes", () => {
    const wait = { kind: "account" } as const;
    expect(holdStep(wait, 0, HOLD_MS, { kind: "fresh", account: "a" })).toBe("resume");
    expect(holdStep(wait, 0, HOLD_MS + 1, { kind: "fresh", account: "a" })).toBe("drop");
  });
});

describe("a held launch", () => {
  beforeEach(() => {
    vi.stubGlobal("document", { activeElement: null });
    vi.stubGlobal("HTMLElement", class {});
    useAccountsUi.setState({
      centerOpen: false,
      login: { open: false, relogin: null },
      fresh: null,
      hosts: 1,
    });
  });
  afterEach(() => {
    dropHeldPlay();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it("OYNA with no account opens the center and starts once one is added", async () => {
    accounts.mockResolvedValue([]);
    const resume = vi.fn();
    expect(await waitsForAccount(play, resume)).toBe(true);
    const ui = useAccountsUi.getState();
    expect(ui.centerOpen).toBe(true);
    expect(ui.centerFocus).toBe("add");
    expect(heldPlay()).toEqual(play);
    // The Microsoft sign-in opens over the center: still waiting.
    useAccountsUi.getState().openLogin(null);
    expect(resume).not.toHaveBeenCalled();
    // It adds the account.
    useAccountsUi.getState().markFresh("a");
    expect(resume).toHaveBeenCalledWith(play);
    expect(useAccountsUi.getState().centerOpen).toBe(false);
    expect(heldPlay()).toBeNull();
    // The account list catching up starts nothing more.
    noteSelected("a");
    expect(resume).toHaveBeenCalledTimes(1);
  });

  it("picking an account in the center starts it too", async () => {
    accounts.mockResolvedValue([account("a", false)]);
    const resume = vi.fn();
    expect(await waitsForAccount(play, resume)).toBe(true);
    noteSelected(undefined);
    noteSelected(null);
    expect(resume).not.toHaveBeenCalled();
    noteSelected("a");
    expect(resume).toHaveBeenCalledTimes(1);
  });

  it("closing the center without an account lets the launch go", async () => {
    accounts.mockResolvedValue([]);
    const resume = vi.fn();
    await waitsForAccount(play, resume);
    useAccountsUi.getState().closeCenter();
    expect(heldPlay()).toBeNull();
    useAccountsUi.getState().markFresh("a");
    expect(resume).not.toHaveBeenCalled();
  });

  it("an account, a named account, no account surfaces or an unreadable list go on", async () => {
    const resume = vi.fn();
    accounts.mockResolvedValue([account("a", true)]);
    expect(await waitsForAccount(play, resume)).toBe(false);
    accounts.mockResolvedValue([]);
    const named = { instanceId: "x", options: { account: { id: "b", name: "x7Ender" } } };
    expect(await waitsForAccount(named, resume)).toBe(false);
    useAccountsUi.setState({ hosts: 0 });
    expect(await waitsForAccount(play, resume)).toBe(false);
    useAccountsUi.setState({ hosts: 1 });
    accounts.mockRejectedValue(new Error("no backend"));
    expect(await waitsForAccount(play, resume)).toBe(false);
    expect(heldPlay()).toBeNull();
    expect(useAccountsUi.getState().centerOpen).toBe(false);
  });

  it("after a new sign-in of the expired account the game starts by itself", () => {
    const resume = vi.fn();
    holdPlay(play, { kind: "signIn", account: "a" }, resume);
    useAccountsUi.getState().openLogin("a");
    // Another account signed in meanwhile is not this one.
    useAccountsUi.getState().markFresh("b");
    expect(resume).not.toHaveBeenCalled();
    useAccountsUi.getState().markFresh("a");
    expect(resume).toHaveBeenCalledWith(play);
    // The dialog closing after its success changes nothing.
    useAccountsUi.getState().closeLogin();
    expect(resume).toHaveBeenCalledTimes(1);
  });

  it("a sign-in closed without success lets the launch go", () => {
    const resume = vi.fn();
    holdPlay(play, { kind: "signIn", account: "a" }, resume);
    useAccountsUi.getState().openLogin("a");
    useAccountsUi.getState().closeLogin();
    expect(heldPlay()).toBeNull();
    useAccountsUi.getState().markFresh("a");
    expect(resume).not.toHaveBeenCalled();
  });

  it("a wait older than ten minutes starts nothing", () => {
    vi.useFakeTimers();
    const resume = vi.fn();
    holdPlay(play, { kind: "signIn", account: "a" }, resume);
    useAccountsUi.getState().openLogin("a");
    vi.advanceTimersByTime(HOLD_MS + 1000);
    useAccountsUi.getState().markFresh("a");
    expect(resume).not.toHaveBeenCalled();
    expect(heldPlay()).toBeNull();
  });
});
