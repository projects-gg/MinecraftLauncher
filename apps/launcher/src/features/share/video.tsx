// One share's picture and sound on this page (SHARE): the watch engine's stream in a `<video>`,
// its sound at the player's volume for that person. In the browser preview there are no streams:
// a still picture stands in (share/mock-scenarios.ts).
import { cn } from "@pl/ui";
import { useEffect, useRef } from "react";
import { useVoicePrefs } from "@/features/voice/prefs";
import { useVoiceUi } from "@/features/voice/store";
import { isTauri } from "@/lib/platform";
import { mockFrame } from "@/share/mock-scenarios";
import { useShareStream } from "./store";

export function ShareVideo({
  person,
  n,
  volume,
  muted,
  className,
  fit = "contain",
}: {
  person: string;
  n: number;
  /** Percent, 0–100. */
  volume: number;
  muted: boolean;
  className?: string;
  fit?: "contain" | "cover";
}) {
  const stream = useShareStream(person, n);
  const video = useRef<HTMLVideoElement>(null);
  // Streams sound like voices do: through the chosen output, at "Herkesin sesi", silent while
  // deafened.
  const output = useVoicePrefs((s) => s.settings?.outputDevice ?? null);
  const everyone = useVoicePrefs((s) => s.settings?.outputVolume ?? 100);
  const deaf = useVoiceUi((s) => s.state.room?.me.deaf ?? false);

  useEffect(() => {
    const element = video.current;
    if (!element) return;
    if (element.srcObject !== stream) element.srcObject = stream;
    if (stream) void element.play().catch(() => {});
  }, [stream]);

  useEffect(() => {
    const element = video.current;
    if (!element) return;
    element.volume = Math.max(0, Math.min(1, (volume / 100) * (Math.min(everyone, 100) / 100)));
    element.muted = muted || deaf;
  }, [volume, muted, everyone, deaf]);

  useEffect(() => {
    const element = video.current as
      | (HTMLVideoElement & { setSinkId?: (id: string) => Promise<void> })
      | null;
    if (!element?.setSinkId) return;
    void element.setSinkId(output ?? "").catch(() => {});
  }, [output]);

  if (!isTauri()) {
    return (
      <img
        src={mockFrame(person)}
        alt=""
        draggable={false}
        className={cn("size-full", fit === "cover" ? "object-cover" : "object-contain", className)}
      />
    );
  }
  return (
    // biome-ignore lint/a11y/useMediaCaption: a live screen share has no captions to give
    <video
      ref={video}
      autoPlay
      playsInline
      disablePictureInPicture
      className={cn("size-full", fit === "cover" ? "object-cover" : "object-contain", className)}
    />
  );
}
