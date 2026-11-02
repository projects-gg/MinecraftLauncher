// Faces of voice chat (VOICE-L3; voice.md §5.1): a face with the speaking ring, a thin jade line
// around it that fades in while they speak and out after (opacity only, never moving, nothing at
// all while nobody speaks; the store holds no speaker while the window is away). Round inside a
// pill, the list's rounded square elsewhere (design.md §4, "round inside round").
import { MicrophoneSlash } from "@phosphor-icons/react";
import { cn, SignalBars } from "@pl/ui";
import { SocialFace } from "@/features/social/social-face";
import type { VoiceQuality } from "@/lib/api/voice-types";
import { HeadphonesSlash } from "./icons";
import type { VoiceFace, VoiceFaceMark } from "./rules";
import { useSpeaking } from "./store";

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
