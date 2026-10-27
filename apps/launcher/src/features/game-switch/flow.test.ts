import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LaunchPhase, RunningGame } from "@/stores/launch";

// The way after the question's answer with the launch store and the prefs stubbed.
const selectInstance = vi.fn();
vi.mock("@/features/accounts/use-accounts", () => ({ announce: vi.fn() }));
vi.mock("@/lib/i18n", () => ({ default: { t: (key: string) => key } }));
vi.mock("@/stores/prefs", () => ({ usePrefs: { getState: () => ({ selectInstance }) } }));
vi.mock("@/stores/launch", async () => {
  const { create } = await import("zustand");
  const useLaunch = create<{
    phase: LaunchPhase;
    instanceId: string | null;
    progress: number;
    games: RunningGame[];
    play: (id: string, options?: unknown) => Promise<void>;
    start: (id: string, options?: unknown) => Promise<void>;
    cancel: () => Promise<void>;
  }>()((set) => ({
    phase: { kind: "idle" },
    instanceId: null,
    progress: 0,
    games: [],
    play: vi.fn(async () => {}),
    start: vi.fn(async () => {}),
    // As the store does: the launch on its way stops at once (its game, if any, goes with it).
    cancel: vi.fn(async () =>
      set((state) => ({
        progress: 0,
        games: state.games.filter((g) => g.run !== state.progress),
        phase: { kind: "idle" },
      })),
    ),
  }));
  return { useLaunch };
});

const { useLaunch } = await import("@/stores/launch");
const { useGameSwitch } = await import("./store");
const { cancelSwitch, confirmSwitch } = await import("./flow");

const opening: LaunchPhase = { kind: "running", since: 1, shown: false, firstStart: false };
const kor = { id: "a2", name: "KorAtesi_" };
const sides = {
  stage: "opening" as const,
  from: {
    account: { id: "a1", name: "EjderAvcisi" },
    instance: "projects",
    instanceName: "Projects",
  },
  to: { account: kor, instance: "projects", instanceName: "Projects" },
};
const stop = { kind: "stop" as const, instanceId: "projects", account: kor, restore: null };
const options = { account: kor, autoJoin: false };

const play = () => vi.mocked(useLaunch.getState().play);
const start = () => vi.mocked(useLaunch.getState().start);
const cancel = () => vi.mocked(useLaunch.getState().cancel);

describe("the answer (PLAY-ACCT, PLAY-AGAIN)", () => {
  beforeEach(() => {
    selectInstance.mockClear();
    play().mockClear();
    start().mockClear();
    cancel().mockClear();
    useLaunch.setState({ phase: opening, instanceId: "projects", progress: 7, games: [] });
    useGameSwitch.setState({ request: stop, step: null, sides: null });
  });

  afterEach(() => {
    // Nothing of a test's way is left for the next one.
    cancelSwitch();
  });

  it("opens one more game at once (the games on screen stay)", () => {
    const again = { ...stop, kind: "again" as const, options };
    useGameSwitch.setState({ request: again });
    confirmSwitch(again, null);
    expect(start()).toHaveBeenCalledWith("projects", options);
    expect(cancel()).not.toHaveBeenCalled();
    expect(useGameSwitch.getState().request).toBeNull();
    const memory = { ...stop, kind: "memory" as const, memoryShort: true, options };
    confirmSwitch(memory, null);
    expect(start()).toHaveBeenCalledTimes(2);
  });

  it("stops the launch on its way, then goes through OYNA's rules again", () => {
    useLaunch.setState({
      games: [
        {
          run: 7,
          game: 107,
          instanceId: "projects",
          account: null,
          since: 1,
          shown: false,
          firstStart: false,
        },
      ],
    });
    confirmSwitch(stop, sides);
    expect(cancel()).toHaveBeenCalledTimes(1);
    expect(play()).toHaveBeenCalledWith("projects", undefined);
    expect(start()).not.toHaveBeenCalled();
    expect(useGameSwitch.getState().request).toBeNull();
  });

  it("goes through the rules at once when the launch is no longer on its way", () => {
    useLaunch.setState({ progress: 0, phase: { kind: "idle" } });
    confirmSwitch(stop, sides);
    expect(cancel()).not.toHaveBeenCalled();
    expect(play()).toHaveBeenCalledTimes(1);
  });

  it("starts nothing when the launch on its way does not stop (Vazgeç on the way)", () => {
    // A launch that takes its time to stop.
    cancel().mockImplementationOnce(async () => {});
    confirmSwitch(stop, sides);
    expect(useGameSwitch.getState().step).toEqual({ kind: "stopping" });
    cancelSwitch();
    useLaunch.setState({ progress: 0, phase: { kind: "idle" } });
    expect(play()).not.toHaveBeenCalled();
  });

  it("gives another installation's selection back on Vazgeç at the question", () => {
    useGameSwitch.setState({ request: { ...stop, restore: "hardcore" } });
    cancelSwitch();
    expect(selectInstance).toHaveBeenCalledWith("hardcore");
    expect(useGameSwitch.getState().request).toBeNull();
  });
});
