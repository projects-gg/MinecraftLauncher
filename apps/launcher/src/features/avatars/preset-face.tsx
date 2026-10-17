import { cn } from "@pl/ui";
import type { PresetAvatar } from "./presets";

/**
 * A bundled picture in the same frame as a player head (square, head radius, hairline ring), so
 * heads and pictures line up as one family wherever an avatar appears. The picture's own accent
 * colour fills the frame until the image has decoded.
 */
export function PresetFace({
  avatar,
  size,
  className,
}: {
  avatar: PresetAvatar;
  size: number;
  className?: string;
}) {
  // The 128 px file covers up to 64 px slots on 2x screens.
  const sharp = size * 2 > 128 ? avatar.large : avatar.small;
  return (
    <span
      className={cn(
        "relative inline-flex shrink-0 overflow-hidden rounded-[var(--radius-head)] ring-1 ring-white/10",
        className,
      )}
      style={{ width: size, height: size, backgroundColor: `${avatar.accent}33` }}
    >
      <img
        src={size > 128 ? avatar.large : avatar.small}
        srcSet={`${avatar.small} 1x, ${sharp} 2x`}
        alt=""
        width={size}
        height={size}
        decoding="async"
        draggable={false}
        className="size-full object-cover"
      />
    </span>
  );
}
