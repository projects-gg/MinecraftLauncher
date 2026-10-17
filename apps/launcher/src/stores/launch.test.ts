import { beforeEach, describe, expect, it, vi } from "vitest";

// The launch store with its surroundings stubbed: the backend, the AFK check and the UI bits.
const launch = vi.fn(async () => {});
const confirm = vi.fn(async () => true);
vi.mock("@/lib/api", () => ({
  api: {
    launch: (...args: unknown[]) => launch(...(args as [])),
    cancelLaunch: vi.fn(async () => {}),
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

const { useLaunch } = await import("./launch");

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
