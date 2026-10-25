// @vitest-environment happy-dom
// ItemSprite in the browser: a 3D icon 48 device px wide waits for the large atlas image, showing
// its 32 px drawing meanwhile and after a failed load, and every sprite shares one load.
import {
  type ItemArt,
  ItemSprite,
  VANILLA_INDEX_FORMAT,
  type VanillaAtlasIndex,
} from "@pl/ui/items";
import { act, cleanup, render } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";

const INDEX: VanillaAtlasIndex = {
  format: VANILLA_INDEX_FORMAT,
  minecraft: "26.3",
  image: "atlas.png",
  size: [256, 128],
  large: { image: "atlas-48.png", size: [128, 64], icon: 48 },
  icons: {
    "minecraft:stone": {
      kind: "block",
      layers: [{ rect: [16, 0, 32, 32] }],
      large: [49, 0, 48, 48],
    },
  },
};

function art(largeUrl: string): ItemArt {
  return { vanilla: { index: INDEX, imageUrl: "atlas.png", largeUrl } };
}

/** Image loads the test settles by hand. */
const loads: { url: string; settle: (ok: boolean) => void }[] = [];

class FakeImage {
  crossOrigin: string | null = null;
  src = "";
  decode(): Promise<void> {
    return new Promise((resolve, reject) => {
      loads.push({ url: this.src, settle: (ok) => (ok ? resolve() : reject(new Error("no"))) });
    });
  }
}

beforeAll(() => vi.stubGlobal("Image", FakeImage));
afterAll(() => vi.unstubAllGlobals());
afterEach(cleanup);

const large = (container: HTMLElement, url: string) =>
  container.querySelectorAll(`img[src="${url}"]`).length;

describe("the large drawing", () => {
  it("shows the 32 px drawing until the large image has loaded, then the 48 px one", async () => {
    const a = art("ok-48.png");
    const { container } = render(
      <>
        <ItemSprite id="stone" size={48} art={a} />
        <ItemSprite id="stone" size={48} art={a} />
      </>,
    );
    expect(large(container, "ok-48.png")).toBe(0);
    expect(container.querySelectorAll('img[src="atlas.png"]')).toHaveLength(2);
    // One load for both sprites, as a CORS request.
    const mine = loads.filter((l) => l.url === "ok-48.png");
    expect(mine).toHaveLength(1);
    await act(async () => mine[0]?.settle(true));
    expect(large(container, "ok-48.png")).toBe(2);
    expect(container.querySelector('img[src="atlas.png"]')).toBeNull();
  });

  it("keeps the 32 px drawing when the large image fails", async () => {
    const { container } = render(<ItemSprite id="stone" size={48} art={art("bad-48.png")} />);
    await act(async () => loads.find((l) => l.url === "bad-48.png")?.settle(false));
    expect(large(container, "bad-48.png")).toBe(0);
    expect(container.querySelector('img[src="atlas.png"]')).not.toBeNull();
  });

  it("loads nothing for sprites that do not need it", () => {
    render(<ItemSprite id="stone" size={32} art={art("unused-48.png")} />);
    expect(loads.some((l) => l.url === "unused-48.png")).toBe(false);
  });
});
