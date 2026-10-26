import { beforeEach, describe, expect, it, vi } from "vitest";

// BETA-GATE: picking Beta and staying on it, against a stubbed backend, preferences and toast.
const updateBetaJoin = vi.fn();
const updateBetaKeep = vi.fn();
vi.mock("@/bindings", () => ({
  commands: {
    updateBetaJoin: () => updateBetaJoin(),
    updateBetaKeep: () => updateBetaKeep(),
  },
}));
vi.mock("@/lib/platform", () => ({ isTauri: () => true }));
const prefs = {
  updateChannel: "stable" as "stable" | "beta",
  set: vi.fn((key: string, value: "stable" | "beta") => {
    if (key === "updateChannel") prefs.updateChannel = value;
  }),
};
vi.mock("@/stores/prefs", () => ({ usePrefs: { getState: () => prefs } }));
const toast = vi.fn();
const warning = vi.fn();
vi.mock("@pl/ui", () => ({
  toast: Object.assign((...args: unknown[]) => toast(...args), {
    warning: (...args: unknown[]) => warning(...args),
  }),
}));
vi.mock("@/lib/i18n", () => ({ default: { t: (key: string) => key } }));
const check = vi.fn();
vi.mock("@/stores/update", () => ({ useUpdate: { getState: () => ({ check }) } }));
// The friends store as the watcher sees it: subscribe, and a way to change who is signed in.
type Social = { view: { auth: string } };
const socialListeners = new Set<(state: Social, previous: Social) => void>();
let social: Social = { view: { auth: "signedOut" } };
vi.mock("@/features/social/store", () => ({
  useSocialUi: {
    subscribe: (listener: (state: Social, previous: Social) => void) => {
      socialListeners.add(listener);
      return () => socialListeners.delete(listener);
    },
  },
}));
function socialAuth(auth: string) {
  const previous = social;
  social = { view: { auth } };
  for (const listener of [...socialListeners]) listener(social, previous);
}

const { keepBeta, useBetaGate } = await import("./beta-gate");

const APPLY = "https://projects.gg/basvuru";
const ok = (verdict: string, apply = APPLY) => ({ status: "ok", data: { verdict, apply } });

beforeEach(() => {
  updateBetaJoin.mockReset();
  updateBetaKeep.mockReset();
  toast.mockReset();
  warning.mockReset();
  prefs.set.mockClear();
  prefs.updateChannel = "stable";
  useBetaGate.setState({ asking: false, refusal: null, rowsShown: 0 });
  // A watcher left by an earlier test signs in and stops (no refusal now: it asks nothing).
  socialAuth("signedOut");
  socialAuth("signedIn");
  socialAuth("signedOut");
  check.mockReset();
});

describe("join", () => {
  it("switches to Beta on a yes", async () => {
    updateBetaJoin.mockResolvedValue(ok("allowed"));
    expect(await useBetaGate.getState().join()).toBe(true);
    expect(prefs.updateChannel).toBe("beta");
    expect(useBetaGate.getState()).toMatchObject({ asking: false, refusal: null });
  });

  it("stays on Kararlı and keeps why on a no", async () => {
    for (const verdict of ["notMember", "signIn", "unknown"]) {
      updateBetaJoin.mockResolvedValue(ok(verdict, "https://projects.gg/arge"));
      expect(await useBetaGate.getState().join()).toBe(false);
      expect(prefs.updateChannel).toBe("stable");
      expect(useBetaGate.getState().refusal).toEqual({
        verdict,
        apply: "https://projects.gg/arge",
      });
    }
    expect(toast).not.toHaveBeenCalled();
  });

  it("asks once while the answer is out, and forgets it when Kararlı is picked meanwhile", async () => {
    let answer: (value: unknown) => void = () => {};
    updateBetaJoin.mockImplementation(
      () =>
        new Promise((resolve) => {
          answer = resolve;
        }),
    );
    const first = useBetaGate.getState().join();
    expect(useBetaGate.getState().asking).toBe(true);
    expect(await useBetaGate.getState().join()).toBe(false);
    expect(updateBetaJoin).toHaveBeenCalledTimes(1);
    useBetaGate.getState().clear();
    answer(ok("allowed"));
    expect(await first).toBe(false);
    expect(prefs.updateChannel).toBe("stable");
    expect(useBetaGate.getState()).toMatchObject({ asking: false, refusal: null });
  });

  it("asks again by itself once the friends feature signs in", async () => {
    updateBetaJoin.mockResolvedValue(ok("signIn"));
    expect(await useBetaGate.getState().join()).toBe(false);
    await vi.waitFor(() => expect(socialListeners.size).toBe(1));
    updateBetaJoin.mockResolvedValue(ok("allowed"));
    // Still signing in: nothing yet.
    socialAuth("signingIn");
    expect(updateBetaJoin).toHaveBeenCalledTimes(1);
    socialAuth("signedIn");
    await vi.waitFor(() => expect(check).toHaveBeenCalledTimes(1));
    expect(updateBetaJoin).toHaveBeenCalledTimes(2);
    expect(prefs.updateChannel).toBe("beta");
    expect(socialListeners.size).toBe(0);
    // Signed in all along afterwards (no change into "signed in"): never asked again.
    socialAuth("signedIn");
    expect(updateBetaJoin).toHaveBeenCalledTimes(2);
  });

  it("cannot ask when the command itself fails", async () => {
    updateBetaJoin.mockRejectedValue(new Error("ipc"));
    expect(await useBetaGate.getState().join()).toBe(false);
    expect(useBetaGate.getState().refusal?.verdict).toBe("unknown");
  });
});

describe("keepBeta", () => {
  it("stays on Beta on a yes, quietly", async () => {
    prefs.updateChannel = "beta";
    updateBetaKeep.mockResolvedValue(ok("allowed"));
    expect(await keepBeta()).toBe("beta");
    expect(prefs.set).not.toHaveBeenCalled();
    expect(toast).not.toHaveBeenCalled();
  });

  it("moves to Kararlı on a no and says so once; what to do stays in Settings' line", async () => {
    prefs.updateChannel = "beta";
    updateBetaKeep.mockResolvedValue(ok("notMember", "https://projects.gg/arge"));
    expect(await keepBeta()).toBe("stable");
    expect(prefs.updateChannel).toBe("stable");
    expect(useBetaGate.getState().refusal).toEqual({
      verdict: "notMember",
      apply: "https://projects.gg/arge",
    });
    // A toast only tells: no button (a second gold thing over OYNA, its action is in the line).
    expect(toast).toHaveBeenCalledTimes(1);
    expect(toast.mock.calls[0]).toEqual(["betaGate.movedNotMember"]);

    toast.mockReset();
    prefs.updateChannel = "beta";
    updateBetaKeep.mockResolvedValue(ok("signIn"));
    expect(await keepBeta()).toBe("stable");
    expect(toast.mock.calls[0]).toEqual(["betaGate.movedSignIn"]);

    // "Couldn't check": the warning state.
    toast.mockReset();
    prefs.updateChannel = "beta";
    updateBetaKeep.mockResolvedValue(ok("unknown"));
    expect(await keepBeta()).toBe("stable");
    expect(toast).not.toHaveBeenCalled();
    expect(warning.mock.calls[0]).toEqual(["betaGate.movedUnknown"]);
  });

  it("leaves the toast to Settings' line while the channel row is on screen", async () => {
    useBetaGate.setState({ rowsShown: 1 });
    prefs.updateChannel = "beta";
    updateBetaKeep.mockResolvedValue(ok("notMember"));
    expect(await keepBeta()).toBe("stable");
    expect(useBetaGate.getState().refusal?.verdict).toBe("notMember");
    expect(toast).not.toHaveBeenCalled();
    expect(warning).not.toHaveBeenCalled();
  });

  it("stays where it is when the command itself fails", async () => {
    prefs.updateChannel = "beta";
    updateBetaKeep.mockResolvedValue({ status: "error", error: { code: "X", message: "x" } });
    expect(await keepBeta()).toBe("beta");
    updateBetaKeep.mockRejectedValue(new Error("ipc"));
    expect(await keepBeta()).toBe("beta");
    expect(prefs.updateChannel).toBe("beta");
    expect(toast).not.toHaveBeenCalled();
  });
});
