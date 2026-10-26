import { isValidElement, type ReactElement } from "react";
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
const check = vi.fn();
vi.mock("@/stores/update", () => ({ useUpdate: { getState: () => ({ check }) } }));
vi.mock("@/lib/i18n", () => ({ default: { t: (key: string) => key } }));
const openWebLink = vi.fn();
vi.mock("@/lib/links", () => ({ openWebLink: (href: string) => openWebLink(href) }));

const { keepBeta, useBetaGate } = await import("./beta-gate");

const APPLY = "https://projects.gg/basvuru";
const ok = (verdict: string, apply = APPLY) => ({ status: "ok", data: { verdict, apply } });

beforeEach(() => {
  updateBetaJoin.mockReset();
  updateBetaKeep.mockReset();
  toast.mockReset();
  warning.mockReset();
  check.mockReset();
  openWebLink.mockReset();
  prefs.set.mockClear();
  prefs.updateChannel = "stable";
  useBetaGate.setState({ asking: false, refusal: null });
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

  it("moves to Kararlı on a no, says so once and offers what to do", async () => {
    prefs.updateChannel = "beta";
    updateBetaKeep.mockResolvedValue(ok("notMember", "https://projects.gg/arge"));
    expect(await keepBeta()).toBe("stable");
    expect(prefs.updateChannel).toBe("stable");
    expect(useBetaGate.getState().refusal?.verdict).toBe("notMember");
    expect(toast).toHaveBeenCalledTimes(1);
    const [message, options] = toast.mock.calls[0] as [
      string,
      { action: { label: ReactElement<{ children: unknown[] }>; onClick: () => void } },
    ];
    expect(message).toBe("betaGate.movedNotMember");
    // The button's words, then the external-link mark (it opens the browser).
    expect(isValidElement(options.action.label)).toBe(true);
    expect(options.action.label.props.children[0]).toBe("betaGate.apply");
    options.action.onClick();
    expect(openWebLink).toHaveBeenCalledWith("https://projects.gg/arge");

    // "Couldn't check": the warning state and "Tekrar dene", which asks again (Beta on a yes).
    toast.mockReset();
    prefs.updateChannel = "beta";
    updateBetaKeep.mockResolvedValue(ok("unknown"));
    expect(await keepBeta()).toBe("stable");
    expect(toast).not.toHaveBeenCalled();
    const [failed, retry] = warning.mock.calls[0] as [
      string,
      { action: { label: string; onClick: () => void } },
    ];
    expect(failed).toBe("betaGate.movedUnknown");
    expect(retry.action.label).toBe("betaGate.retry");
    updateBetaJoin.mockResolvedValue(ok("allowed"));
    retry.action.onClick();
    await vi.waitFor(() => expect(check).toHaveBeenCalledTimes(1));
    expect(prefs.updateChannel).toBe("beta");
    toast.mockReset();
    prefs.updateChannel = "beta";
    updateBetaKeep.mockResolvedValue(ok("signIn"));
    expect(await keepBeta()).toBe("stable");
    expect(toast.mock.calls[0]?.[0]).toBe("betaGate.movedSignIn");
    const signIn = toast.mock.calls[0]?.[1] as { action: { label: string } };
    expect(signIn.action.label).toBe("betaGate.signInShort");
  });

  it("leaves the toast to Settings' line while the channel row is on screen", async () => {
    useBetaGate.setState({ rowsShown: 1 });
    prefs.updateChannel = "beta";
    updateBetaKeep.mockResolvedValue(ok("notMember"));
    expect(await keepBeta()).toBe("stable");
    expect(useBetaGate.getState().refusal?.verdict).toBe("notMember");
    expect(toast).not.toHaveBeenCalled();
    expect(warning).not.toHaveBeenCalled();
    useBetaGate.setState({ rowsShown: 0 });
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
