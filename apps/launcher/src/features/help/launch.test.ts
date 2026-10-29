import { describe, expect, it } from "vitest";
import { helpsWithLaunchError, helpToastLine } from "./launch";

describe("helpsWithLaunchError", () => {
  it("leaves the errors that say their own fix alone", () => {
    expect(helpsWithLaunchError("DISK-DOLU")).toBe(false);
    expect(helpsWithLaunchError("OYUN")).toBe(true);
  });
});

describe("helpToastLine", () => {
  it("drops the closing try-again beside 'Sorunu bul'", () => {
    expect(helpToastLine("Oyun hazırlanırken bir sorun oluştu. Tekrar dene.")).toBe(
      "Oyun hazırlanırken bir sorun oluştu.",
    );
    expect(helpToastLine("Something went wrong while getting the game ready. Try again.")).toBe(
      "Something went wrong while getting the game ready.",
    );
  });

  it("keeps a message that ends otherwise", () => {
    const own = "Bu Minecraft sürümü bulunamadı. Kurulumun sürümünü değiştir.";
    expect(helpToastLine(own)).toBe(own);
  });

  it("keeps a message that is nothing but the try-again", () => {
    expect(helpToastLine("Tekrar dene.")).toBe("Tekrar dene.");
  });
});
