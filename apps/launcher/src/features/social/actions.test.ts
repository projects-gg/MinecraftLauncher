import { beforeEach, describe, expect, it, vi } from "vitest";

// attempt() with its surroundings stubbed: the toast, the error wording and the modules it loads.
const toastError = vi.fn();
vi.mock("@pl/ui", () => ({
  toast: Object.assign(vi.fn(), {
    error: (message: string) => toastError(message),
    success: vi.fn(),
  }),
}));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn(),
  requestPermission: vi.fn(),
  sendNotification: vi.fn(),
}));
vi.mock("@/features/accounts/use-accounts", () => ({
  announce: vi.fn(),
  friendlyError: (_error: unknown, fallback: string) => ({ message: fallback }),
}));
vi.mock("@/lib/api", () => ({ api: {} }));
vi.mock("@/lib/clipboard", () => ({ copyText: vi.fn() }));
vi.mock("@/lib/i18n", () => ({ default: { t: (key: string) => key } }));
vi.mock("@/lib/platform", () => ({ isTauri: () => false }));
vi.mock("@/stores/prefs", () => ({ usePrefs: { getState: () => ({}) } }));
vi.mock("./store", () => ({ useSocialUi: { getState: () => ({}) } }));

const { attempt } = await import("./actions");

describe("attempt", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // Saving the crew, a permission, a moderator, the crew mute, turning the backup off... answer
  // nothing: their success must not read as a failure (the editor stayed open after "Kaydet").
  it("tells a call that answers nothing apart from a failed one", async () => {
    const done = await attempt(async () => {});
    expect(done).not.toBeUndefined();
    expect(toastError).not.toHaveBeenCalled();
  });

  it("passes an answer on as it is", async () => {
    expect(await attempt(async () => "EKP-ABCD-EFGH")).toBe("EKP-ABCD-EFGH");
    expect(await attempt(async () => false)).toBe(false);
    expect(await attempt(async () => null)).toBeNull();
    expect(await attempt(async () => ({ status: "called" }))).toEqual({ status: "called" });
  });

  it("turns a failure into one toast and undefined", async () => {
    const done = await attempt(async () => {
      throw new Error("team_not_permitted");
    });
    expect(done).toBeUndefined();
    expect(toastError).toHaveBeenCalledTimes(1);
    expect(toastError).toHaveBeenCalledWith("social.actions.failed");
  });
});
