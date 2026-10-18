import { describe, expect, it } from "vitest";
import { blurhashUrl, decodeBlurhash } from "./blurhash";

describe("BlurHash placeholders", () => {
  it("decodes a one-colour hash to that colour everywhere", () => {
    // One component, DC = #FF0000.
    const pixels = decodeBlurhash("00TI:j", 4, 3);
    expect(pixels).not.toBeNull();
    expect(pixels?.length).toBe(4 * 3 * 4);
    for (let i = 0; i < 12; i++) {
      expect(Array.from(pixels?.slice(i * 4, i * 4 + 4) ?? [])).toEqual([255, 0, 0, 255]);
    }
  });

  it("decodes the reference hash with every pixel opaque", () => {
    const pixels = decodeBlurhash("LEHV6nWB2yk8pyo0adR*.7kCMdnj", 32, 18);
    expect(pixels?.length).toBe(32 * 18 * 4);
    for (let i = 3; i < (pixels?.length ?? 0); i += 4) expect(pixels?.[i]).toBe(255);
    // A bot cover's hash (MCN1 dry run) reads too.
    expect(decodeBlurhash("LZFzA:9^I==v}rI=NbxDxGW;R+WV", 32, 18)).not.toBeNull();
  });

  it("refuses what is not a BlurHash", () => {
    for (const hash of [
      "",
      "00TI:",
      "LEHV6nWB2yk8pyo0adR*.7kCMdn",
      "LEHV6nWB2yk8pyo0adR*.7kCMdnjj",
      "LEHV6nWB2yk8pyo0adR*.7kCMdnç",
      "00TI j",
    ]) {
      expect(decodeBlurhash(hash, 4, 3), hash).toBeNull();
    }
  });

  it("gives no placeholder without a hash or a canvas", () => {
    expect(blurhashUrl(undefined)).toBeUndefined();
    expect(blurhashUrl("")).toBeUndefined();
    // Tests run without a DOM: no canvas, no picture, and no error either.
    expect(blurhashUrl("00TI:j")).toBeUndefined();
  });
});
