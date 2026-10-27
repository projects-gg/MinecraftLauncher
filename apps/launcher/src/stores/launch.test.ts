import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LaunchEvent } from "@/lib/api";

// The launch store with its surroundings stubbed: the backend, the AFK check and the UI bits.
const launch = vi.fn(async (..._args: unknown[]) => {});
const cancelLaunch = vi.fn(async () => {});
const confirm = vi.fn(async () => true);
/** Whether one more game fits in the free memory (PLAY-AGAIN). */
const memoryFits = vi.fn(async () => true);
const flowOpen = vi.fn();
/** The account list as the backend has it: the first one is selected unless a test says. */
const accounts = vi.fn(async () => [
  { id: "a1", username: "EjderAvcisi", kind: "offline", active: true },
  { id: "a2", username: "KorAtesi_", kind: "microsoft", active: false },
]);
vi.mock("@/lib/api", () => ({
  api: {
    launch: (...args: unknown[]) => launch(...args),
    cancelLaunch: () => cancelLaunch(),
    accounts: () => accounts(),
    gameMemoryFits: () => memoryFits(),
  },
}));
vi.mock("@/features/accounts/afk-guard", () => ({
  confirmAfkBeforePlay: () => confirm(),
}));
vi.mock("@pl/ui", () => ({
  toast: Object.assign(vi.fn(), { success: vi.fn(), error: vi.fn(), info: vi.fn() }),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: vi.fn() }));
vi.mock("@/lib/sound", () => ({ playSound: vi.fn() }));
vi.mock("@/lib/attention", () => ({ notifyWhenAway: vi.fn(), notify: vi.fn() }));
vi.mock("@/lib/platform", () => ({ isTauri: () => false }));
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
  useModFlow: { getState: () => ({ open: flowOpen }) },
}));
vi.mock("./prefs", () => ({
  usePrefs: { getState: () => ({ selectInstance: vi.fn(), onGameStart: "keep" }) },
}));

const { isOpening, useLaunch } = await import("./launch");
const { useGameSwitch } = await import("@/features/game-switch/store");

/** The account the tests' games play with. */
const EJDER = { id: "a1", name: "EjderAvcisi" };
const KOR = { id: "a2", name: "KorAtesi_" };

/** Nothing runs, nothing is on its way. */
function reset() {
  useLaunch.setState({
    phase: { kind: "idle" },
    instanceId: null,
    account: null,
    games: [],
    progress: 0,
    followed: 0,
    crash: null,
  });
}

/** Starts a launch whose backend events the test sends. */
async function startLaunch(instanceId = "projects"): Promise<(event: LaunchEvent) => void> {
  let send: ((event: LaunchEvent) => void) | undefined;
  launch.mockImplementationOnce(async (...args: unknown[]) => {
    send = args[1] as (event: LaunchEvent) => void;
  });
  await useLaunch.getState().play(instanceId);
  if (!send) throw new Error("the launch did not start");
  return send;
}

describe("useLaunch.play", () => {
  beforeEach(() => {
    launch.mockClear();
    confirm.mockClear();
    reset();
  });

  it("starts one launch for a double click on OYNA (CR1-F)", async () => {
    // The second press arrives while the AFK check of the first still waits for the backend.
    await Promise.all([
      useLaunch.getState().play("projects"),
      useLaunch.getState().play("projects"),
    ]);
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(launch).toHaveBeenCalledTimes(1);
  });

  it("starts again once the first launch is over", async () => {
    await useLaunch.getState().play("projects");
    reset();
    await useLaunch.getState().play("projects");
    expect(launch).toHaveBeenCalledTimes(2);
  });

  it("starts nothing when the player cancels the AFK question", async () => {
    confirm.mockResolvedValueOnce(false);
    await useLaunch.getState().play("projects");
    expect(launch).not.toHaveBeenCalled();
    expect(useLaunch.getState().phase.kind).toBe("idle");
  });
});

describe("the game opening (PLAY1)", () => {
  beforeEach(() => {
    launch.mockClear();
    cancelLaunch.mockClear();
    reset();
  });

  it("says Açılıyor from the files being ready until the game's window shows", async () => {
    const send = await startLaunch();
    send({ type: "launching" });
    expect(isOpening(useLaunch.getState().phase)).toBe(true);
    send({ type: "started", pid: 42, firstStart: true, account: EJDER });
    const opening = useLaunch.getState().phase;
    expect(opening).toMatchObject({ kind: "running", shown: false, firstStart: true });
    expect(isOpening(opening)).toBe(true);
    send({ type: "windowShown" });
    const playing = useLaunch.getState().phase;
    expect(playing).toMatchObject({ kind: "running", shown: true });
    expect(isOpening(playing)).toBe(false);
    send({ type: "exited", code: 0, runtimeSecs: 30 });
    expect(useLaunch.getState().phase.kind).toBe("idle");
  });

  it("goes back to OYNA when the game closes before its window", async () => {
    const send = await startLaunch();
    send({ type: "started", pid: 42, firstStart: false, account: EJDER });
    send({ type: "exited", code: 1, runtimeSecs: 3 });
    expect(useLaunch.getState().phase.kind).toBe("idle");
  });

  it("closes a game that opens on İptal; the next launch waits for its exit", async () => {
    const send = await startLaunch();
    send({ type: "started", pid: 42, firstStart: false, account: EJDER });
    await useLaunch.getState().cancel();
    expect(cancelLaunch).toHaveBeenCalledTimes(1);
    expect(useLaunch.getState().phase.kind).toBe("idle");
    // OYNA again while the closed game is still exiting: nothing starts yet.
    const again = useLaunch.getState().play("projects");
    await Promise.resolve();
    expect(launch).toHaveBeenCalledTimes(1);
    send({ type: "exited", code: null, runtimeSecs: 1 });
    await again;
    expect(launch).toHaveBeenCalledTimes(2);
    // The closed game's exit did not touch the new launch.
    expect(useLaunch.getState().phase.kind).toBe("working");
  });

  it("never closes a game whose window is on screen", async () => {
    const send = await startLaunch();
    send({ type: "started", pid: 42, firstStart: false, account: EJDER });
    send({ type: "windowShown" });
    await useLaunch.getState().cancel();
    expect(cancelLaunch).not.toHaveBeenCalled();
    expect(useLaunch.getState().phase).toMatchObject({ kind: "running", shown: true });
  });

  it("follows a game whose window came before İptal could close it", async () => {
    const send = await startLaunch();
    send({ type: "started", pid: 42, firstStart: false, account: EJDER });
    await useLaunch.getState().cancel();
    send({ type: "windowShown" });
    expect(useLaunch.getState().phase).toMatchObject({ kind: "running", shown: true });
    send({ type: "exited", code: 0, runtimeSecs: 60 });
    expect(useLaunch.getState().phase.kind).toBe("idle");
  });
});

describe('"Devam et" into a world from the main menu (OVL-FIX)', () => {
  it("says what to do and why when the game opens on its main menu, and the launch goes on", async () => {
    reset();
    const { toast } = await import("@pl/ui");
    const shown = vi.mocked(toast.info);
    shown.mockClear();
    const send = await startLaunch();
    send({ type: "worldFromTitle", mods: ["Remove Reloading Screen"] });
    expect(shown).toHaveBeenCalledTimes(1);
    expect(shown.mock.calls[0]?.[0]).toBe("worldEntry.fromTitle");
    expect(shown.mock.calls[0]?.[1]).toMatchObject({ description: "worldEntry.fromTitleWhy" });
    send({ type: "launching" });
    expect(isOpening(useLaunch.getState().phase)).toBe(true);
  });
});

describe("OYNA for the account in game alone (PLAY-ACCT)", () => {
  /** Selects `id` the way the account switcher does (the backend's list). */
  function select(id: string) {
    accounts.mockImplementation(async () => [
      { id: "a1", username: "EjderAvcisi", kind: "offline", active: id === "a1" },
      { id: "a2", username: "KorAtesi_", kind: "microsoft", active: id === "a2" },
    ]);
  }

  /** EjderAvcisi's game of "projects" on screen. */
  async function playing() {
    const send = await startLaunch();
    send({ type: "started", pid: 42, firstStart: false, account: EJDER });
    send({ type: "windowShown" });
    return send;
  }

  beforeEach(() => {
    launch.mockClear();
    select("a1");
    reset();
    useGameSwitch.setState({ request: null, step: null, sides: null });
  });

  it("keeps the account the game plays with, from its start", async () => {
    const send = await startLaunch();
    expect(useLaunch.getState().account).toBeNull();
    send({ type: "started", pid: 42, firstStart: false, account: EJDER });
    expect(useLaunch.getState().account).toEqual(EJDER);
  });

  it("starts another account's game beside the one on screen (PLAY-AGAIN)", async () => {
    await playing();
    select("a2");
    await useLaunch.getState().play("projects");
    expect(useGameSwitch.getState().request).toBeNull();
    expect(launch).toHaveBeenCalledTimes(2);
    // It plays as the account selected at the press, whatever is selected later.
    expect(launch.mock.calls[1]?.[2]).toMatchObject({ account: { id: "a2", name: "KorAtesi_" } });
  });

  it("asks first when the free memory would not hold another game", async () => {
    await playing();
    select("a2");
    memoryFits.mockResolvedValueOnce(false);
    await useLaunch.getState().play("projects");
    expect(launch).toHaveBeenCalledTimes(1);
    expect(useGameSwitch.getState().request).toMatchObject({
      kind: "memory",
      memoryShort: true,
      instanceId: "projects",
    });
  });

  it("asks the owner's question for the account's own game; one more joins no server", async () => {
    await playing();
    await useLaunch.getState().play("projects");
    expect(launch).toHaveBeenCalledTimes(1);
    expect(useGameSwitch.getState().request).toMatchObject({
      kind: "again",
      memoryShort: false,
      instanceId: "projects",
      options: { account: EJDER, autoJoin: false },
    });
  });

  it("asks once for a double press", async () => {
    await playing();
    await Promise.all([
      useLaunch.getState().play("projects"),
      useLaunch.getState().play("projects"),
    ]);
    const first = useGameSwitch.getState().request;
    await useLaunch.getState().play("fabric");
    // The question on screen stays the first one.
    expect(useGameSwitch.getState().request).toBe(first);
    expect(launch).toHaveBeenCalledTimes(1);
  });

  it("asks the same for another installation of the same account", async () => {
    await playing();
    await useLaunch.getState().play("fabric");
    expect(useGameSwitch.getState().request).toMatchObject({ kind: "again", instanceId: "fabric" });
  });

  it("starts a game that ended while the account was being read", async () => {
    const send = await playing();
    select("a2");
    let release: () => void = () => {};
    accounts.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = () =>
            resolve([
              { id: "a1", username: "EjderAvcisi", kind: "offline", active: false },
              { id: "a2", username: "KorAtesi_", kind: "microsoft", active: true },
            ]);
        }),
    );
    const pressed = useLaunch.getState().play("projects");
    send({ type: "exited", code: 0, runtimeSecs: 60 });
    release();
    await pressed;
    expect(useGameSwitch.getState().request).toBeNull();
    expect(launch).toHaveBeenCalledTimes(2);
  });
});

describe("several games at once (PLAY-AGAIN)", () => {
  beforeEach(() => {
    launch.mockClear();
    flowOpen.mockClear();
    reset();
    useGameSwitch.setState({ request: null, step: null, sides: null });
  });

  /** A game of `account` on screen, its launch started at once (the question answered). */
  async function onScreen(account: typeof EJDER, game: number) {
    let send: ((event: LaunchEvent) => void) | undefined;
    launch.mockImplementationOnce(async (...args: unknown[]) => {
      send = args[1] as (event: LaunchEvent) => void;
    });
    await useLaunch.getState().start("projects", { account });
    if (!send) throw new Error("the launch did not start");
    send({ type: "started", pid: game, game, firstStart: false, account });
    send({ type: "windowShown" });
    return send;
  }

  it("follows each game; the bar stays on while one of them runs", async () => {
    const first = await onScreen(EJDER, 1);
    const second = await onScreen(KOR, 2);
    expect(useLaunch.getState().games.map((g) => g.game)).toEqual([1, 2]);
    expect(useLaunch.getState().account).toEqual(KOR);
    // The newer one ends: the store follows the other, still "Oyunda".
    second({ type: "exited", code: 0, runtimeSecs: 60 });
    expect(useLaunch.getState()).toMatchObject({
      account: EJDER,
      phase: { kind: "running", shown: true },
    });
    first({ type: "exited", code: 0, runtimeSecs: 90 });
    expect(useLaunch.getState().phase.kind).toBe("idle");
    expect(useLaunch.getState().games).toEqual([]);
  });

  it("helps with each crash in turn", async () => {
    const first = await onScreen(EJDER, 1);
    const second = await onScreen(EJDER, 2);
    const crash = (instanceId: string) =>
      ({
        type: "crashed",
        title: "t",
        hint: "h",
        actions: [],
        mods: [],
        logFile: null,
        crashReport: null,
        kind: "unknown",
        instanceId,
        suspects: [],
        repairable: false,
        turnOff: null,
        reinstall: false,
        memory: null,
        resetJvmArgs: false,
      }) as unknown as LaunchEvent;
    first(crash("projects"));
    second(crash("other"));
    expect(useLaunch.getState().crash?.instanceId).toBe("projects");
    expect(flowOpen).toHaveBeenCalledTimes(1);
    useLaunch.getState().dismissCrash();
    expect(useLaunch.getState().crash?.instanceId).toBe("other");
    expect(flowOpen).toHaveBeenCalledTimes(2);
    useLaunch.getState().dismissCrash();
    expect(useLaunch.getState().crash).toBeNull();
  });

  it("never closes a game on screen when another launch is cancelled", async () => {
    await onScreen(EJDER, 1);
    let send: ((event: LaunchEvent) => void) | undefined;
    launch.mockImplementationOnce(async (...args: unknown[]) => {
      send = args[1] as (event: LaunchEvent) => void;
    });
    await useLaunch.getState().start("projects", { account: KOR });
    expect(useLaunch.getState().progress).not.toBe(0);
    send?.({ type: "started", pid: 3, game: 3, firstStart: false, account: KOR });
    await useLaunch.getState().cancel();
    expect(useLaunch.getState().games.map((g) => g.game)).toEqual([1]);
    expect(useLaunch.getState().phase).toMatchObject({ kind: "running", shown: true });
  });
});
