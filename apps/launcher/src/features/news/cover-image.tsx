import { cn } from "@pl/ui";
import { useState } from "react";
import { blurhashUrl } from "@/lib/blurhash";

/**
 * A news cover filling a box that already holds its place. With a BlurHash the box shows it
 * first and the picture fades in over it once decoded (opacity only, so reduced motion keeps the
 * fade); a picture the browser already has is drawn at once, without a fade on every visit. A
 * picture that is missing or fails to load leaves the BlurHash (or the box's own colour).
 */
export function CoverImage({
  src,
  blurhash,
  className,
  imgClassName,
  lazy,
  zoomMs = 500,
}: {
  src: string | undefined;
  blurhash?: string;
  /** The box: size, shape and background. */
  className?: string;
  /** The picture: hover transforms. */
  imgClassName?: string;
  lazy?: boolean;
  /** How long the hover zoom eases (the big featured card takes its time). */
  zoomMs?: number;
}) {
  const placeholder = blurhashUrl(blurhash);
  const [loaded, setLoaded] = useState<string | undefined>(undefined);
  const [failed, setFailed] = useState<string | undefined>(undefined);
  const showImage = Boolean(src) && failed !== src;
  // Without a placeholder there is nothing to fade from: the picture is simply there.
  const hidden = Boolean(placeholder) && loaded !== src;
  return (
    <span
      className={cn("block bg-cover bg-center", className)}
      style={placeholder ? { backgroundImage: `url("${placeholder}")` } : undefined}
    >
      {showImage ? (
        <img
          key={src}
          src={src}
          alt=""
          loading={lazy ? "lazy" : undefined}
          decoding="async"
          draggable={false}
          ref={(img) => {
            // Already decoded (the browser's cache): shown before the first paint, no fade.
            if (img?.complete && img.naturalWidth > 0 && loaded !== src) setLoaded(src);
          }}
          onLoad={() => setLoaded(src)}
          onError={() => setFailed(src)}
          className={cn("size-full object-cover", hidden && "opacity-0", imgClassName)}
          // The hover zoom (`group-hover:scale-*`) is Tailwind 4's `scale` property, not `transform`.
          style={{
            transition: `opacity 260ms cubic-bezier(0.16, 1, 0.3, 1), scale ${zoomMs}ms ease-out`,
          }}
        />
      ) : null}
    </span>
  );
}
