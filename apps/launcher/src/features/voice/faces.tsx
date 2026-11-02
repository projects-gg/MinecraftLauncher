// Faces of voice chat (VOICE-L3; voice.md §5.1): a face with the speaking ring, a thin jade line
// around it that fades in while they speak and out after (opacity only, never moving, nothing at
// all while nobody speaks; the store holds no speaker while the window is away). Round inside a
// pill, the list's rounded square elsewhere (design.md §4, "round inside round").
import { cn } from "@pl/ui";
import { SocialFace } from "@/features/social/social-face";
import type { VoiceFace } from "./rules";
import { useSpeaking } from "./store";

export function SpeakingFace({
  face,
  size,
  round = false,
  dim = false,
  speaking: forced,
  className,
}: {
  face: VoiceFace;
  size: number;
  round?: boolean;
  /** Not here yet (an invite that rings): the face waits at half strength. */
  dim?: boolean;
  /** Overrides the store (previews of a ring card): their own speaking state. */
  speaking?: boolean;
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
    </span>
  );
}
