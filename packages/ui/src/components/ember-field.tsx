import { useEffect, useRef } from "react";
import { cn } from "../lib/cn";
import { useAmbientMotion } from "../perf";

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

/**
 * Rising ember particles on a single canvas. Runs outside React state; pauses whenever
 * ambient motion is not allowed (reduced motion, Performance mode, hidden window, game running).
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
  const enabled = useAmbientMotion() && kind !== "none";

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !enabled) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let width = 0;
    let height = 0;
    let dpr = 1;
    let frame = 0;
    let last = performance.now();
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
        hue: 28 + Math.random() * 18,
        phase: Math.random() * Math.PI * 2,
      };
    };

    const tick = (now: number) => {
      const dt = Math.min(0.05, (now - last) / 1000);
      last = now;
      ctx.clearRect(0, 0, width, height);
      ctx.globalCompositeOperation = snow ? "source-over" : "lighter";

      while (particles.length < target()) particles.push(spawn(particles.length < target() / 2));

      for (let i = particles.length - 1; i >= 0; i--) {
        const p = particles[i];
        if (!p) continue;
        p.life += dt;
        p.phase += dt * 2.2;
        p.x += (p.vx + Math.sin(p.phase) * (snow ? 10 : 8)) * dt;
        p.y += p.vy * dt;
        const t = p.life / p.maxLife;
        if (t >= 1 || p.y < -20 || p.y > height + 20) {
          particles[i] = spawn(false);
          continue;
        }
        // Fade in fast, out slow; embers flicker.
        const fade = Math.min(1, t * 6) * (1 - t) ** 1.4;
        const flicker = snow ? 1 : 0.75 + Math.sin(p.phase * 3.1) * 0.25;
        const alpha = fade * flicker;
        if (snow) {
          ctx.fillStyle = `rgba(243, 235, 221, ${alpha * 0.75})`;
          ctx.beginPath();
          ctx.arc(p.x, p.y, p.size, 0, Math.PI * 2);
          ctx.fill();
        } else {
          const r = p.size * 3.2;
          const g = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, r);
          g.addColorStop(0, `hsla(${p.hue + 12}, 100%, 78%, ${alpha})`);
          g.addColorStop(0.35, `hsla(${p.hue}, 95%, 56%, ${alpha * 0.55})`);
          g.addColorStop(1, `hsla(${p.hue - 10}, 90%, 40%, 0)`);
          ctx.fillStyle = g;
          ctx.fillRect(p.x - r, p.y - r, r * 2, r * 2);
        }
      }
      frame = requestAnimationFrame(tick);
    };

    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(canvas);
    frame = requestAnimationFrame(tick);

    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      ctx.clearRect(0, 0, width, height);
    };
  }, [enabled, kind, density]);

  return (
    <canvas
      ref={canvasRef}
      aria-hidden
      className={cn(
        "pointer-events-none absolute inset-0 size-full",
        !enabled && "hidden",
        className,
      )}
    />
  );
}
