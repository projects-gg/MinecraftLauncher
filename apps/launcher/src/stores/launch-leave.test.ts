import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LaunchEvent } from "@/lib/api";

// "Oyun açılınca launcher" in the desktop app (SMALL7): the launch store with a window, the shell's
// commands and its events stubbed.
const launch = vi.fn(async (..._args: unknown[]) => {});
const commands = {
  launcherLeaveForGame: vi.fn(async () => {}),
  windowBringForward: vi.fn(async () => {}),
  gameAdopted: vi.fn(
    async (): Promise<{
      instanceId: string;
      sinceMs: number;
      account?: { id: string; name: string } | null;
    } | null> => null,
  ),
};
const win = {
  isVisible: vi.fn(async () => true),
  isMinimized: vi.fn(async () => false),
  minimize: vi.fn(async () => {}),
  hide: vi.fn(async () => {}),
};
let heard: ((event: { payload: { instanceId: string } }) => void) | undefined;
const prefs = { selectInstance: vi.fn(), onGameStart: "close" };

vi.mock("@/bindings", () => ({ commands }));
vi.mock("@/lib/api", () => ({
  api: { launch: (...args: unknown[]) => launch(...args), cancelLaunch: vi.fn() },
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => win }));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(async () => {}),
  listen: vi.fn(async (_name: string, handler: typeof heard) => {
    heard = handler;
    return () => {
      heard = undefined;
    };
  }),
}));
vi.mock("@/lib/platform", () => ({ isTauri: () => true }));
vi.mock("@/lib/window-away", () => ({ refreshWindowAway: vi.fn(async () => {}) }));
vi.mock("@/features/accounts/afk-guard", () => ({ confirmAfkBeforePlay: async () => true }));
vi.mock("@pl/ui", () => ({
  toast: Object.assign(vi.fn(), { success: vi.fn(), error: vi.fn() }),
}));
vi.mock("@/lib/sound", () => ({ playSound: vi.fn() }));
vi.mock("@/lib/attention", () => ({ notifyWhenAway: vi.fn() }));
vi.mock("@/lib/i18n", () => ({ default: { t: (key: string) => key } }));
vi.mock("@/features/java/prompt-store", () => ({
  useJavaPrompt: { getState: () => ({ open: vi.fn() }) },
}));
vi.mock("@/features/modcenter/notices", () => ({
  announceModsNotice: vi.fn(),
  announceVersionDropped: vi.fn(),
}));
vi.mock("@/features/modhealth/flow", () => ({
  announceModWarnings: vi.fn(),
  useModFlow: { getState: () => ({ open: vi.fn() }) },
}));
vi.mock("./prefs", () => ({ usePrefs: { getState: () => prefs } }));

const { followAdoptedGame, useLaunch } = await import("./launch");

/** Starts a launch whose backend events the test sends; the game's window shows. */
async function playUntilShown(): Promise<(event: LaunchEvent) => void> {
  let send: ((event: LaunchEvent) => void) | undefined;
  launch.mockImplementationOnce(async (...args: unknown[]) => {
    send = args[1] as (event: LaunchEvent) => void;
  });
  await useLaunch.getState().play("projects");
  if (!send) throw new Error("the launch did not start");
  send({
    type: "started",
    pid: 42,
    firstStart: false,
    account: { id: "a1", name: "EjderAvcisi" },
  });
  send({ type: "windowShown" });
  return send;
}

describe("Oyun açılınca launcher", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    for (const fn of [
      commands.launcherLeaveForGame,
      commands.windowBringForward,
      win.hide,
      win.minimize,
    ]) {
      fn.mockClear();
    }
    win.isVisible.mockResolvedValue(true);
    win.isMinimized.mockResolvedValue(false);
    useLaunch.setState({ phase: { kind: "idle" }, instanceId: null });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("Kapansın leaves the game alone once Oyunda has settled, and does not come back", async () => {
    prefs.onGameStart = "close";
    const send = await playUntilShown();
    await vi.advanceTimersByTimeAsync(200);
    expect(commands.launcherLeaveForGame).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(300);
    expect(commands.launcherLeaveForGame).toHaveBeenCalledTimes(1);
    expect(win.hide).not.toHaveBeenCalled();
    // AFK accounts kept the launcher in the tray: the game's end does not bring it back.
    send({ type: "exited", code: 0, runtimeSecs: 600 });
    expect(useLaunch.getState().phase.kind).toBe("idle");
    expect(commands.windowBringForward).not.toHaveBeenCalled();
  });

  it("Kapansın leaves even when the window was out of sight already", async () => {
    prefs.onGameStart = "close";
    win.isVisible.mockResolvedValue(false);
    await playUntilShown();
    await vi.advanceTimersByTimeAsync(0);
    expect(commands.launcherLeaveForGame).toHaveBeenCalledTimes(1);
  });

  it("Tepsiye gizlensin hides, and the window comes back after the game", async () => {
    prefs.onGameStart = "tray";
    const send = await playUntilShown();
    await vi.advanceTimersByTimeAsync(450);
    expect(win.hide).toHaveBeenCalledTimes(1);
    expect(commands.launcherLeaveForGame).not.toHaveBeenCalled();
    send({ type: "exited", code: 0, runtimeSecs: 600 });
    expect(commands.windowBringForward).toHaveBeenCalledTimes(1);
  });

  it("a game that closed while Oyunda settled keeps the launcher", async () => {
    prefs.onGameStart = "close";
    const send = await playUntilShown();
    send({ type: "exited", code: 1, runtimeSecs: 40 });
    await vi.advanceTimersByTimeAsync(450);
    expect(commands.launcherLeaveForGame).not.toHaveBeenCalled();
  });
});

describe("a game an earlier start left running", () => {
  beforeEach(() => {
    useLaunch.setState({ phase: { kind: "idle" }, instanceId: null });
  });

  it("is Oyunda from the start until it ends", async () => {
    commands.gameAdopted.mockResolvedValueOnce({ instanceId: "projects", sinceMs: 1_000 });
    const ended = vi.fn();
    const stop = followAdoptedGame(ended);
    await vi.waitFor(() =>
      expect(useLaunch.getState().phase).toMatchObject({ kind: "running", since: 1_000 }),
    );
    expect(useLaunch.getState()).toMatchObject({
      instanceId: "projects",
      phase: { shown: true, firstStart: false },
    });
    heard?.({ payload: { instanceId: "projects" } });
    expect(useLaunch.getState().phase.kind).toBe("idle");
    expect(ended).toHaveBeenCalledWith("projects");
    stop();
  });

  it("keeps the account it plays with (PLAY-ACCT); one from an older launcher has none", async () => {
    const account = { id: "a1", name: "EjderAvcisi" };
    commands.gameAdopted.mockResolvedValueOnce({ instanceId: "projects", sinceMs: 1_000, account });
    const stop = followAdoptedGame(vi.fn());
    await vi.waitFor(() => expect(useLaunch.getState().account).toEqual(account));
    stop();
    useLaunch.setState({ phase: { kind: "idle" }, instanceId: null, account: null });
    commands.gameAdopted.mockResolvedValueOnce({ instanceId: "projects", sinceMs: 1_000 });
    const again = followAdoptedGame(vi.fn());
    await vi.waitFor(() => expect(useLaunch.getState().phase.kind).toBe("running"));
    expect(useLaunch.getState().account).toBeNull();
    again();
  });

  it("changes nothing when no game was left running", async () => {
    commands.gameAdopted.mockResolvedValueOnce(null);
    const stop = followAdoptedGame(vi.fn());
    await vi.waitFor(() => expect(commands.gameAdopted).toHaveBeenCalled());
    await Promise.resolve();
    expect(useLaunch.getState().phase.kind).toBe("idle");
    stop();
  });
});
