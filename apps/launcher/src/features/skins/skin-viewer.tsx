// WebGL skin preview (skinview3d). Loaded lazily so three.js stays out of the main bundle.
import { useAmbientMotion, useInteractionMotion } from "@pl/ui";
import { useEffect, useRef, useState } from "react";
import { IdleAnimation, SkinViewer } from "skinview3d";
import type { SkinVariant } from "@/lib/api";
import { loadImage } from "./skin-art";

/** Turn the model toward the camera ("front") or away from it ("back"); `key` re-triggers. */
export interface SkinTurn {
  side: "front" | "back";
  key: number;
}

export interface SkinViewer3DProps {
  /** Skin texture as a data URL; `null` hides the player. */
  skin: string | null;
  variant: SkinVariant;
  /** Cape texture as a data URL; `null` for no cape. */
  cape: string | null;
  turn: SkinTurn | null;
  /** WebGL is not available: the caller shows a flat preview instead. */
  onUnsupported: () => void;
  /** The current `skin` has been drawn (lets the caller fade the new skin in after it is there). */
  onSkinShown?: () => void;
  className?: string;
}

const modelOf = (variant: SkinVariant) => (variant === "slim" ? "slim" : "default");
const easeOutExpo = (t: number) => (t >= 1 ? 1 : 1 - 2 ** (-10 * t));

export default function SkinViewer3D({
  skin,
  variant,
  cape,
  turn,
  onUnsupported,
  onSkinShown,
  className,
}: SkinViewer3DProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const [viewer, setViewer] = useState<SkinViewer | null>(null);
  // Auto-rotation and the idle sway are ambient motion: off under reduced motion, in
  // Performance mode, while the window is hidden and while the game runs.
  const ambient = useAmbientMotion();
  const animateTurns = useInteractionMotion();
  /** The player dragged or turned the model: auto-rotation stays off from then on. */
  const takenOver = useRef(false);
  const variantRef = useRef(variant);
  variantRef.current = variant;
  const unsupported = useRef(onUnsupported);
  unsupported.current = onUnsupported;
  const shown = useRef(onSkinShown);
  shown.current = onSkinShown;

  // One viewer per mount, on its own canvas; the WebGL context is released on unmount.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let created: SkinViewer;
    try {
      created = new SkinViewer({
        width: Math.max(1, host.clientWidth),
        height: Math.max(1, host.clientHeight),
        fov: 38,
        zoom: 0.8,
        renderPaused: true,
      });
    } catch {
      unsupported.current();
      return;
    }
    const canvas = created.canvas;
    canvas.className = "block cursor-grab outline-none active:cursor-grabbing";
    host.appendChild(canvas);
    created.controls.enableZoom = false;
    const takeOver = () => {
      takenOver.current = true;
      created.autoRotate = false;
    };
    // Still frames (no ambient motion): draw only when something changes.
    const redraw = () => {
      if (created.renderPaused) created.render();
    };
    created.controls.addEventListener("start", takeOver);
    created.controls.addEventListener("change", redraw);
    const observer = new ResizeObserver(() => {
      created.setSize(Math.max(1, host.clientWidth), Math.max(1, host.clientHeight));
      redraw();
    });
    observer.observe(host);
    setViewer(created);
    return () => {
      observer.disconnect();
      created.controls.removeEventListener("start", takeOver);
      created.controls.removeEventListener("change", redraw);
      created.dispose();
      created.renderer.forceContextLoss();
      canvas.remove();
      setViewer(null);
    };
  }, []);

  useEffect(() => {
    if (!viewer) return;
    viewer.animation = ambient ? new IdleAnimation() : null;
    viewer.autoRotate = ambient && !takenOver.current;
    viewer.autoRotateSpeed = 0.45;
    viewer.renderPaused = !ambient;
    if (!ambient) viewer.render();
  }, [viewer, ambient]);

  useEffect(() => {
    if (!viewer) return;
    if (!skin) {
      viewer.loadSkin(null);
      if (viewer.renderPaused) viewer.render();
      shown.current?.();
      return;
    }
    let live = true;
    loadImage(skin)
      .then((image) => {
        if (!live || viewer.disposed) return;
        viewer.loadSkin(image, { model: modelOf(variantRef.current) });
        if (viewer.renderPaused) viewer.render();
        shown.current?.();
      })
      .catch(() => shown.current?.());
    return () => {
      live = false;
    };
  }, [viewer, skin]);

  useEffect(() => {
    if (!viewer) return;
    viewer.playerObject.skin.modelType = modelOf(variant);
    if (viewer.renderPaused) viewer.render();
  }, [viewer, variant]);

  useEffect(() => {
    if (!viewer) return;
    if (!cape) {
      viewer.loadCape(null);
      if (viewer.renderPaused) viewer.render();
      return;
    }
    let live = true;
    loadImage(cape)
      .then((image) => {
        if (!live || viewer.disposed) return;
        viewer.loadCape(image);
        if (viewer.renderPaused) viewer.render();
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [viewer, cape]);

  // Turning answers a player action (a cape was picked, a file was chosen).
  useEffect(() => {
    if (!viewer || !turn) return;
    takenOver.current = true;
    viewer.autoRotate = false;
    const wrapper = viewer.playerWrapper;
    const target = viewer.controls.getAzimuthalAngle() + (turn.side === "back" ? Math.PI : 0);
    const from = wrapper.rotation.y;
    // Shortest way round.
    let delta = (target - from) % (2 * Math.PI);
    if (delta > Math.PI) delta -= 2 * Math.PI;
    if (delta < -Math.PI) delta += 2 * Math.PI;
    if (!animateTurns || Math.abs(delta) < 0.01) {
      wrapper.rotation.y = from + delta;
      if (viewer.renderPaused) viewer.render();
      return;
    }
    const start = performance.now();
    const duration = 650;
    let frame = 0;
    const step = (now: number) => {
      if (viewer.disposed) return;
      const progress = Math.min(1, (now - start) / duration);
      wrapper.rotation.y = from + delta * easeOutExpo(progress);
      if (viewer.renderPaused) viewer.render();
      if (progress < 1) frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [viewer, turn, animateTurns]);

  return <div ref={hostRef} className={className} />;
}
