// One share's picture on this page (SHARE, SQOL-U1): the watch engine's stream in a `<video>`,
// always muted. It only shows the picture; the sound plays through WebAudio (share/stream-audio.ts:
// "Yayın Sesi" up to 200 %, ducking, the output device). In the browser preview there are no
// streams: a still picture stands in (share/mock-scenarios.ts).
import { cn } from "@pl/ui";
import { useEffect, useRef } from "react";
import { isTauri } from "@/lib/platform";
import { mockFrame } from "@/share/mock-scenarios";
import { useShareStream } from "./store";

export function ShareVideo({
  person,
  n,
  className,
  fit = "contain",
}: {
  person: string;
  n: number;
  className?: string;
  fit?: "contain" | "cover";
}) {
  const stream = useShareStream(person, n);
  const video = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const element = video.current;
    if (!element) return;
    // Never its own sound: the stream audio plays it (twice would echo).
    element.muted = true;
    if (element.srcObject !== stream) element.srcObject = stream;
    if (stream) void element.play().catch(() => {});
  }, [stream]);

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
      muted
      playsInline
      disablePictureInPicture
      className={cn("size-full", fit === "cover" ? "object-cover" : "object-contain", className)}
    />
  );
}
