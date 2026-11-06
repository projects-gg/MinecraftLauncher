import { describe, expect, it, vi } from "vitest";

const toast = vi.fn();
vi.mock("@pl/ui", () => ({ toast: (message: string) => toast(message) }));
vi.mock("@/lib/i18n", () => ({ default: { t: (key: string) => key } }));
vi.mock("@/lib/api/discover-api", () => ({ discoverApi: {} }));

const { discoverOff, useListing } = await import("./listing");

describe("discoverOff", () => {
  it("tells Studio's switch going off calmly, and the page shows it is off", () => {
    useListing.setState({
      kept: { listing: { crews: [] }, etag: '"d1"', fetchedAt: 1 } as never,
      state: "ready",
    });
    const off = Object.assign(new Error("off"), { code: "SOSYAL-KESIF-KAPALI" });
    expect(discoverOff(off)).toBe(true);
    // A plain toast: no error mark.
    expect(toast).toHaveBeenCalledWith("discover.off");
    expect(useListing.getState()).toMatchObject({ kept: null, state: "off" });
    // Any other failure is the caller's error toast.
    expect(discoverOff(new Error("team_not_permitted"))).toBe(false);
    expect(toast).toHaveBeenCalledTimes(1);
  });
});
