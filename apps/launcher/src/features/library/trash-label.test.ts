import { describe, expect, it } from "vitest";
import { backupParts, screenshotTakenAt, shotTime, trashLabel } from "./trash-label";

const now = new Date(2026, 9, 17, 12, 0, 0);

describe("screenshotTakenAt", () => {
  it("reads the game's screenshot names", () => {
    expect(screenshotTakenAt("2026-10-14_19.22.11.png")).toEqual(new Date(2026, 9, 14, 19, 22, 11));
    expect(screenshotTakenAt("2026-10-14_19.22.11_2.png")).toEqual(
      new Date(2026, 9, 14, 19, 22, 11),
    );
  });

  it("gives up on other names and impossible dates", () => {
    expect(screenshotTakenAt("kale.png")).toBeUndefined();
    expect(screenshotTakenAt("2026-13-40_19.22.11.png")).toBeUndefined();
  });
});

describe("shotTime", () => {
  it("trusts the name over the last write", () => {
    expect(shotTime({ fileName: "2026-10-16_10.28.19.png", takenAt: "2026-10-17T05:15:00Z" })).toBe(
      new Date(2026, 9, 16, 10, 28, 19).toISOString(),
    );
    expect(shotTime({ fileName: "kale.png", takenAt: "2026-10-17T05:15:00Z" })).toBe(
      "2026-10-17T05:15:00Z",
    );
  });
});

describe("backupParts", () => {
  it("splits the stamp from the world", () => {
    expect(backupParts("2026-10-12_20-11-04_Yeni Dünya.zip")).toEqual({
      world: "Yeni Dünya",
      createdAt: new Date(2026, 9, 12, 20, 11, 4),
    });
  });

  it("keeps a plain name as the world", () => {
    expect(backupParts("Kale.zip")).toEqual({ world: "Kale" });
    expect(backupParts(".zip")).toEqual({});
  });
});

describe("trashLabel", () => {
  it("never shows a file name", () => {
    expect(
      trashLabel(
        { kind: "backup", name: "x", fileName: "2026-10-12_20-11-04_Yeni Dünya.zip" },
        now,
      ),
    ).toBe("Yeni Dünya yedeği");
    expect(
      trashLabel({ kind: "screenshot", name: "x", fileName: "2026-10-17_09.05.00.png" }, now),
    ).toBe("Ekran görüntüsü, Bugün 09:05");
    expect(trashLabel({ kind: "screenshot", name: "x", fileName: "garip.png" }, now)).toBe(
      "Ekran görüntüsü",
    );
  });

  it("names the folders a reinstall set aside", () => {
    expect(trashLabel({ kind: "modSet", name: "26.1.2 mods", fileName: "mods" }, now)).toBe(
      "Eski modlar (26.1.2)",
    );
    expect(trashLabel({ kind: "modSet", name: "1.21.1 config", fileName: "config" }, now)).toBe(
      "Eski mod ayarları (1.21.1)",
    );
  });

  it("names a parked version set by its version", () => {
    expect(trashLabel({ kind: "versionSet", name: "1.17.1", fileName: "1.17.1" }, now)).toBe(
      "1.17.1 sürüm seti",
    );
  });

  it("keeps world names as they are", () => {
    expect(trashLabel({ kind: "world", name: "Zorlu macera", fileName: "Hardcore 2" }, now)).toBe(
      "Zorlu macera",
    );
  });
});
