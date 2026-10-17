import { describe, expect, it, vi } from "vitest";

// Only the pure rules are tested here; their surroundings are stubbed.
vi.mock("@pl/ui", () => ({ toast: Object.assign(vi.fn(), { success: vi.fn(), error: vi.fn() }) }));
vi.mock("@/lib/api", () => ({ api: { afkAccounts: vi.fn(async () => []) } }));
vi.mock("@/lib/i18n", () => ({ default: { t: (key: string) => key } }));
vi.mock("@/stores/launch", () => ({
  useLaunch: { getState: () => ({ phase: { kind: "idle" } }) },
}));
vi.mock("@/stores/update", () => ({ useUpdate: { getState: vi.fn(), setState: vi.fn() } }));

const { installDecision, isAfkOpen, isGameBusy, pillView } = await import("./install");
type PillInput = Parameters<typeof pillView>[0];

const base: PillInput = {
  phase: "idle",
  version: "2.5.0",
  mandatory: false,
  manual: false,
  percent: 0,
  step: null,
  gameBusy: false,
};
const view = (change: Partial<PillInput>) => pillView({ ...base, ...change });

describe("pillView", () => {
  it("restarts into a downloaded update, but never under a game", () => {
    expect(view({ phase: "ready" })).toEqual({ kind: "ready" });
    expect(view({ phase: "ready", gameBusy: true })).toEqual({ kind: "afterGame" });
  });

  it("names the step while installing", () => {
    expect(view({ phase: "applying", step: "stoppingAfk" })).toEqual({
      kind: "installing",
      step: "stoppingAfk",
    });
    expect(view({ phase: "applying" })).toEqual({ kind: "installing", step: "installing" });
  });

  it("shows only the downloads the player asked for", () => {
    expect(view({ phase: "downloading", percent: 41.6 })).toEqual({ kind: "hidden" });
    expect(view({ phase: "downloading", manual: true, percent: 41.6 })).toEqual({
      kind: "downloading",
      percent: 42,
    });
    // "Güncelleme var" pressed: no gap while the download is looked up.
    expect(view({ phase: "checking", manual: true })).toEqual({ kind: "downloading", percent: 0 });
    expect(view({ phase: "checking", manual: true, version: null })).toEqual({ kind: "hidden" });
  });

  it("offers an update that is not downloaded, also after a failed download", () => {
    expect(view({ phase: "available" })).toEqual({ kind: "available" });
    expect(view({ phase: "error" })).toEqual({ kind: "available" });
    expect(view({ phase: "error", version: null })).toEqual({ kind: "hidden" });
  });

  it("stays out of the way otherwise and under the mandatory screen", () => {
    expect(view({ phase: "idle" })).toEqual({ kind: "hidden" });
    expect(view({ phase: "checking" })).toEqual({ kind: "hidden" });
    expect(view({ phase: "ready", mandatory: true })).toEqual({ kind: "hidden" });
  });
});

describe("installDecision", () => {
  const ready = { phase: "ready" as const, asking: false, gameBusy: false, openAfk: 0 };

  it("installs at once without open AFK accounts", () => {
    expect(installDecision(ready)).toBe("install");
  });

  it("asks first when AFK accounts are open", () => {
    expect(installDecision({ ...ready, openAfk: 2 })).toBe("ask");
  });

  it("does nothing while a game is open, a question is open or nothing is ready", () => {
    expect(installDecision({ ...ready, gameBusy: true })).toBe("ignore");
    expect(installDecision({ ...ready, asking: true, openAfk: 1 })).toBe("ignore");
    expect(installDecision({ ...ready, phase: "applying" })).toBe("ignore");
    expect(installDecision({ ...ready, phase: "available" })).toBe("ignore");
  });
});

describe("helpers", () => {
  it("count a game being prepared as busy", () => {
    expect(["working", "launching", "running"].every(isGameBusy)).toBe(true);
    expect(["idle", "error"].some(isGameBusy)).toBe(false);
  });

  it("count queued and reconnecting AFK accounts as open", () => {
    const afk = (kind: string) => ({ state: { kind } }) as Parameters<typeof isAfkOpen>[0];
    expect(isAfkOpen(afk("connected"))).toBe(true);
    expect(isAfkOpen(afk("queued"))).toBe(true);
    expect(isAfkOpen(afk("reconnecting"))).toBe(true);
    expect(isAfkOpen(afk("stopped"))).toBe(false);
    expect(isAfkOpen(afk("stopping"))).toBe(false);
    expect(isAfkOpen(afk("error"))).toBe(false);
  });
});
