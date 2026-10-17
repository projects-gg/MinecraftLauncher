import { describe, expect, it } from "vitest";
import { fileNameOf } from "./paths";

describe("fileNameOf", () => {
  it("reads Windows paths", () => {
    expect(
      fileNameOf(
        "C:\\Users\\Çağrı\\AppData\\Roaming\\projects\\instances\\Survival\\screenshots\\2026-10-17_14.20.11.png",
      ),
    ).toBe("2026-10-17_14.20.11.png");
  });

  it("reads POSIX paths and keeps Turkish letters and spaces", () => {
    expect(
      fileNameOf("/home/şule/.local/share/projects/instances/a/screenshots/gün batımı ışık.png"),
    ).toBe("gün batımı ışık.png");
  });

  it("handles mixed separators, bare names and trailing separators", () => {
    expect(fileNameOf("D:/Oyunlar\\Projects/screenshots\\Ödül töreni.png")).toBe("Ödül töreni.png");
    expect(fileNameOf("İlk gün.png")).toBe("İlk gün.png");
    expect(fileNameOf("C:\\screenshots\\")).toBe("screenshots");
    expect(fileNameOf("")).toBe("");
  });
});
