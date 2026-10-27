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
  useLaunch: { getState: () => ({ phase: { kind: "idle" }, games: [], play }) },
}));
let selectedInstanceId: string | null = null;
vi.mock("@/stores/prefs", () => ({
  usePrefs: { getState: () => ({ selectInstance, selectedInstanceId }) },
}));
vi.mock("@/features/content/jobs", () => ({ showError: (error: unknown) => showError(error) }));
vi.mock("@/lib/i18n", () => ({ default: { t: (key: string) => key } }));

const { joinProjects, joinServer } = await import("./join");

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

describe("joinServer", () => {
  const server = { host: "survival.example.net", port: 25570 };
  beforeEach(() => {
    vi.clearAllMocks();
    selectedInstanceId = null;
  });

  it("starts the instance OYNA would start, straight into the inviter's server", async () => {
    selectedInstanceId = "fabric-1-21";
    instances.mockResolvedValue([{ id: "projects" }, { id: "fabric-1-21" }]);
    await joinServer(server);
    expect(selectInstance).toHaveBeenCalledWith("fabric-1-21");
    expect(play).toHaveBeenCalledWith("fabric-1-21", {
      autoJoin: false,
      quickPlay: { kind: "multiplayer", host: "survival.example.net", port: 25570 },
    });
    expect(ensureOfficialInstance).not.toHaveBeenCalled();
  });

  it("falls back to the Projects instance, as OYNA does", async () => {
    selectedInstanceId = "deleted";
    instances.mockResolvedValue([{ id: "vanilla" }]);
    ensureOfficialInstance.mockResolvedValue({ id: "projects" });
    await joinServer({ host: "survival.example.net", port: null });
    expect(play).toHaveBeenCalledWith("projects", {
      autoJoin: false,
      quickPlay: { kind: "multiplayer", host: "survival.example.net", port: null },
    });
  });

  it("says why it could not start", async () => {
    instances.mockRejectedValue("ipc channel closed");
    await joinServer(server);
    expect(showError).toHaveBeenCalledTimes(1);
    expect(play).not.toHaveBeenCalled();
  });
});
