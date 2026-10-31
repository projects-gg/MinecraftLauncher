import { describe, expect, it } from "vitest";
import { DISK_FULL, launchErrorAction, launchErrorCode, NO_ACCOUNT } from "./launch-error";

describe("launchErrorCode", () => {
  it("reads the code from the first line of the details", () => {
    expect(launchErrorCode(`${DISK_FULL}\nnot enough room: 1288490189 bytes needed`)).toBe(
      DISK_FULL,
    );
    expect(launchErrorCode("SURUM-YOK\nMinecraft version 1.99 was not found")).toBe("SURUM-YOK");
    expect(launchErrorCode("SUREC\n")).toBe("SUREC");
    expect(launchErrorCode("AG-YOK")).toBe("AG-YOK");
    // Microsoft's own reason follows the first part as it is.
    expect(launchErrorCode("MSA-invalid_grant\nAADSTS70000: token expired")).toBe(
      "MSA-invalid_grant",
    );
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

  it("offers a new sign-in for an expired session and an account when there is none", () => {
    expect(launchErrorAction("MSA-invalid_grant")).toBe("signIn");
    expect(launchErrorAction("MSA-NOREFRESH")).toBe("signIn");
    expect(launchErrorAction("KC-YOK")).toBe("signIn");
    expect(launchErrorAction(NO_ACCOUNT)).toBe("addAccount");
  });
});
