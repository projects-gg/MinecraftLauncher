import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LaunchEvent } from "@/lib/api";

// The launch store with its surroundings stubbed: the backend, the AFK check and the UI bits.
const launch = vi.fn(async (..._args: unknown[]) => {});
const cancelLaunch = vi.fn(async () => {});
const confirm = vi.fn(async () => true);
vi.mock("@/lib/api", () => ({
  api: {
    launch: (...args: unknown[]) => launch(...args),
    cancelLaunch: () => cancelLaunch(),
  },
}));
vi.mock("@/features/accounts/afk-guard", () => ({
  confirmAfkBeforePlay: () => confirm(),
}));
vi.mock("@pl/ui", () => ({
  toast: Object.assign(vi.fn(), { success: vi.fn(), error: vi.fn() }),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: vi.fn() }));
vi.mock("@/lib/sound", () => ({ playSound: vi.fn() }));
vi.mock("@/lib/attention", () => ({ notifyWhenAway: vi.fn() }));
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
  useModFlow: { getState: () => ({ open: vi.fn() }) },
}));
vi.mock("./prefs", () => ({
  usePrefs: { getState: () => ({ selectInstance: vi.fn(), onGameStart: "keep" }) },
}));

const { isOpening, useLaunch } = await import("./launch");

/** Starts a launch whose backend events the test sends. */
async function startLaunch(): Promise<(event: LaunchEvent) => void> {
  let send: ((event: LaunchEvent) => void) | undefined;
  launch.mockImplementationOnce(async (...args: unknown[]) => {
    send = args[1] as (event: LaunchEvent) => void;
  });
  await useLaunch.getState().play("projects");
  if (!send) throw new Error("the launch did not start");
  return send;
}

describe("useLaunch.play", () => {
  beforeEach(() => {
    launch.mockClear();
    confirm.mockClear();
    useLaunch.setState({ phase: { kind: "idle" }, instanceId: null });
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
    useLaunch.setState({ phase: { kind: "idle" } });
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
    useLaunch.setState({ phase: { kind: "idle" }, instanceId: null });
  });

  it("says Açılıyor from the files being ready until the game's window shows", async () => {
    const send = await startLaunch();
    send({ type: "launching" });
    expect(isOpening(useLaunch.getState().phase)).toBe(true);
    send({ type: "started", pid: 42, firstStart: true });
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
    send({ type: "started", pid: 42, firstStart: false });
    send({ type: "exited", code: 1, runtimeSecs: 3 });
    expect(useLaunch.getState().phase.kind).toBe("idle");
  });

  it("closes a game that opens on İptal; the next launch waits for its exit", async () => {
    const send = await startLaunch();
    send({ type: "started", pid: 42, firstStart: false });
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
    send({ type: "started", pid: 42, firstStart: false });
    send({ type: "windowShown" });
    await useLaunch.getState().cancel();
    expect(cancelLaunch).not.toHaveBeenCalled();
    expect(useLaunch.getState().phase).toMatchObject({ kind: "running", shown: true });
  });

  it("follows a game whose window came before İptal could close it", async () => {
    const send = await startLaunch();
    send({ type: "started", pid: 42, firstStart: false });
    await useLaunch.getState().cancel();
    send({ type: "windowShown" });
    expect(useLaunch.getState().phase).toMatchObject({ kind: "running", shown: true });
    send({ type: "exited", code: 0, runtimeSecs: 60 });
    expect(useLaunch.getState().phase.kind).toBe("idle");
  });
});
