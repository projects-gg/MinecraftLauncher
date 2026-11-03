// Faces of voice chat (VOICE-L3; voice.md §5.1): a face with the speaking ring, a thin jade line
// around it that fades in while they speak and out after (opacity only, never moving, nothing at
// all while nobody speaks; the store holds no speaker while the window is away). Round inside a
// pill, the list's rounded square elsewhere (design.md §4, "round inside round").
//
// VOICE-CALL: the speaking indicator follows the voice. Outside the ring a second, softer ring
// grows with how loud they speak and settles back between words (transform scale and opacity
// only, written by the live levels: no render, no frame loop), the same on the pill's faces, the
// panel's rows and the call's large faces, where it reaches furthest. The player's own face does
// it only while what they say goes out. Reduced motion and Performance mode: the plain ring.
import { MicrophoneSlash } from "@phosphor-icons/react";
import { cn, SignalBars, useMotionPrefs } from "@pl/ui";
import { useRef } from "react";
import { SocialFace } from "@/features/social/social-face";
import type { VoiceQuality } from "@/lib/api/voice-types";
import { HeadphonesSlash } from "./icons";
import { MIC, usePainter } from "./levels";
import type { VoiceFace, VoiceFaceMark } from "./rules";
import { useSpeaking, useVoiceUi } from "./store";

/** A voice's strength for the halo (0–1): from a quiet word (-39 dBFS) to a loud one (-9). */
export function haloStrength(level: number): number {
  return Math.max(0, Math.min(1, (level - 0.35) / 0.5));
}

/** How far the halo grows at full strength, by the face's size: furthest on the call's faces. */
export function haloReach(size: number): number {
  if (size >= 64) return 0.22;
  // The panel's rows: a dense list, a little less.
  if (size >= 32) return 0.13;
  // The pill's 20 px faces: a few pixels, or it would not show at all.
  return 0.2;
}

/** The ring that follows the voice (absent under reduced motion). */
function Halo({ id, size, round }: { id: string; size: number; round: boolean }) {
  const ref = useRef<HTMLSpanElement>(null);
  const shown = useRef(0);
  const isMe = useVoiceUi((s) => s.state.room?.me.id === id);
  usePainter(isMe ? MIC : id, (level, open) => {
    const element = ref.current;
    if (!element) return;
    const strength = isMe && !open ? 0 : haloStrength(level);
    if (strength === shown.current) return;
    // A voice rises at once and lets go a little slower (a meter's ballistics).
    element.style.transitionDuration = strength > shown.current ? "80ms" : "240ms";
    shown.current = strength;
    element.style.opacity = String(strength * 0.85);
    element.style.transform = `scale(${1 + strength * haloReach(size)})`;
  });
  return (
    <span
      ref={ref}
      aria-hidden
      data-voice-halo=""
      style={{ opacity: 0, transform: "scale(1)" }}
      className={cn(
        "pointer-events-none absolute -inset-[3px] ring-[1.5px] ring-jade-400/60 transition-[transform,opacity] ease-out",
        round ? "rounded-full" : "rounded-[calc(var(--radius-head)+3px)]",
      )}
    />
  );
}

/**
 * What is off on a small face (VOICE-KEYS), as a sign on a dark disc at its lower right corner:
 * the microphone (ash; a moderator's mute in flare) or the headphones (ash). None while both are on.
 */
function FaceMark({ mark, size }: { mark: VoiceFaceMark; size: number }) {
  // About three fifths of the face; the sign a little smaller inside it.
  const disc = Math.max(11, Math.round(size * 0.6));
  const glyph = Math.round(disc * 0.72);
  return (
    <span
      aria-hidden
      data-voice-mark={mark}
      style={{ width: disc, height: disc }}
      className="pointer-events-none absolute -right-1 -bottom-1 flex items-center justify-center rounded-full bg-obsidian-950"
    >
      {mark === "deaf" ? (
        <HeadphonesSlash size={glyph} className="text-ash-300" />
      ) : (
        <MicrophoneSlash
          size={glyph}
          weight="bold"
          className={mark === "moderator" ? "text-flare-400" : "text-ash-300"}
        />
      )}
    </span>
  );
}

/**
 * A connection that is not good, in the launcher's own connection look (design.md §6, the server
 * bars): weak four amber bars, bad two red; nothing while it is good. Still: the bars' fill plays
 * only when a server answers, never here.
 */
export function QualityBars({ quality, className }: { quality: VoiceQuality; className?: string }) {
  if (quality === "good") return null;
  return (
    <SignalBars state="online" latencyMs={quality === "weak" ? 200 : 700} className={className} />
  );
}

export function SpeakingFace({
  face,
  size,
  round = false,
  dim = false,
  speaking: forced,
  mark = null,
  className,
}: {
  face: VoiceFace;
  size: number;
  round?: boolean;
  /** Not here yet (an invite that rings): the face waits at half strength. */
  dim?: boolean;
  /** Overrides the store (previews of a ring card): their own speaking state. */
  speaking?: boolean;
  /** What is off for them, on the face's corner (small faces without a list's marks). */
  mark?: VoiceFaceMark | null;
  className?: string;
}) {
  const live = useSpeaking(face.id);
  const speaking = forced ?? live;
  const { reduce } = useMotionPrefs();
  const radius = round ? "rounded-full" : "rounded-[var(--radius-head)]";
  return (
    <span className={cn("relative inline-flex shrink-0", className)}>
      <span
        className={cn(
          "inline-flex overflow-hidden transition-opacity duration-300",
          radius,
          dim && "opacity-55",
        )}
      >
        <SocialFace source={face} size={size} />
      </span>
      {/* The voice's own strength (VOICE-CALL), under the ring; never for a still preview. */}
      {reduce || forced !== undefined ? null : <Halo id={face.id} size={size} round={round} />}
      {/* Fast in (the voice is heard now), slower out, so short pauses do not flicker. */}
      <span
        aria-hidden
        data-speaking={speaking ? "" : undefined}
        className={cn(
          "pointer-events-none absolute -inset-[3px] ring-2 ring-jade-400 transition-opacity",
          round ? "rounded-full" : "rounded-[calc(var(--radius-head)+3px)]",
          speaking ? "opacity-100 duration-100" : "opacity-0 duration-300",
        )}
      />
      {mark ? <FaceMark mark={mark} size={size} /> : null}
    </span>
  );
}
