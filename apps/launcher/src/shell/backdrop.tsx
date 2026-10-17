import { EmberField, type ParticleKind, useAmbientMotion } from "@pl/ui";
import { motion, useMotionValue, useSpring, useTransform } from "motion/react";
import { useEffect } from "react";
import { PerfAdvisor } from "@/features/perf/perf-advisor";

/**
 * The season art behind every page: sharp with embers on Home, blurred and dimmed elsewhere.
 * Both layers stay mounted and cross-fade, so navigating never flashes black. It also hosts the
 * one-time Performance mode check, which measures Home's frames (features/perf).
 */
export function Backdrop({
  imageUrl,
  variant,
  particles,
}: {
  imageUrl: string;
  variant: "home" | "page";
  particles: ParticleKind;
}) {
  const ambient = useAmbientMotion();
  const px = useMotionValue(0);
  const py = useMotionValue(0);
  const sx = useSpring(px, { stiffness: 40, damping: 18 });
  const sy = useSpring(py, { stiffness: 40, damping: 18 });
  const x = useTransform(sx, (v) => v * -12);
  const y = useTransform(sy, (v) => v * -8);

  useEffect(() => {
    if (!ambient) {
      px.set(0);
      py.set(0);
      return;
    }
    const onMove = (e: PointerEvent) => {
      px.set(e.clientX / window.innerWidth - 0.5);
      py.set(e.clientY / window.innerHeight - 0.5);
    };
    window.addEventListener("pointermove", onMove, { passive: true });
    return () => window.removeEventListener("pointermove", onMove);
  }, [ambient, px, py]);

  const home = variant === "home";

  return (
    <>
      <div
        aria-hidden
        className="pointer-events-none fixed inset-0 -z-10 overflow-hidden bg-obsidian-950"
      >
        {/* Sharp art (Home) */}
        <motion.div
          className="absolute -inset-6"
          style={{ x, y }}
          initial={false}
          animate={{ opacity: home ? 1 : 0 }}
          transition={{ duration: 0.45 }}
        >
          {/* The drift pauses instead of stopping: switching Performance mode (a toast's button
              on Home), the tray or a game no longer snaps the art back to its starting size,
              and it moves on from the same frame afterwards. It also pauses on the other pages,
              where this layer is invisible: a running animation kept the compositor drawing
              every frame there (OPT1: ~6 % of a CPU core with the OYNA sheen). */}
          <img
            src={imageUrl}
            alt=""
            draggable={false}
            className="size-full animate-kenburns object-cover object-[88%_center]"
            style={{ animationPlayState: ambient && home ? "running" : "paused" }}
          />
        </motion.div>

        {/* Blurred, dimmed art (other pages). A quarter-size copy is blurred and then scaled up
            4.4×: it looks like a 40 px blur of the full image but costs a sixteenth of the paint,
            which matters because this layer appears on every change from Home to another page. */}
        <motion.div
          className="absolute -inset-16 overflow-hidden"
          initial={false}
          animate={{ opacity: home ? 0 : 1 }}
          transition={{ duration: 0.45 }}
        >
          <img
            src={imageUrl}
            alt=""
            draggable={false}
            className="absolute top-0 left-0 h-1/4 w-1/4 origin-top-left scale-[4.4] object-cover blur-[9px] brightness-[0.34] saturate-[1.15] will-change-transform [html[data-perf=on]_&]:blur-none [html[data-perf=on]_&]:brightness-[0.22]"
          />
        </motion.div>

        {/* Scrims keep text legible over bright art (WCAG AA over the darkest-needed region). */}
        <motion.div
          className="absolute inset-0"
          initial={false}
          animate={{ opacity: home ? 1 : 0 }}
          transition={{ duration: 0.45 }}
        >
          <div className="absolute inset-0 bg-[radial-gradient(120%_85%_at_0%_100%,rgb(34_6_6/0.92)_0%,rgb(34_6_6/0.55)_38%,transparent_68%)]" />
          <div className="absolute inset-x-0 bottom-0 h-56 bg-[linear-gradient(to_top,rgb(10_9_12/0.92),transparent)]" />
          <div className="absolute inset-y-0 right-0 w-[420px] bg-[linear-gradient(to_left,rgb(10_9_12/0.7),transparent)]" />
          <div className="absolute inset-x-0 top-0 h-28 bg-[linear-gradient(to_bottom,rgb(10_9_12/0.6),transparent)]" />
        </motion.div>
        <div className="absolute inset-0 bg-[radial-gradient(140%_120%_at_50%_40%,transparent_55%,rgb(10_9_12/0.65))]" />

        {home ? <EmberField kind={particles} /> : null}

        {/* Static film grain on a fixed layer (never on scrolling containers). */}
        <div className="absolute inset-0 opacity-[0.045] mix-blend-overlay [background-image:url('data:image/svg+xml;utf8,<svg xmlns=%22http://www.w3.org/2000/svg%22 width=%22160%22 height=%22160%22><filter id=%22n%22><feTurbulence type=%22fractalNoise%22 baseFrequency=%220.9%22 numOctaves=%222%22 stitchTiles=%22stitch%22/></filter><rect width=%22100%25%22 height=%22100%25%22 filter=%22url(%23n)%22/></svg>')]" />
      </div>
      <PerfAdvisor home={home} />
    </>
  );
}
