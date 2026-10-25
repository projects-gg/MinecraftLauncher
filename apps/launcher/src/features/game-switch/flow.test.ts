import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LaunchPhase } from "@/stores/launch";

// The way after "Kapat ve oyna" with the launch store, the backend and the prefs stubbed.
const closeGame = vi.fn(async () => true);
const killGame = vi.fn(async () => {});
const selectInstance = vi.fn();
vi.mock("@/lib/api", () => ({
  api: { closeGame: () => closeGame(), killGame: () => killGame() },
}));
vi.mock("@/features/accounts/use-accounts", () => ({ announce: vi.fn() }));
vi.mock("@/lib/i18n", () => ({ default: { t: (key: string) => key } }));
vi.mock("@/stores/prefs", () => ({ usePrefs: { getState: () => ({ selectInstance }) } }));
vi.mock("@/stores/launch", async () => {
  const { create } = await import("zustand");
  const useLaunch = create<{
    phase: LaunchPhase;
    instanceId: string | null;
    play: (id: string, options?: unknown) => Promise<void>;
    cancel: () => Promise<void>;
  }>()((set) => ({
    phase: { kind: "idle" },
    instanceId: null,
    play: vi.fn(async () => {}),
    // As the store does: idle at once, the backend stops it.
    cancel: vi.fn(async () => set({ phase: { kind: "idle" } })),
  }));
  return { useLaunch };
});

const { useLaunch } = await import("@/stores/launch");
const { useGameSwitch } = await import("./store");
const { cancelSwitch, confirmSwitch, sendSwitch } = await import("./flow");

const open: LaunchPhase = { kind: "running", since: 1, shown: true, firstStart: false };
const opening: LaunchPhase = { kind: "running", since: 1, shown: false, firstStart: false };
const kor = { id: "a2", name: "KorAtesi_" };
const request = { instanceId: "projects", account: kor, restore: null };
const sides = {
  stage: "open" as const,
  from: {
    account: { id: "a1", name: "EjderAvcisi" },
    instance: "projects",
    instanceName: "Projects",
  },
  to: { account: kor, instance: "projects", instanceName: "Projects" },
};

const play = () => vi.mocked(useLaunch.getState().play);
const exit = () => useLaunch.setState({ phase: { kind: "idle" } });

/** Lets the promise callbacks of the step that just began run. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("Kapat ve oyna (PLAY-ACCT)", () => {
  beforeEach(() => {
    closeGame.mockReset();
    closeGame.mockResolvedValue(true);
    killGame.mockClear();
    selectInstance.mockClear();
    play().mockClear();
    vi.mocked(useLaunch.getState().cancel).mockClear();
    useLaunch.setState({ phase: open, instanceId: "projects" });
    useGameSwitch.setState({ request, step: null, sides: null });
  });

  afterEach(() => {
    vi.useRealTimers();
    // Nothing of a test's way is left for the next one.
    cancelSwitch();
  });

  it("closes the game the way its close button does, then starts with the selected account", async () => {
    confirmSwitch(request, sides);
    expect(useGameSwitch.getState().step).toMatchObject({ kind: "closing" });
    expect(closeGame).toHaveBeenCalledTimes(1);
    await settle();
    // Nothing starts while the game is still on.
    expect(play()).not.toHaveBeenCalled();
    exit();
    expect(play()).toHaveBeenCalledWith("projects", undefined);
    expect(useGameSwitch.getState().request).toBeNull();
  });

  it("keeps the words it asked with while the game goes", () => {
    confirmSwitch(request, sides);
    expect(useGameSwitch.getState().sides).toMatchObject({ stage: "open", to: { account: kor } });
  });

  it("stops a game that is still opening and starts at once (nothing to save)", () => {
    useLaunch.setState({ phase: opening });
    confirmSwitch(request, { ...sides, stage: "opening" });
    expect(useLaunch.getState().cancel).toHaveBeenCalledTimes(1);
    expect(closeGame).not.toHaveBeenCalled();
    expect(play()).toHaveBeenCalledTimes(1);
  });

  it("starts straight away when the game ended before the answer", () => {
    exit();
    confirmSwitch(request, sides);
    expect(play()).toHaveBeenCalledTimes(1);
    expect(closeGame).not.toHaveBeenCalled();
  });

  it("offers to end a game that has not closed in time, and ends it only at the player's word", async () => {
    vi.useFakeTimers();
    confirmSwitch(request, sides);
    await vi.advanceTimersByTimeAsync(20_000);
    expect(useGameSwitch.getState().step).toMatchObject({ kind: "overdue" });
    expect(killGame).not.toHaveBeenCalled();
    sendSwitch({ type: "force", now: Date.now() });
    expect(useGameSwitch.getState().step).toMatchObject({ kind: "forcing" });
    expect(killGame).toHaveBeenCalledTimes(1);
    exit();
    expect(play()).toHaveBeenCalledTimes(1);
  });

  it("goes to that question at once when the game cannot be asked to close", async () => {
    closeGame.mockResolvedValue(false);
    confirmSwitch(request, sides);
    await settle();
    expect(useGameSwitch.getState().step).toMatchObject({ kind: "overdue" });
  });

  it("gives another installation's selection back on Vazgeç at the question", () => {
    useGameSwitch.setState({ request: { ...request, restore: "hardcore" } });
    cancelSwitch();
    expect(selectInstance).toHaveBeenCalledWith("hardcore");
    expect(useGameSwitch.getState().request).toBeNull();
  });

  it("starts nothing more after Vazgeç on the way", async () => {
    confirmSwitch(request, sides);
    await settle();
    cancelSwitch();
    exit();
    expect(play()).not.toHaveBeenCalled();
    expect(selectInstance).not.toHaveBeenCalled();
  });
});
