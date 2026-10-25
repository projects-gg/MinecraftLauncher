// An item's picture (ITEMART): Projects' own art on its servers, else the game's texture from the
// vanilla atlas, else a quiet stand-in. Each drawn layer is the atlas itself, scaled
// nearest-neighbour and clipped to the box; tinted layers are multiplied in place through a mask
// of their own pixels, and the glint is one still frame of the game's own glint added over the
// item (never animated). Every atlas request is a CORS request (images with `crossorigin` and CSS
// masks alike): a CDN answers a request without an Origin without CORS headers, and a browser that
// cached that answer would refuse it to the masks. 3D icons drawn 48 device pixels wide (the
// game's GUI scale 3) come from the atlas's second, 48 px image, loaded only then. Icons the atlas
// marks dark get a quiet one-texel edge so they stay visible on our dark slots; a light surface (the
// game-grey inventory) turns it off with `--pl-item-rim: transparent` and sets the stand-ins' tone
// with `--pl-item-standin` (the game's own `#555555` there). Renders on the server too.
import {
  type CSSProperties,
  createContext,
  type ReactNode,
  useContext,
  useSyncExternalStore,
} from "react";
import { cn } from "../lib/cn";
import { FALLBACK_GLYPHS } from "./fallback-glyphs";
import { hasGlint, layerColor, type ResolvedIcon, resolveIcon } from "./resolve";
import type { ItemArt, ItemRef } from "./types";

const ItemArtContext = createContext<ItemArt | undefined>(undefined);

/** Gives every ItemSprite below the atlases (and the server host the overlay checks). */
export function ItemArtProvider({
  value,
  children,
}: {
  value: ItemArt | undefined;
  children: ReactNode;
}) {
  return <ItemArtContext.Provider value={value}>{children}</ItemArtContext.Provider>;
}

export function useItemArt(): ItemArt | undefined {
  return useContext(ItemArtContext);
}

/** Whole multiples of 16 keep the game's pixels square. */
export type ItemSpriteSize = 16 | 32 | 48 | 64;

export interface ItemSpriteProps extends ItemRef {
  size?: ItemSpriteSize;
  /** The item's name when nothing beside the sprite says it; otherwise it is decorative. */
  label?: string;
  className?: string;
  /** Art for this sprite alone (otherwise the provider's). */
  art?: ItemArt;
}

type Rect = readonly [number, number, number, number];

/** An atlas: where it is and its size in pixels (the height when known). */
interface Sheet {
  url: string;
  width: number;
  height?: number;
}

const FILL: CSSProperties = { position: "absolute", inset: 0 };

/**
 * The edge of icons whose outline is near-black on our dark slots: one texel wide, hard and on the
 * game's pixel grid (four unblurred shadows a texel out), in a tone only a few steps above the
 * darkest card (L* 16 on 9), so it separates without reading as a frame or a selection. Opaque, so
 * the corners match the sides; a light surface turns it off with `transparent`.
 */
function rim(size: number): string {
  const t = size / 16;
  const color = "var(--pl-item-rim, #2c2733)";
  return [`${t}px 0`, `${-t}px 0`, `0 ${t}px`, `0 ${-t}px`]
    .map((offset) => `drop-shadow(${offset} 0 ${color})`)
    .join(" ");
}

/** One rectangle of an atlas drawn over the whole box: the atlas image, moved and scaled. */
function Cut({
  sheet,
  rect,
  size,
  smooth = false,
}: {
  sheet: Sheet;
  rect: Rect;
  size: number;
  smooth?: boolean;
}) {
  const k = size / rect[2];
  return (
    <img
      alt=""
      src={sheet.url}
      crossOrigin="anonymous"
      draggable={false}
      decoding="async"
      style={{
        position: "absolute",
        left: -rect[0] * k,
        top: -rect[1] * k,
        width: sheet.width * k,
        height: sheet.height === undefined ? "auto" : sheet.height * k,
        maxWidth: "none",
        maxHeight: "none",
        imageRendering: smooth ? "auto" : "pixelated",
        pointerEvents: "none",
        userSelect: "none",
      }}
    />
  );
}

/**
 * A mask of the given atlas rectangles together (an icon's drawn layers), scaled
 * nearest-neighbour like the pictures: a smoothed mask blurs a tinted layer's texels into their
 * neighbours and lets the glint spill past the outline.
 */
function mask(sheet: Sheet, rects: readonly Rect[], size: number): CSSProperties {
  const image = rects.map(() => `url("${sheet.url}")`).join(", ");
  const scale = rects.map((rect) => `${(sheet.width * size) / rect[2]}px auto`).join(", ");
  const position = rects
    .map((rect) => `${(-rect[0] * size) / rect[2]}px ${(-rect[1] * size) / rect[2]}px`)
    .join(", ");
  return {
    WebkitMaskImage: image,
    maskImage: image,
    WebkitMaskSize: scale,
    maskSize: scale,
    WebkitMaskPosition: position,
    maskPosition: position,
    WebkitMaskRepeat: "no-repeat",
    maskRepeat: "no-repeat",
    imageRendering: "pixelated",
  };
}

/**
 * The stand-ins' one flat tone, like the game's own empty-slot hints (one grey, no light, no
 * outline), so a stand-in never passes for a real item: our placeholder grey (`ash-500`, 4:1 on
 * the darkest card) by default; the game-grey inventory sets the game's own `#555555`.
 */
const STANDIN_FILL = "var(--pl-item-standin, #7d7586)";

const glyphPaths = new Map<string, string>();

/** A stand-in's silhouette as one SVG path, a rectangle per run of filled cells. */
function glyph(category: keyof typeof FALLBACK_GLYPHS): string {
  const hit = glyphPaths.get(category);
  if (hit) return hit;
  let d = "";
  FALLBACK_GLYPHS[category].forEach((row, y) => {
    for (const run of row.matchAll(/#+/g)) {
      d += `M${run.index} ${y}h${run[0].length}v1h-${run[0].length}z`;
    }
  });
  glyphPaths.set(category, d);
  return d;
}

interface Drawn {
  layers: ReactNode[];
  /** The mask the glint is drawn through: everything drawn. */
  glintMask?: CSSProperties;
  dark: boolean;
}

// Device pixels per CSS pixel, shared by every sprite: 1 on the server and while hydrating, then
// the screen's, followed when the window moves to another screen or the page is zoomed.
const ratioListeners = new Set<() => void>();
let ratioQuery: MediaQueryList | undefined;

function watchRatio(): void {
  ratioQuery?.removeEventListener("change", onRatioChange);
  ratioQuery = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
  ratioQuery.addEventListener("change", onRatioChange);
}

function onRatioChange(): void {
  watchRatio();
  for (const listener of ratioListeners) listener();
}

function subscribeRatio(listener: () => void): () => void {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return () => {};
  ratioListeners.add(listener);
  if (ratioListeners.size === 1) watchRatio();
  return () => {
    ratioListeners.delete(listener);
    if (ratioListeners.size === 0) {
      ratioQuery?.removeEventListener("change", onRatioChange);
      ratioQuery = undefined;
    }
  };
}

const screenRatio = () => (typeof window === "undefined" ? 1 : window.devicePixelRatio || 1);
const serverRatio = () => 1;

/**
 * Which drawing of a 3D icon to show `device` pixels wide: its large one (48) where that fits a
 * whole number of times and 32 does not (48, the game's GUI scale 3, where 32 scaled by 1.5 shows
 * uneven pixels; 144), else the 32 px one, scaled by a whole number or halved (16; the atlas keeps
 * its cells on even pixels, so a halved icon looks the same in every atlas). The large image is
 * loaded only when it is drawn.
 */
export function pickDrawing(device: number, large?: number, base = 32): "base" | "large" {
  const whole = (side: number) =>
    device >= side - 0.01 && Math.abs(device / side - Math.round(device / side)) < 0.01;
  return large !== undefined && whole(large) && !whole(base) ? "large" : "base";
}

function drawLayers(
  resolved: ResolvedIcon,
  item: ItemRef,
  art: ItemArt | undefined,
  size: number,
  ratio: number,
): Drawn | undefined {
  if (resolved.source === "overlay") {
    const { atlas, url, rect } = resolved;
    const rows =
      atlas.cells === undefined ? undefined : Math.ceil(atlas.cells / Math.max(1, atlas.columns));
    const sheet: Sheet = {
      url,
      width: atlas.columns * atlas.size,
      height: rows === undefined ? undefined : rows * atlas.size,
    };
    return {
      layers: [<Cut key="o" sheet={sheet} rect={rect} size={size} />],
      glintMask: mask(sheet, [rect], size),
      dark: false,
    };
  }
  if (resolved.source === "vanilla" && art?.vanilla) {
    const { imageUrl, largeUrl, index } = art.vanilla;
    const sheet: Sheet = { url: imageUrl, width: index.size[0], height: index.size[1] };
    // A 3D icon (one untinted layer) 48 device px wide: the drawing made for that size.
    const { large } = resolved.icon;
    if (
      large &&
      largeUrl &&
      index.large &&
      resolved.icon.layers.every((layer) => !layer.tint) &&
      pickDrawing(size * ratio, index.large.icon) === "large"
    ) {
      const largeSheet: Sheet = {
        url: largeUrl,
        width: index.large.size[0],
        height: index.large.size[1],
      };
      return {
        layers: [<Cut key="l" sheet={largeSheet} rect={large} size={size} />],
        glintMask: mask(largeSheet, [large], size),
        dark: resolved.icon.dark === true,
      };
    }
    const drawn: Rect[] = [];
    const layers = resolved.icon.layers.flatMap((layer) => {
      // Each layer has its own place in the atlas, so the place names it.
      const key = layer.rect.join(",");
      const own = <Cut key={key} sheet={sheet} rect={layer.rect} size={size} />;
      if (!layer.tint) {
        drawn.push(layer.rect);
        return [own];
      }
      const color = layerColor(item, layer.tint.source, layer.tint.default);
      // A dye layer without a colour is not there (undyed wolf armour).
      if (!color) return [];
      drawn.push(layer.rect);
      return [
        own,
        <span
          key={`${key}t`}
          style={{
            ...FILL,
            ...mask(sheet, [layer.rect], size),
            backgroundColor: color,
            mixBlendMode: "multiply",
          }}
        />,
      ];
    });
    return {
      layers,
      glintMask: drawn.length ? mask(sheet, drawn, size) : undefined,
      dark: resolved.icon.dark === true,
    };
  }
  return undefined;
}

export function ItemSprite({ size = 32, label, className, art: own, ...item }: ItemSpriteProps) {
  const context = useItemArt();
  const art = own ?? context;
  const ratio = useSyncExternalStore(subscribeRatio, screenRatio, serverRatio);
  const resolved = resolveIcon(item, art, size);
  const drawn = drawLayers(resolved, item, art, size, ratio);
  const a11y = label
    ? ({ role: "img", "aria-label": label } as const)
    : ({ "aria-hidden": true } as const);
  const box: CSSProperties = {
    position: "relative",
    display: "inline-block",
    width: size,
    height: size,
    isolation: "isolate",
    flexShrink: 0,
  };
  if (!drawn) {
    const category = resolved.source === "fallback" ? resolved.category : "other";
    return (
      <span
        {...a11y}
        data-item={item.id}
        data-source="fallback"
        className={cn("shrink-0", className)}
        style={box}
      >
        <svg
          aria-hidden
          focusable="false"
          width={size}
          height={size}
          viewBox="0 0 16 16"
          shapeRendering="crispEdges"
          style={{ display: "block" }}
        >
          <path d={glyph(category)} style={{ fill: STANDIN_FILL }} />
        </svg>
      </span>
    );
  }
  // The still glint frame from the vanilla atlas, scaled smoothly like the game's blurred glint.
  const vanilla = art?.vanilla;
  const frame = vanilla?.index.glint;
  const glint =
    hasGlint(item) && drawn.glintMask && frame && vanilla ? (
      <span style={{ ...FILL, ...drawn.glintMask, mixBlendMode: "screen" }}>
        <Cut
          sheet={{
            url: vanilla.imageUrl,
            width: vanilla.index.size[0],
            height: vanilla.index.size[1],
          }}
          rect={frame}
          size={size}
          smooth
        />
      </span>
    ) : null;
  return (
    <span
      {...a11y}
      data-item={item.id}
      data-source={resolved.source}
      className={cn("shrink-0", className)}
      style={drawn.dark ? { ...box, filter: rim(size) } : box}
    >
      <span style={{ ...FILL, overflow: "hidden" }}>
        {drawn.layers}
        {glint}
      </span>
    </span>
  );
}
