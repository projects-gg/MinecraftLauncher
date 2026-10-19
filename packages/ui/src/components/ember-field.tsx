import { useReducedMotion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { useAmbientMotion, useResting } from "../perf";

interface Ember {
  x: number;
  y: number;
  vx: number;
  vy: number;
  size: number;
  life: number;
  maxLife: number;
  hue: number;
  phase: number;
}

export type ParticleKind = "embers" | "sparks" | "snow" | "none";

/** Hue range of the embers (`hue` below) and how finely their glow sprites are cut. */
const HUE_MIN = 28;
const HUE_SPAN = 18;
const HUE_STEPS = 8;
const SPRITE_PX = 64;
const sprites = new Map<number, HTMLCanvasElement>();

/**
 * The embers draw at most 60 times a second: on a 120/144 Hz display every frame would double
 * the canvas work and its compositor commits for motion nobody can tell apart (slow drift,
 * moved by elapsed time, so the speed stays the same). A frame up to 2 ms early still counts, so
 * a 60 Hz display never drops to 30.
 */
const FRAME_MS = 1000 / 60;
const FRAME_SLACK_MS = 2;

/**
 * The glow of one ember, drawn once per hue step at full strength. Each frame then only scales
 * and fades a copy (`drawImage` + `globalAlpha`), the same picture as a radial gradient built
 * per ember per frame, at a fraction of the main-thread time (the home page draws up to 110
 * embers up to 60 times a second).
 */
function emberSprite(hue: number): HTMLCanvasElement | null {
  const step = Math.min(
    HUE_STEPS - 1,
    Math.max(0, Math.floor(((hue - HUE_MIN) / HUE_SPAN) * HUE_STEPS)),
  );
  const cached = sprites.get(step);
  if (cached) return cached;
  const sprite = document.createElement("canvas");
  sprite.width = SPRITE_PX;
  sprite.height = SPRITE_PX;
  const ctx = sprite.getContext("2d");
  if (!ctx) return null;
  const h = HUE_MIN + ((step + 0.5) * HUE_SPAN) / HUE_STEPS;
  const c = SPRITE_PX / 2;
  const g = ctx.createRadialGradient(c, c, 0, c, c, c);
  g.addColorStop(0, `hsla(${h + 12}, 100%, 78%, 1)`);
  g.addColorStop(0.35, `hsla(${h}, 95%, 56%, 0.55)`);
  g.addColorStop(1, `hsla(${h - 10}, 90%, 40%, 0)`);
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, SPRITE_PX, SPRITE_PX);
  sprites.set(step, sprite);
  return sprite;
}

/**
 * When the embers start they fade in. When they pause (a game starts, Performance mode) they
 * keep drifting while they fade out: a scene that went still in one frame read as a glitch, most
 * of all right after OYNA. When the player rests (no input for a while, another window in front)
 * no new ember rises and the field fades slowly, a fire dying down; the next input brings it back
 * from wherever the fade got to, with the embers still in the air.
 */
export const EMBER_FADE_IN_MS = 900;
export const EMBER_FADE_OUT_MS = 600;
export const EMBER_REST_FADE_MS = 2400;

/**
 * How strong the field is `dtMs` after it was `strength` (0 to 1): rising towards 1 while it may
 * draw (`on`), else sinking towards 0, slowly while the player rests.
 */
export function emberStrength(strength: number, dtMs: number, on: boolean, resting: boolean) {
  if (on) return Math.min(1, strength + dtMs / EMBER_FADE_IN_MS);
  return Math.max(0, strength - dtMs / (resting ? EMBER_REST_FADE_MS : EMBER_FADE_OUT_MS));
}

/**
 * Rising ember particles on a single canvas. Runs outside React state; draws only while ambient
 * motion may run (not under reduced motion, in Performance mode, with the window hidden, while
 * the game runs or while the player rests). Between those times nothing runs at all: no frame is
 * requested and the canvas is hidden.
 */
export function EmberField({
  kind = "embers",
  density = 1,
  className,
}: {
  kind?: ParticleKind;
  density?: number;
  className?: string;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const on = useAmbientMotion() && kind !== "none";
  const resting = useResting();
  const reduce = useReducedMotion() ?? false;
  // What the drawing loop heads for, read on every frame.
  const wish = useRef({ on, resting, reduce });
  // Starts the drawing loop unless it runs (a loop still fading out turns around instead).
  const wake = useRef<(() => void) | null>(null);
  // Drawing or fading out: the canvas stays shown until the fade ended.
  const [live, setLive] = useState(false);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || kind === "none") return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let running = false;
    let frame = 0;
    let width = 0;
    let height = 0;
    let dpr = 1;
    let last = 0;
    // When the next frame may draw (see FRAME_MS).
    let next = 0;
    let strength = 0;
    // The first top-up of a new loop spreads embers over the scene; later ones rise from below.
    let seeded = false;
    const particles: Ember[] = [];
    const snow = kind === "snow";

    const resize = () => {
      dpr = Math.min(window.devicePixelRatio || 1, 2);
      width = canvas.clientWidth;
      height = canvas.clientHeight;
      canvas.width = Math.max(1, Math.floor(width * dpr));
      canvas.height = Math.max(1, Math.floor(height * dpr));
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };

    const target = () => Math.min(110, Math.round(((width * height) / 14000) * density));

    const spawn = (initial: boolean): Ember => {
      const maxLife = snow ? 9 + Math.random() * 8 : 3.5 + Math.random() * 4.5;
      return {
        x: Math.random() * width,
        y: snow
          ? initial
            ? Math.random() * height
            : -10
          : initial
            ? height * (0.35 + Math.random() * 0.65)
            : height + 10,
        vx: (Math.random() - 0.5) * (snow ? 14 : 10),
        vy: snow ? 18 + Math.random() * 26 : -(18 + Math.random() * 46),
        size: snow
          ? 1 + Math.random() * 2.2
          : kind === "sparks"
            ? 0.6 + Math.random() * 1.2
            : 0.8 + Math.random() * 1.9,
        life: initial ? Math.random() * maxLife : 0,
        maxLife,
        hue: HUE_MIN + Math.random() * HUE_SPAN,
        phase: Math.random() * Math.PI * 2,
      };
    };

    const stop = () => {
      running = false;
      cancelAnimationFrame(frame);
      ctx.clearRect(0, 0, width, height);
      particles.length = 0;
      setLive(false);
    };

    const tick = (now: number) => {
      if (!canvas.isConnected) {
        stop();
        return;
      }
      frame = requestAnimationFrame(tick);
      if (now < next - FRAME_SLACK_MS) return;
      // Keep the 60 Hz rhythm (a 144 Hz display draws every 2nd or 3rd frame); after a stall
      // start over instead of drawing several frames back to back.
      next = now - next > FRAME_MS ? now + FRAME_MS : next + FRAME_MS;
      const dtMs = Math.min(50, now - last);
      last = now;
      const want = wish.current;
      // Reduced motion came on: nothing drifts out, the embers go at once.
      if (!want.on && want.reduce) {
        stop();
        return;
      }
      strength = emberStrength(strength, dtMs, want.on, want.resting);
      if (!want.on && strength <= 0) {
        stop();
        return;
      }
      const dt = dtMs / 1000;
      ctx.clearRect(0, 0, width, height);
      ctx.globalCompositeOperation = snow ? "source-over" : "lighter";

      // Winding down, nothing new rises.
      if (want.on) {
        const count = target();
        while (particles.length < count) {
          particles.push(spawn(!seeded && particles.length < count / 2));
        }
        if (count > 0) seeded = true;
      }

      for (let i = particles.length - 1; i >= 0; i--) {
        const p = particles[i];
        if (!p) continue;
        p.life += dt;
        p.phase += dt * 2.2;
        p.x += (p.vx + Math.sin(p.phase) * (snow ? 10 : 8)) * dt;
        p.y += p.vy * dt;
        const t = p.life / p.maxLife;
        if (t >= 1 || p.y < -20 || p.y > height + 20) {
          // A spent ember is replaced while the field is on; winding down, it is simply gone.
          if (want.on) particles[i] = spawn(false);
          else particles.splice(i, 1);
          continue;
        }
        // Fade in fast, out slow; embers flicker.
        const fade = Math.min(1, t * 6) * (1 - t) ** 1.4;
        const flicker = snow ? 1 : 0.75 + Math.sin(p.phase * 3.1) * 0.25;
        const alpha = fade * flicker * strength;
        if (snow) {
          ctx.fillStyle = `rgba(243, 235, 221, ${alpha * 0.75})`;
          ctx.beginPath();
          ctx.arc(p.x, p.y, p.size, 0, Math.PI * 2);
          ctx.fill();
        } else {
          const r = p.size * 3.2;
          const sprite = emberSprite(p.hue);
          if (!sprite) continue;
          ctx.globalAlpha = alpha;
          ctx.drawImage(sprite, p.x - r, p.y - r, r * 2, r * 2);
        }
      }
      ctx.globalAlpha = 1;
    };

    const start = () => {
      if (running) return;
      running = true;
      strength = 0;
      seeded = false;
      last = performance.now();
      next = 0;
      setLive(true);
      frame = requestAnimationFrame(tick);
    };

    resize();
    // Also sizes the canvas when it shows (it is hidden while nothing draws).
    const observer = new ResizeObserver(resize);
    observer.observe(canvas);
    wake.current = start;
    if (wish.current.on) start();

    return () => {
      wake.current = null;
      observer.disconnect();
      // Gone with its page, or a new kind of particles: at once.
      if (running) stop();
    };
  }, [kind, density]);

  useEffect(() => {
    wish.current = { on, resting, reduce };
    if (on) wake.current?.();
  }, [on, resting, reduce]);

  return (
    <canvas
      ref={canvasRef}
      aria-hidden
      className={cn(
        "pointer-events-none absolute inset-0 size-full",
        !on && !live && "hidden",
        className,
      )}
    />
  );
}
