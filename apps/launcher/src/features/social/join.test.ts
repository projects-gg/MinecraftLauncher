import { beforeEach, describe, expect, it, vi } from "vitest";

// "Katıl" with its surroundings stubbed: the backend, the launch store and the error toast.
const instances = vi.fn();
const ensureOfficialInstance = vi.fn();
const play = vi.fn(async () => {});
const selectInstance = vi.fn();
const showError = vi.fn();
vi.mock("@/lib/api", () => ({
  api: {
    instances: () => instances(),
    ensureOfficialInstance: () => ensureOfficialInstance(),
  },
}));
vi.mock("@/stores/launch", () => ({
  useLaunch: { getState: () => ({ phase: { kind: "idle" }, play }) },
}));
vi.mock("@/stores/prefs", () => ({ usePrefs: { getState: () => ({ selectInstance }) } }));
vi.mock("@/features/content/jobs", () => ({ showError: (error: unknown) => showError(error) }));
vi.mock("@/lib/i18n", () => ({ default: { t: (key: string) => key } }));

const { joinProjects } = await import("./join");

describe("joinProjects", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("starts the Projects instance straight into the server", async () => {
    instances.mockResolvedValue([
      { id: "vanilla", managed: false, autoJoin: null },
      { id: "projects", managed: true, autoJoin: "play.projects.gg" },
    ]);
    await joinProjects();
    expect(selectInstance).toHaveBeenCalledWith("projects");
    expect(play).toHaveBeenCalledWith("projects", { autoJoin: true });
    expect(showError).not.toHaveBeenCalled();
  });

  it("tells the player why it could not start (E1: no Projects instance and no connection)", async () => {
    const offline = Object.assign(
      new Error("Projects paketi indirilemedi. İnternet bağlantını kontrol edip tekrar dene."),
      { code: "PAKET-AG", details: "request failed" },
    );
    instances.mockResolvedValue([]);
    ensureOfficialInstance.mockRejectedValue(offline);
    await expect(joinProjects()).resolves.toBeUndefined();
    expect(showError).toHaveBeenCalledWith(offline);
    expect(play).not.toHaveBeenCalled();
  });

  it("puts a failure without a message into plain words, keeping the details", async () => {
    instances.mockRejectedValue("ipc channel closed");
    await joinProjects();
    expect(showError).toHaveBeenCalledTimes(1);
    const shown = showError.mock.calls[0]?.[0] as Error & { details?: string };
    expect(shown.message).toBe("eventJoin.failed");
    expect(shown.details).toBe("ipc channel closed");
  });
});
