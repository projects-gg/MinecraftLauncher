// Loading a skin head (INV-HEADS): one CORS image request per skin (Mojang's texture server
// answers with `Access-Control-Allow-Origin: *` and a week's caching), its pixels read once and
// drawn per icon size, all kept for the page's life in a bounded memory. Nothing shows while it
// loads or when it fails: the sprite draws the plain head then. Renders on the server too (no
// skin there).
import { type CSSProperties, useEffect, useRef, useSyncExternalStore } from "react";
import { type Pixels, processSkin, renderHead, skinTextureUrl } from "./skin";

type SkinState =
  | { status: "loading" }
  | { status: "ready"; skin: Pixels; drawings: Map<number, Pixels> }
  | { status: "failed" };

/** Skins kept: a menu of heads holds a few dozen. */
const MAX_SKINS = 96;

const states = new Map<string, SkinState>();
const listeners = new Set<() => void>();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function settle(url: string, state: SkinState): void {
  states.delete(url);
  states.set(url, state);
  while (states.size > MAX_SKINS) {
    const oldest = states.keys().next().value;
    if (oldest === undefined) break;
    states.delete(oldest);
  }
  for (const listener of listeners) listener();
}

function read(image: HTMLImageElement): Pixels | null {
  const canvas = document.createElement("canvas");
  canvas.width = image.naturalWidth;
  canvas.height = image.naturalHeight;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) return null;
  context.drawImage(image, 0, 0);
  const { width, height, data } = context.getImageData(0, 0, canvas.width, canvas.height);
  return processSkin({ width, height, data });
}

function load(url: string): void {
  if (states.has(url) || typeof Image !== "function" || typeof document === "undefined") return;
  states.set(url, { status: "loading" });
  const image = new Image();
  // A CORS request: the pixels are read (a tainted canvas would refuse them).
  image.crossOrigin = "anonymous";
  image.decoding = "async";
  image.src = url;
  image.decode().then(
    () => {
      let skin: Pixels | null = null;
      try {
        skin = read(image);
      } catch {
        skin = null;
      }
      settle(url, skin ? { status: "ready", skin, drawings: new Map() } : { status: "failed" });
    },
    () => settle(url, { status: "failed" }),
  );
}

/**
 * The head drawn with `skin` (a frame's texture hash) at `size` pixels (32 or 48, the atlas's 3D
 * drawings), once its skin has loaded; undefined until then, on the server, and for a skin that
 * did not load or that the game would not take.
 */
export function useSkinHead(skin: string | undefined, size: number): Pixels | undefined {
  const url = skinTextureUrl(skin);
  const state = useSyncExternalStore(
    subscribe,
    () => (url ? states.get(url) : undefined),
    () => undefined,
  );
  useEffect(() => {
    if (url) load(url);
  }, [url]);
  if (state?.status !== "ready") return undefined;
  let drawing = state.drawings.get(size);
  if (!drawing) {
    drawing = renderHead(state.skin, size);
    state.drawings.set(size, drawing);
  }
  return drawing;
}

/** A skin head's drawing over the sprite's box, scaled nearest-neighbour like the atlas's icons. */
export function SkinHeadCanvas({ drawing, size }: { drawing: Pixels; size: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const context = ref.current?.getContext("2d");
    if (!context) return;
    context.putImageData(
      new ImageData(new Uint8ClampedArray(drawing.data), drawing.width, drawing.height),
      0,
      0,
    );
  }, [drawing]);
  const style: CSSProperties = {
    position: "absolute",
    inset: 0,
    width: size,
    height: size,
    imageRendering: "pixelated",
    pointerEvents: "none",
  };
  return (
    <canvas ref={ref} width={drawing.width} height={drawing.height} aria-hidden style={style} />
  );
}
