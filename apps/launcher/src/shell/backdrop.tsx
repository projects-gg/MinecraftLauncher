import { EmberField, type ParticleKind, useAmbientMotion, useLiveMotion } from "@pl/ui";
import { motion, useMotionValue, useSpring, useTransform } from "motion/react";
import { useEffect, useRef } from "react";
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
  // The drift waits for the player (it rests with the embers); the parallax only ever follows
  // the pointer, so it needs no rest of its own and holds where it is when the player stops.
  const ambient = useAmbientMotion();
  const follow = useLiveMotion();
  const px = useMotionValue(0);
  const py = useMotionValue(0);
  const sx = useSpring(px, { stiffness: 40, damping: 18 });
  const sy = useSpring(py, { stiffness: 40, damping: 18 });
  const x = useTransform(sx, (v) => v * -12);
  const y = useTransform(sy, (v) => v * -8);

  useEffect(() => {
    if (!follow) {
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
  }, [follow, px, py]);

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
              every frame there (OPT1: ~6 % of a CPU core with the OYNA sheen). And it pauses
              while the player rests: it runs on the compositor, but every frame of it is a
              composite of the whole scene under the glass blurs, at the display's rate (OPT2).
              It moves a few pixels a second at most, so holding and moving on never shows. */}
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
            className="absolute top-0 left-0 h-1/4 w-1/4 origin-top-left scale-[4.4] object-cover blur-[9px] brightness-[0.34] saturate-[1.15] will-change-transform [html[data-perf=on]_&]:hidden"
          />
          {/* Performance mode: the same art blurred once, ahead of time (TASTE-SWEEP, BETA-FB).
              Without a filter the quarter-size copy above showed sharp and busy behind the pages
              (its blur was simply switched off); a filter on that layer costs the compositor
              every frame, so this one has none. */}
          <PerfArt imageUrl={imageUrl} />
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

/** The art's width in the pre-blurred copy: stretched over the window it reads as a ~40 px blur. */
const PERF_ART_WIDTH = 48;

/**
 * Performance mode's page backdrop: the art drawn once into a tiny canvas (dimmed as the blurred
 * layer is, brightness 0.22) and stretched over the window, so it is blurred with no filter at all.
 * Hidden outside Performance mode; drawn again only when the season art changes. A canvas shows
 * an image from another origin even when it cannot be read back, so nothing is exported.
 */
function PerfArt({ imageUrl }: { imageUrl: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const image = new Image();
    let live = true;
    image.decoding = "async";
    image.onload = () => {
      const canvas = ref.current;
      if (!live || !canvas || image.naturalWidth === 0) return;
      const width = PERF_ART_WIDTH;
      const height = Math.max(1, Math.round((image.naturalHeight / image.naturalWidth) * width));
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext("2d");
      if (!context) return;
      context.imageSmoothingEnabled = true;
      context.imageSmoothingQuality = "high";
      context.drawImage(image, 0, 0, width, height);
      // brightness(0.22) without a filter: black over it at 78 %.
      context.fillStyle = "rgb(0 0 0 / 0.78)";
      context.fillRect(0, 0, width, height);
    };
    image.src = imageUrl;
    return () => {
      live = false;
    };
  }, [imageUrl]);
  return (
    <canvas
      ref={ref}
      aria-hidden
      className="absolute inset-0 hidden size-full object-cover [html[data-perf=on]_&]:block"
    />
  );
}
