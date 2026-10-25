import {
  ALWAYS_GLINT,
  fullItemId,
  hasGlint,
  hostMatches,
  type ItemArt,
  type ItemArtDoc,
  ItemSprite,
  itemCategory,
  layerColor,
  loadVanillaAtlas,
  POTION_COLORS,
  pickDrawing,
  pickVanillaVersion,
  potionColor,
  resolveIcon,
  VANILLA_INDEX_FORMAT,
  VANILLA_MANIFEST_FORMAT,
  type VanillaAtlasIndex,
  type VanillaAtlasManifest,
  vanillaAtlas,
} from "@pl/ui/items";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

const INDEX: VanillaAtlasIndex = {
  format: VANILLA_INDEX_FORMAT,
  minecraft: "26.3",
  image: "atlas.png",
  size: [256, 128],
  glint: [200, 0, 32, 32],
  icons: {
    "minecraft:paper": { kind: "flat", layers: [{ rect: [0, 0, 16, 16] }] },
    "minecraft:stone": { kind: "block", layers: [{ rect: [16, 0, 32, 32] }] },
    "minecraft:obsidian": { kind: "block", layers: [{ rect: [16, 32, 32, 32] }], dark: true },
    "minecraft:enchanted_book": { kind: "flat", layers: [{ rect: [80, 0, 16, 16] }] },
    "minecraft:potion": {
      kind: "flat",
      layers: [
        { rect: [48, 0, 16, 16], tint: { source: "potion", default: "#385dc6" } },
        { rect: [64, 0, 16, 16] },
      ],
    },
    "minecraft:wolf_armor": {
      kind: "flat",
      layers: [{ rect: [96, 0, 16, 16] }, { rect: [112, 0, 16, 16], tint: { source: "dye" } }],
    },
  },
};

const DOC: ItemArtDoc = {
  schema: 1,
  hosts: ["projects.gg"],
  atlases: [
    { size: 16, columns: 8, file: { path: "itemart/atlas-16.aa.png", sha256: "aa", size: 10 } },
    { size: 32, columns: 8, file: { path: "itemart/atlas-32.bb.png", sha256: "bb", size: 20 } },
  ],
  items: [
    { item: "minecraft:paper", cmd: 100001, icon: 9, name: "Ruby" },
    { item: "minecraft:paper", model: "projects:coin", icon: 3, name: "Coin" },
  ],
};

function art(host?: string): ItemArt {
  return {
    vanilla: { index: INDEX, imageUrl: "atlas.png" },
    overlay: { doc: DOC, atlasUrl: (file) => `https://cdn.projects.gg/v1/${file.path}` },
    host,
  };
}

/** The index with a large atlas: stone drawn again at 48 px. */
const LARGE_INDEX: VanillaAtlasIndex = {
  ...INDEX,
  large: { image: "atlas-48.png", size: [128, 64], icon: 48 },
  icons: {
    ...INDEX.icons,
    "minecraft:stone": {
      kind: "block",
      layers: [{ rect: [16, 0, 32, 32] }],
      large: [49, 0, 48, 48],
    },
  },
};

/** The large atlas art; `largeReady` as a host that has its image already (a screenshot page). */
function largeArt(largeReady = true): ItemArt {
  return {
    vanilla: { index: LARGE_INDEX, imageUrl: "atlas.png", largeUrl: "atlas-48.png", largeReady },
  };
}

describe("item ids and hosts", () => {
  it("fills in the namespace and lower-cases", () => {
    expect(fullItemId("Diamond_Sword")).toBe("minecraft:diamond_sword");
    expect(fullItemId("projects:coin")).toBe("projects:coin");
  });

  it.each([
    ["projects.gg", true],
    ["play.projects.gg:25565", true],
    ["PLAY.Projects.GG.", true],
    ["PLAY.projects.gg.:25565", true],
    ["notprojects.gg", false],
    ["projects.gg.example.com", false],
    ["[::1]:25565", false],
    ["", false],
  ])("%s on projects.gg is %s", (address, expected) => {
    expect(hostMatches(address, ["projects.gg"])).toBe(expected);
  });

  it("uses the document's own hosts, as pl_content does", () => {
    expect(hostMatches("mc.example.net", ["example.net"])).toBe(true);
    expect(hostMatches("play.projects.gg", ["example.net"])).toBe(false);
    // Without hosts the art shows nowhere.
    expect(hostMatches("play.projects.gg", [])).toBe(false);
    expect(hostMatches("play.projects.gg", undefined)).toBe(false);
    // An IPv4 entry is only itself.
    expect(hostMatches("95.0.0.1:25565", ["95.0.0.1"])).toBe(true);
    expect(hostMatches("1.95.0.0.1", ["95.0.0.1"])).toBe(false);
    expect(hostMatches("play.projects.gg", ["PROJECTS.gg."])).toBe(true);
  });
});

describe("resolveIcon", () => {
  it("prefers the overlay's model, then item and cmd, then vanilla", () => {
    const onProjects = art("play.projects.gg");
    const byModel = resolveIcon({ id: "paper", cmd: 100001, model: "projects:coin" }, onProjects);
    expect(byModel.source === "overlay" && byModel.entry.name).toBe("Coin");
    const byCmd = resolveIcon({ id: "minecraft:paper", cmd: 100001 }, onProjects);
    expect(byCmd.source === "overlay" && byCmd.entry.name).toBe("Ruby");
    expect(resolveIcon({ id: "paper", cmd: 5 }, onProjects).source).toBe("vanilla");
  });

  it("places the overlay cell by its index and picks the atlas by size", () => {
    const big = resolveIcon({ id: "paper", cmd: 100001 }, art("projects.gg"), 48);
    expect(big.source === "overlay" && big.rect).toEqual([32, 32, 32, 32]);
    expect(big.source === "overlay" && big.url).toBe(
      "https://cdn.projects.gg/v1/itemart/atlas-32.bb.png",
    );
    const small = resolveIcon({ id: "paper", cmd: 100001 }, art("projects.gg"), 16);
    expect(small.source === "overlay" && small.rect).toEqual([16, 16, 16, 16]);
  });

  it("scales overlay art up, not down, and keeps to its cells", () => {
    const doc: ItemArtDoc = {
      hosts: ["projects.gg"],
      atlases: [
        { size: 32, columns: 4, cells: 6, file: { path: "a32.png", sha256: "c", size: 1 } },
        { size: 16, columns: 4, cells: 6, file: { path: "a16.png", sha256: "d", size: 1 } },
        { size: 12, columns: 4, cells: 6, file: { path: "a12.png", sha256: "e", size: 1 } },
      ],
      items: [
        { model: "projects:gem", icon: 5 },
        { item: "paper", cmd: 7, icon: 6 },
        { item: "paper", cmd: 8, icon: 1 },
        { item: "minecraft:paper", cmd: 8, icon: 2 },
      ],
    };
    const own: ItemArt = { overlay: { doc, atlasUrl: (f) => f.path }, host: "projects.gg" };
    const at = (size: number) => {
      const r = resolveIcon({ id: "stick", model: "projects:gem" }, own, size);
      return r.source === "overlay" ? r.url : r.source;
    };
    expect([at(16), at(24), at(32), at(48), at(64)]).toEqual([
      "a16.png",
      "a16.png",
      "a32.png",
      "a32.png",
      "a32.png",
    ]);
    // An entry with only `model` matches by model; a cell past the atlas is no art.
    expect(resolveIcon({ id: "paper", cmd: 7 }, own).source).toBe("fallback");
    // The first entry claiming a key wins.
    const first = resolveIcon({ id: "paper", cmd: 8 }, own, 16);
    expect(first.source === "overlay" && first.rect).toEqual([16, 0, 16, 16]);
  });

  it("shows Projects' art only on its servers", () => {
    expect(resolveIcon({ id: "paper", cmd: 100001 }, art("mc.example.net")).source).toBe("vanilla");
    expect(resolveIcon({ id: "paper", cmd: 100001 }, art(undefined)).source).toBe("vanilla");
  });

  it("falls back to a category drawing", () => {
    const missing = resolveIcon({ id: "minecraft:decorated_pot" }, art("projects.gg"));
    expect(missing).toEqual({ source: "fallback", category: "other" });
    expect(resolveIcon({ id: "diamond_sword" }, undefined)).toEqual({
      source: "fallback",
      category: "weapon",
    });
    expect(itemCategory("oak_planks")).toBe("block");
    expect(itemCategory("splash_potion")).toBe("potion");
    expect(itemCategory("netherite_chestplate")).toBe("armor");
  });
});

describe("tints and glint", () => {
  it("colours potions by id, custom colours first", () => {
    expect(potionColor("minecraft:strong_healing")).toBe(POTION_COLORS.healing);
    expect(potionColor("water")).toBeUndefined();
    expect(layerColor({ id: "potion", potion: "long_swiftness" }, "potion", "#385dc6")).toBe(
      "#33ebff",
    );
    expect(layerColor({ id: "potion", color: 0x00ff00 }, "potion", "#385dc6")).toBe("#00ff00");
    expect(layerColor({ id: "potion" }, "potion", "#385dc6")).toBe("#385dc6");
    expect(layerColor({ id: "leather_helmet", color: 0x123 }, "dye", "#a06540")).toBe("#000123");
  });

  it("leaves a dye layer out until it has a colour", () => {
    expect(layerColor({ id: "wolf_armor" }, "dye", undefined)).toBeUndefined();
    expect(layerColor({ id: "wolf_armor", color: 0x169c9c }, "dye", undefined)).toBe("#169c9c");
  });

  it("glints as the server says, else as the game draws the item", () => {
    expect(hasGlint({ id: "diamond_sword" })).toBe(false);
    expect(hasGlint({ id: "diamond_sword", enchanted: true })).toBe(true);
    expect(hasGlint({ id: "Enchanted_Golden_Apple" })).toBe(true);
    expect(hasGlint({ id: "minecraft:nether_star", enchanted: false })).toBe(false);
    expect(ALWAYS_GLINT.has("minecraft:enchanted_book")).toBe(true);
  });
});

describe("vanillaAtlas", () => {
  it("reads an index of this format, from JSON text too", () => {
    expect(vanillaAtlas(INDEX, "a.png")?.index.minecraft).toBe("26.3");
    expect(vanillaAtlas(JSON.stringify(INDEX), "a.png")?.imageUrl).toBe("a.png");
  });

  it("refuses a newer format or something else", () => {
    expect(vanillaAtlas({ ...INDEX, format: VANILLA_INDEX_FORMAT + 1 }, "a.png")).toBeUndefined();
    expect(vanillaAtlas({ hello: 1 }, "a.png")).toBeUndefined();
    expect(vanillaAtlas(null, "a.png")).toBeUndefined();
    expect(vanillaAtlas("{not json", "a.png")).toBeUndefined();
  });

  it("keeps the large atlas's image only when the index has one", () => {
    expect(vanillaAtlas(LARGE_INDEX, "a.png", "a-48.png")?.largeUrl).toBe("a-48.png");
    expect(vanillaAtlas(INDEX, "a.png", "a-48.png")?.largeUrl).toBeUndefined();
  });
});

describe("pickDrawing", () => {
  it.each([
    [16, "base"],
    [32, "base"],
    [48, "large"],
    [64, "base"],
    // 2.5 x 32 steps by 2 and 3 pixels, 1.67 x 48 by 1 and 2: the 32 px drawing is the evener.
    [80, "base"],
    // Both whole: the 32 px drawing, and the large image is not loaded.
    [96, "base"],
    [128, "base"],
    [144, "large"],
    [24, "base"],
    [40, "base"],
    // The inventory at GUI scale 3 on a 125 % screen: 38.4 CSS px.
    [38.4 * 1.25, "large"],
  ])("a 3D icon %s device px wide: its %s drawing", (device, expected) => {
    expect(pickDrawing(device, 48)).toBe(expected);
  });

  it("keeps to the drawings there are", () => {
    expect(pickDrawing(48)).toBe("base");
  });
});

describe("the site's atlases on the CDN", () => {
  const MANIFEST: VanillaAtlasManifest = {
    format: VANILLA_MANIFEST_FORMAT,
    default: "26.1.2",
    versions: {
      "26.1.2": "26.1.2/index.aaa.json",
      "26.2": "26.2/index.bbb.json",
      "26.3": "26.3/index.ccc.json",
    },
  };

  it.each([
    [undefined, "26.1.2"],
    ["26.2", "26.2"],
    ["26.3", "26.3"],
    ["26.2.1", "26.2"],
    ["26.4", "26.3"],
    ["1.20.4", "26.1.2"],
  ])("draws %s with %s", (minecraft, expected) => {
    expect(pickVanillaVersion(MANIFEST, minecraft)).toBe(expected);
  });

  it("finds the index and the atlas beside it", async () => {
    const asked: string[] = [];
    const files: Record<string, unknown> = {
      "https://cdn.example/itemart/vanilla/manifest.json": MANIFEST,
      "https://cdn.example/itemart/vanilla/26.3/index.ccc.json": {
        ...INDEX,
        image: "atlas.ddd.png",
      },
    };
    const get = async (url: string) => {
      asked.push(url);
      return files[url];
    };
    const atlas = await loadVanillaAtlas("26.3", "https://cdn.example/itemart/vanilla/", get);
    expect(atlas?.imageUrl).toBe("https://cdn.example/itemart/vanilla/26.3/atlas.ddd.png");
    expect(atlas?.index.icons["minecraft:paper"]).toBeDefined();
    expect(atlas?.largeUrl).toBeUndefined();
    // Asked once per version.
    await loadVanillaAtlas("26.3", "https://cdn.example/itemart/vanilla/", get);
    expect(asked).toHaveLength(2);
  });

  it("finds the large atlas beside the index too", async () => {
    const files: Record<string, unknown> = {
      "https://cdn.large/manifest.json": MANIFEST,
      "https://cdn.large/26.2/index.bbb.json": {
        ...LARGE_INDEX,
        image: "atlas.eee.png",
        large: { image: "atlas-48.fff.png", size: [128, 64], icon: 48 },
      },
    };
    const atlas = await loadVanillaAtlas("26.2", "https://cdn.large/", async (url) => files[url]);
    expect(atlas?.imageUrl).toBe("https://cdn.large/26.2/atlas.eee.png");
    expect(atlas?.largeUrl).toBe("https://cdn.large/26.2/atlas-48.fff.png");
  });

  it("gives up quietly on a newer manifest or an unreachable CDN", async () => {
    const newer = async () => ({ ...MANIFEST, format: VANILLA_MANIFEST_FORMAT + 1 });
    expect(await loadVanillaAtlas(undefined, "https://cdn.newer/", newer)).toBeUndefined();
    const down = async () => {
      throw new Error("offline");
    };
    expect(await loadVanillaAtlas(undefined, "https://cdn.down/", down)).toBeUndefined();
  });
});

describe("ItemSprite", () => {
  const render = (props: Parameters<typeof ItemSprite>[0]) =>
    renderToStaticMarkup(createElement(ItemSprite, props));

  it("scales a flat icon's sprite to whole pixels", () => {
    const html = render({ id: "paper", size: 48, art: art() });
    expect(html).toContain('data-source="vanilla"');
    // The 256 x 128 atlas three times over, the paper's cell at the box's corner.
    expect(html).toMatch(/left:0;top:0;width:768px;height:384px/);
    expect(html).toContain("image-rendering:pixelated");
    expect(html).toContain('aria-hidden="true"');
  });

  it("draws a 3D icon at 48 px from its 48 px drawing, and only then", () => {
    const html = render({ id: "stone", size: 48, enchanted: true, art: largeArt() });
    expect(html).toContain('src="atlas-48.png"');
    // The 128 x 64 large atlas at its own scale, the stone's cell at the box's corner.
    expect(html).toMatch(/left:-49px;top:0;width:128px;height:64px/);
    expect(html).toMatch(/mask-position:-49px 0px/);
    expect(render({ id: "stone", size: 32, art: largeArt() })).not.toContain("atlas-48.png");
    expect(render({ id: "stone", size: 64, art: largeArt() })).not.toContain("atlas-48.png");
    // Flat icons are whole at 48 already; without the large image's address, the 32 px drawing.
    expect(render({ id: "paper", size: 48, art: largeArt() })).not.toContain("atlas-48.png");
    const without: ItemArt = { vanilla: { index: LARGE_INDEX, imageUrl: "atlas.png" } };
    expect(render({ id: "stone", size: 48, art: without })).not.toContain("atlas-48.png");
  });

  it("shows the 32 px drawing at 48 px until the large image has loaded, never an empty slot", () => {
    // On the server and while the large image loads (or after it failed): the stone's 32 px cell,
    // scaled to the box.
    const html = render({ id: "stone", size: 48, art: largeArt(false) });
    expect(html).not.toContain("atlas-48.png");
    expect(html).toContain('src="atlas.png"');
    expect(html).toMatch(/left:-24px;top:0;width:384px;height:192px/);
  });

  it("halves the 32 px drawing of a 3D icon at 16 px", () => {
    expect(render({ id: "stone", size: 16, art: largeArt() })).toMatch(
      /left:-8px;top:0;width:128px;height:64px/,
    );
  });

  it("asks for every atlas picture with CORS, as the masks are", () => {
    const html = render({ id: "potion", potion: "healing", enchanted: true, art: art() });
    const images = html.match(/<img [^>]*>/g) ?? [];
    expect(images.length).toBe(3);
    for (const image of images) {
      expect(image).toContain('crossorigin="anonymous"');
      expect(image).toContain('draggable="false"');
      expect(image).toContain('alt=""');
    }
    expect(html).not.toContain("background-image");
  });

  it("multiplies a tinted layer by the item's colour, through a mask as crisp as the picture", () => {
    const html = render({ id: "potion", potion: "healing", art: art() });
    expect(html).toContain(
      "mask-repeat:no-repeat;image-rendering:pixelated;background-color:#f82423;mix-blend-mode:multiply",
    );
  });

  it("draws a dye layer only when dyed", () => {
    expect(render({ id: "wolf_armor", art: art() })).not.toContain("mix-blend-mode:multiply");
    const dyed = render({ id: "wolf_armor", color: 0x169c9c, art: art() });
    expect(dyed).toContain("background-color:#169c9c");
  });

  it("adds the still glint frame only where the item glints", () => {
    expect(render({ id: "stone", art: art() })).not.toContain("mix-blend-mode:screen");
    const html = render({ id: "stone", enchanted: true, size: 32, art: art() });
    expect(html).toContain("mix-blend-mode:screen");
    expect(html).toMatch(/left:-200px;top:0;width:256px;height:128px;[^"]*image-rendering:auto/);
    expect(render({ id: "enchanted_book", art: art() })).toContain("mix-blend-mode:screen");
  });

  it("masks the glint by every drawn layer", () => {
    const html = render({ id: "potion", enchanted: true, size: 16, art: art() });
    expect(html).toMatch(/mask-position:-48px 0px, -64px 0px/);
  });

  it("edges a dark icon by one hard texel, which a light surface can turn off", () => {
    const edge = (t: number) =>
      [`${t}px 0`, `-${t}px 0`, `0 ${t}px`, `0 -${t}px`]
        .map((offset) => `drop-shadow(${offset} 0 var(--pl-item-rim, #2c2733))`)
        .join(" ");
    expect(render({ id: "obsidian", size: 32, art: art() })).toContain(`filter:${edge(2)}`);
    expect(render({ id: "obsidian", size: 16, art: art() })).toContain(`filter:${edge(1)}`);
    expect(render({ id: "obsidian", size: 48, art: art() })).toContain(`filter:${edge(3)}`);
    expect(render({ id: "stone", art: art() })).not.toContain("drop-shadow");
  });

  it("draws the stand-ins as one flat silhouette, the same on every render", () => {
    const html = render({ id: "minecraft:unknown_block" });
    expect(render({ id: "minecraft:unknown_block" })).toBe(html);
    // One path in one tone (no light, no outline) that a light surface can set.
    expect(html.match(/<path/g)).toHaveLength(1);
    expect(html).toContain("fill:var(--pl-item-standin, #5a5462)");
    expect(html).not.toMatch(/fill="#/);
    // The block's three faces: its sides parted by the near edge.
    expect(html).toContain("M0 7h7v1h-7zM8 7h8v1h-8z");
  });

  it("draws Projects' art from the overlay atlas on its servers", () => {
    const html = render({ id: "paper", cmd: 100001, size: 32, art: art("projects.gg") });
    expect(html).toContain('data-source="overlay"');
    expect(html).toContain("atlas-32.bb.png");
  });

  it("names the item when asked and stands in for unknown ones", () => {
    const html = render({ id: "minecraft:unknown_thing", label: "Gizemli eşya" });
    expect(html).toContain('role="img"');
    expect(html).toContain('aria-label="Gizemli eşya"');
    expect(html).toContain('data-source="fallback"');
    expect(html).toContain("<path");
  });
});
