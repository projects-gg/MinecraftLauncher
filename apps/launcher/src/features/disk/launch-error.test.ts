import { describe, expect, it } from "vitest";
import { DISK_FULL, launchErrorAction, launchErrorCode } from "./launch-error";

describe("launchErrorCode", () => {
  it("reads the code from the first line of the details", () => {
    expect(launchErrorCode(`${DISK_FULL}\nnot enough room: 1288490189 bytes needed`)).toBe(
      DISK_FULL,
    );
    expect(launchErrorCode("SURUM-YOK\nMinecraft version 1.99 was not found")).toBe("SURUM-YOK");
    expect(launchErrorCode("SUREC\n")).toBe("SUREC");
    expect(launchErrorCode("AG-YOK")).toBe("AG-YOK");
  });

  it("finds no code in other details", () => {
    expect(launchErrorCode("")).toBe("");
    expect(launchErrorCode("I/O error at /data/a.part: (os error 112)")).toBe("");
    expect(launchErrorCode("launch cancelled")).toBe("");
    expect(launchErrorCode("\nDISK-DOLU")).toBe("");
  });
});

describe("launchErrorAction", () => {
  it("offers room for a full disk and the details for the rest", () => {
    expect(launchErrorAction(DISK_FULL)).toBe("makeSpace");
    for (const code of ["DISK", "SURUM-YOK", "SUREC", "BULUNAMADI", "OYUN", "PAKET", ""]) {
      expect(launchErrorAction(code)).toBe("copyDetails");
    }
  });
});
