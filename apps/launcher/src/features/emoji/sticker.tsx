// A Projects sticker (CHAT2, docs: chat2.md §8): the still image at its size, reserved before it
// loads and faded in once it has. It moves once (its pattern, sticker-motion.ts) when it arrives
// (`arrive`: a message that came in live) and when the pointer comes onto it or presses it.
// Nothing moves under reduced motion, in Performance mode or while the window is away; an arrival
// then only fades in. A reference this build does not know (a newer set) is a quiet box.
import { Sticker as StickerIcon } from "@phosphor-icons/react";
import type { Sticker as StickerData } from "@pl/stickers";
import { stickerImage } from "@pl/stickers/images";
import { cn, useLiveMotion } from "@pl/ui";
import { type PointerEvent, useEffect, useLayoutEffect, useRef, useState } from "react";
import { fadeInSticker, playStickerMotion } from "./sticker-motion";

export function Sticker({
  sticker,
  size,
  arrive = false,
  alt = "",
  moveOnHover = true,
  className,
}: {
  sticker: StickerData;
  /** CSS pixels: 112 in chat, 64 in the picker. */
  size: number;
  /** Plays its motion once when it mounts (a message that just came in). */
  arrive?: boolean;
  /** Its accessible name; empty where a control around it names it. */
  alt?: string;
  /** Moves when the pointer comes onto it (off where a larger target plays it, the picker cell). */
  moveOnHover?: boolean;
  className?: string;
}) {
  const live = useLiveMotion();
  const box = useRef<HTMLSpanElement>(null);
  const image = useRef<HTMLImageElement>(null);
  const [loaded, setLoaded] = useState(false);
  const liveRef = useRef(live);
  liveRef.current = live;

  // A picture the browser already has is shown at once, without a fade.
  useLayoutEffect(() => {
    if (image.current?.complete && image.current.naturalWidth > 0) setLoaded(true);
  }, []);

  // The arrival waits for the decoded picture (an empty box would move); then once.
  // biome-ignore lint/correctness/useExhaustiveDependencies: only when it mounts
  useEffect(() => {
    if (!arrive) return;
    let cancelled = false;
    const start = () => {
      const element = box.current;
      if (cancelled || !element) return;
      if (liveRef.current) playStickerMotion(element, sticker.motion);
      else fadeInSticker(element);
    };
    const picture = image.current;
    if (picture && typeof picture.decode === "function") picture.decode().then(start, start);
    else start();
    return () => {
      cancelled = true;
    };
  }, []);

  const move = (event: PointerEvent) => {
    if (!live || !box.current) return;
    if (event.type === "pointerenter" && (!moveOnHover || event.pointerType !== "mouse")) return;
    playStickerMotion(box.current, sticker.motion);
  };

  return (
    <span
      ref={box}
      className={cn("relative inline-flex shrink-0", className)}
      style={{ width: size, height: size }}
      onPointerEnter={move}
      onPointerDown={move}
    >
      <img
        ref={image}
        src={stickerImage(sticker)}
        alt={alt}
        width={size}
        height={size}
        loading="lazy"
        decoding="async"
        draggable={false}
        onLoad={() => setLoaded(true)}
        className={cn(
          "size-full object-contain select-none transition-opacity duration-150",
          loaded ? "opacity-100" : "opacity-0",
        )}
      />
    </span>
  );
}

/** A sticker this build does not have (a newer set): the word in a quiet box of the same size. */
export function UnknownSticker({
  size,
  label,
  className,
}: {
  size: number;
  label: string;
  className?: string;
}) {
  return (
    <span
      role="img"
      aria-label={label}
      className={cn(
        "inline-flex shrink-0 flex-col items-center justify-center gap-1.5 rounded-[var(--radius-panel)] bg-white/[0.04] text-[12px] text-ash-400 ring-1 ring-white/8",
        className,
      )}
      style={{ width: size, height: size }}
    >
      <StickerIcon size={Math.max(14, Math.round(size / 4))} aria-hidden />
      {size >= 56 ? <span aria-hidden>{label}</span> : null}
    </span>
  );
}
