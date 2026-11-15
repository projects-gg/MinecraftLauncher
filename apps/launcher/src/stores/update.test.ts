import { beforeEach, describe, expect, it, vi } from "vitest";

// The update store against a stubbed backend: commands, the IPC channel and the preferences.
type Listener = ((message: unknown) => void) | null;
const channels: { onmessage: Listener }[] = [];
const updateApply = vi.fn();
const updateCheck = vi.fn();
const updateDownload = vi.fn();
const prefs = { updateChannel: "stable", autoDownloadUpdates: true };
vi.mock("@/bindings", () => ({
  commands: {
    updateApply: (...args: unknown[]) => updateApply(...args),
    updateCheck: (...args: unknown[]) => updateCheck(...args),
    updateDownload: (...args: unknown[]) => updateDownload(...args),
  },
}));
vi.mock("@tauri-apps/api/core", () => ({
  Channel: class {
    onmessage: Listener = null;
    constructor() {
      channels.push(this);
    }
  },
}));
vi.mock("@/lib/platform", () => ({ isTauri: () => true }));
vi.mock("./prefs", () => ({ usePrefs: { getState: () => prefs } }));
// BETA-GATE: the answer about the beta channel (beta-gate.test.ts covers it).
const keepBeta = vi.fn();
vi.mock("@/features/updates/beta-gate", () => ({ keepBeta: () => keepBeta() }));

/** The first channel the store opened in this test (progress for a check, steps for an install). */
function channel() {
  const first = channels[0];
  if (!first) throw new Error("no channel opened");
  return first;
}

/** localStorage as a map. */
function memoryStorage() {
  const map = new Map<string, string>();
  return {
    map,
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => void map.set(key, value),
    removeItem: (key: string) => void map.delete(key),
  };
}

const storage = memoryStorage();
vi.stubGlobal("localStorage", storage);
vi.stubGlobal("window", { location: { hash: "#/afk" } });

const { takePendingNotes, useUpdate } = await import("./update");
const initial = useUpdate.getState();

beforeEach(() => {
  useUpdate.setState({ ...initial, phase: "ready", version: "2.5.0", notes: "- yeni" }, true);
  channels.length = 0;
  storage.map.clear();
  updateApply.mockReset();
  updateCheck.mockReset();
  updateDownload.mockReset();
  prefs.autoDownloadUpdates = true;
});

/** The look at the channel before an install is over (it is asked first, one tick away). */
const looked = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("apply", () => {
  it("sends the page the player is on and follows the backend's steps", async () => {
    let finish: (value: unknown) => void = () => {};
    updateApply.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const done = useUpdate.getState().apply({ afkOpen: true });
    expect(useUpdate.getState()).toMatchObject({ phase: "applying", step: "stoppingAfk" });
    await looked();
    expect(updateApply).toHaveBeenCalledWith("/afk", channel());
    channel().onmessage?.("installing");
    expect(useUpdate.getState().step).toBe("installing");
    finish({
      status: "error",
      error: { code: "GUNCELLEME-KURULUM", message: "Güncelleme kurulamadı. Tekrar dene." },
    });
    await done;
    // Kept for "Tekrar dene".
    expect(useUpdate.getState()).toMatchObject({
      phase: "ready",
      step: null,
      error: "Güncelleme kurulamadı. Tekrar dene.",
    });
  });

  it("downloads again when the update is gone", async () => {
    updateApply.mockResolvedValue({
      status: "error",
      error: { code: "GUNCELLEME-HAZIR-DEGIL", message: "Hazır bir güncelleme yok." },
    });
    await useUpdate.getState().apply();
    expect(useUpdate.getState().phase).toBe("error");
  });

  it("takes one press only", async () => {
    updateApply.mockImplementation(() => new Promise(() => {}));
    void useUpdate.getState().apply();
    void useUpdate.getState().apply();
    await looked();
    expect(updateApply).toHaveBeenCalledTimes(1);
    useUpdate.setState({ phase: "available" });
    await useUpdate.getState().apply();
    expect(updateApply).toHaveBeenCalledTimes(1);
  });

  it("keeps a copy of the notes for after the restart", async () => {
    updateApply.mockImplementation(() => new Promise(() => {}));
    void useUpdate.getState().apply();
    await looked();
    expect(JSON.parse(storage.map.get("pl.pendingNotes") ?? "null")).toEqual({
      version: "2.5.0",
      notes: "- yeni",
    });
  });
});

describe("check", () => {
  const found = { status: "ok", data: { state: "available", version: "2.6.0", notes: null } };

  it("asks first on the beta channel and looks where the answer says (BETA-GATE)", async () => {
    useUpdate.setState({ phase: "idle" });
    updateCheck.mockResolvedValue({ status: "ok", data: { state: "upToDate", current: "2.5.0" } });
    keepBeta.mockReset();
    // Kararlı asks nobody.
    prefs.updateChannel = "stable";
    await useUpdate.getState().check();
    expect(keepBeta).not.toHaveBeenCalled();
    expect(updateCheck).toHaveBeenLastCalledWith("stable");
    // Beta asks; a yes looks at beta, a no at Kararlı.
    prefs.updateChannel = "beta";
    keepBeta.mockResolvedValueOnce("beta");
    await useUpdate.getState().check();
    expect(updateCheck).toHaveBeenLastCalledWith("beta");
    keepBeta.mockResolvedValueOnce("stable");
    await useUpdate.getState().check();
    expect(keepBeta).toHaveBeenCalledTimes(2);
    expect(updateCheck).toHaveBeenLastCalledWith("stable");
    prefs.updateChannel = "stable";
  });

  it("offers an update without downloading it when background downloads are off", async () => {
    prefs.autoDownloadUpdates = false;
    useUpdate.setState({ phase: "idle" });
    updateCheck.mockResolvedValue(found);
    expect(await useUpdate.getState().check()).toBe("available");
    expect(useUpdate.getState().phase).toBe("available");
    expect(updateDownload).not.toHaveBeenCalled();
  });

  it("keeps the offer in place during a later background check", async () => {
    prefs.autoDownloadUpdates = false;
    useUpdate.setState({ phase: "available" });
    const phases: string[] = [];
    const stop = useUpdate.subscribe((s) => phases.push(s.phase));
    updateCheck.mockResolvedValue(found);
    await useUpdate.getState().check();
    stop();
    expect(phases).not.toContain("checking");
    expect(useUpdate.getState().phase).toBe("available");
  });

  it("downloads on request and reports whole percents only", async () => {
    prefs.autoDownloadUpdates = false;
    useUpdate.setState({ phase: "available", percent: 0 });
    updateCheck.mockResolvedValue(found);
    const seen: number[] = [];
    updateDownload.mockImplementation(async () => {
      const stop = useUpdate.subscribe((s) => seen.push(s.percent));
      for (const downloaded of [100, 200, 900, 1000, 1500]) {
        channel().onmessage?.({ downloaded, total: 100_000, done: false });
      }
      stop();
      return { status: "ok", data: true };
    });
    expect(await useUpdate.getState().check(true)).toBe("ready");
    expect(seen).toEqual([1]);
    expect(useUpdate.getState()).toMatchObject({ phase: "ready", manual: true });
  });
});

describe("a downloaded update the channel left behind", () => {
  const offer = (version: string, state = "available") => ({
    status: "ok",
    data: { state, current: "2.2.1", version, notes: null },
  });
  const waiting = () =>
    useUpdate.setState({ phase: "ready", version: "2.2.2", notes: null, mandatory: false });

  it("downloads the channel's version over it at the next look", async () => {
    waiting();
    updateCheck.mockResolvedValue(offer("2.2.3"));
    updateDownload.mockResolvedValue({ status: "ok", data: true });
    expect(await useUpdate.getState().check()).toBe("ready");
    expect(updateDownload).toHaveBeenCalledTimes(1);
    expect(useUpdate.getState()).toMatchObject({ phase: "ready", version: "2.2.3" });
  });

  it("installs the channel's version, never the old download", async () => {
    waiting();
    updateCheck.mockResolvedValue(offer("2.2.3"));
    updateDownload.mockResolvedValue({ status: "ok", data: true });
    updateApply.mockImplementation(async () => {
      expect(useUpdate.getState().version).toBe("2.2.3");
      return { status: "ok", data: null };
    });
    await useUpdate.getState().apply();
    expect(updateDownload).toHaveBeenCalledTimes(1);
    expect(updateApply).toHaveBeenCalledTimes(1);
  });

  it("drops it when the rollout stopped, keeps it when the channel cannot be read", async () => {
    waiting();
    updateCheck.mockResolvedValue(offer("2.2.2", "unavailable"));
    expect(await useUpdate.getState().check()).toBe("ready");
    updateCheck.mockResolvedValue(offer("2.2.2", "heldBack"));
    await useUpdate.getState().apply();
    expect(updateApply).not.toHaveBeenCalled();
    expect(useUpdate.getState()).toMatchObject({ phase: "idle", version: null });
  });
});

describe("takePendingNotes", () => {
  it("shows the notes once, for their version", () => {
    storage.setItem("pl.pendingNotes", JSON.stringify({ version: "2.5.0", notes: "- yeni" }));
    expect(takePendingNotes("2.4.0", storage)).toBeNull();
    expect(storage.map.has("pl.pendingNotes")).toBe(true);
    expect(takePendingNotes("2.5.0", storage)).toEqual({ version: "2.5.0", notes: "- yeni" });
    expect(takePendingNotes("2.5.0", storage)).toBeNull();
  });

  it("drops what can never show", () => {
    storage.setItem("pl.pendingNotes", "{ not json");
    expect(takePendingNotes("2.5.0", storage)).toBeNull();
    expect(storage.map.has("pl.pendingNotes")).toBe(false);
    storage.setItem("pl.pendingNotes", JSON.stringify({ version: "2.5.0", notes: " " }));
    expect(takePendingNotes("2.5.0", storage)).toBeNull();
    expect(storage.map.has("pl.pendingNotes")).toBe(false);
  });
});
