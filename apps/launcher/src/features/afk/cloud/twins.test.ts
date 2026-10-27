import { describe, expect, it } from "vitest";
import type { AfkAccount, AfkState } from "@/lib/api";
import type { CloudAccount, CloudPageData } from "./api";
import { foldedIds, hereCopy, twinPlace } from "./twins";

const NOW = 1_800_000_000;

function cloud(id: string, overrides: Partial<CloudAccount> = {}): CloudAccount {
  return {
    id,
    name: "Madenci07",
    server: { host: "mc.madenciler.net", port: 25570 },
    desired: "run",
    hold: null,
    expired: false,
    live: { state: "running", since: NOW - 60, reason: null, attempt: null, retryAt: null },
    ...overrides,
  } as CloudAccount;
}

function desktop(id: string, state: AfkState = { kind: "stopped" }): AfkAccount {
  return {
    id,
    nickname: "Madenci07",
    server: "mc.madenciler.net:25570",
    autoStart: false,
    mcVersion: "auto",
    mcVersionRecommended: false,
    state,
  };
}

function page(
  accounts: CloudAccount[],
  twins: Array<{ desktopId: string; cloudId: string }>,
  session: "signedIn" | "signedOut" = "signedIn",
  available = true,
): CloudPageData {
  return { overview: { available, session, accounts }, twins } as unknown as CloudPageData;
}

describe("twins", () => {
  const both = page([cloud("c1"), cloud("c2")], [{ desktopId: "d1", cloudId: "c1" }]);

  it("folds a desktop account into its listed cloud twin, and only then", () => {
    expect([...foldedIds(both)]).toEqual(["d1"]);
    // The service down, signed out, or the twin not listed: every desktop account keeps its card.
    expect(foldedIds(undefined).size).toBe(0);
    expect(foldedIds(page([cloud("c1")], both.twins, "signedOut")).size).toBe(0);
    expect(foldedIds(page([cloud("c1")], both.twins, "signedIn", false)).size).toBe(0);
    expect(foldedIds(page([cloud("c2")], both.twins)).size).toBe(0);
  });

  it("finds the copy on this computer of a cloud account", () => {
    const list = [desktop("d1"), desktop("d2")];
    expect(hereCopy(both, "c1", list)?.id).toBe("d1");
    expect(hereCopy(both, "c2", list)).toBeNull();
    // A twin whose desktop account is gone.
    expect(hereCopy(both, "c1", [desktop("d2")])).toBeNull();
  });

  it("speaks for this computer while its copy is busy or failed there last", () => {
    const stopped = cloud("c1", {
      desired: "stop",
      live: { ...cloud("c1").live, state: "stopped" },
    });
    const busy: AfkState = { kind: "connected", since: new Date(0).toISOString() };
    const failed: AfkState = { kind: "error", reason: "Giriş şifresi yanlış" };
    expect(twinPlace(cloud("c1"), desktop("d1", busy))).toBe("here");
    expect(twinPlace(cloud("c1"), desktop("d1", { kind: "queued", inSecs: 15 }))).toBe("here");
    expect(twinPlace(cloud("c1"), desktop("d1"))).toBe("cloud");
    // Nowhere: the cloud card (its subscription, its start).
    expect(twinPlace(stopped, desktop("d1"))).toBe("cloud");
    // Failed here, the cloud one not asked to run: why it stopped here.
    expect(twinPlace(stopped, desktop("d1", failed))).toBe("here");
    // Running in the cloud, an old failure here says nothing any more.
    expect(twinPlace(cloud("c1"), desktop("d1", failed))).toBe("cloud");
    // The cloud one holds something of its own: that comes first.
    const held = cloud("c1", {
      desired: "stop",
      hold: { kind: "cause", cause: "wrongPassword", at: NOW, row: null },
    });
    expect(twinPlace(held, desktop("d1", failed))).toBe("cloud");
  });
});
