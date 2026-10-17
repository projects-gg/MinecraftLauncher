// Flat front view of a skin, drawn with pixel-sharp CSS crops: the file thumbnail and the
// preview where WebGL is not available.
import { cn } from "@pl/ui";
import type { SkinVariant } from "@/lib/api";

interface Part {
  id: string;
  /** Top-left of the face in the texture. */
  tex: [number, number];
  /** Where it sits in the 16×32 front view. */
  at: [number, number];
  size: [number, number];
  /** 64×32 skins reuse the right limbs for the left ones, mirrored. */
  mirror?: boolean;
}

function parts(variant: SkinVariant, legacy: boolean): Part[] {
  const arm = variant === "slim" && !legacy ? 3 : 4;
  const base: Part[] = [
    { id: "head", tex: [8, 8], at: [4, 0], size: [8, 8] },
    { id: "body", tex: [20, 20], at: [4, 8], size: [8, 12] },
    { id: "rightArm", tex: [44, 20], at: [4 - arm, 8], size: [arm, 12] },
    legacy
      ? { id: "leftArm", tex: [44, 20], at: [12, 8], size: [arm, 12], mirror: true }
      : { id: "leftArm", tex: [36, 52], at: [12, 8], size: [arm, 12] },
    { id: "rightLeg", tex: [4, 20], at: [4, 20], size: [4, 12] },
    legacy
      ? { id: "leftLeg", tex: [4, 20], at: [8, 20], size: [4, 12], mirror: true }
      : { id: "leftLeg", tex: [20, 52], at: [8, 20], size: [4, 12] },
  ];
  const hat: Part = { id: "hat", tex: [40, 8], at: [4, 0], size: [8, 8] };
  if (legacy) return [...base, hat];
  return [
    ...base,
    hat,
    { id: "jacket", tex: [20, 36], at: [4, 8], size: [8, 12] },
    { id: "rightSleeve", tex: [44, 36], at: [4 - arm, 8], size: [arm, 12] },
    { id: "leftSleeve", tex: [52, 52], at: [12, 8], size: [arm, 12] },
    { id: "rightPants", tex: [4, 36], at: [4, 20], size: [4, 12] },
    { id: "leftPants", tex: [4, 52], at: [8, 20], size: [4, 12] },
  ];
}

/** Decorative: the surrounding element carries the accessible description. */
export function SkinFlat({
  src,
  variant,
  legacy = false,
  scale,
  className,
}: {
  src: string;
  variant: SkinVariant;
  /** A 64×32 texture. */
  legacy?: boolean;
  /** Screen pixels per texture pixel. */
  scale: number;
  className?: string;
}) {
  const height = legacy ? 32 : 64;
  return (
    <div
      aria-hidden
      className={cn("relative shrink-0", className)}
      style={{ width: 16 * scale, height: 32 * scale }}
    >
      {parts(variant, legacy).map((p) => (
        <span
          key={p.id}
          className="absolute"
          style={{
            left: p.at[0] * scale,
            top: p.at[1] * scale,
            width: p.size[0] * scale,
            height: p.size[1] * scale,
            backgroundImage: `url("${src}")`,
            backgroundSize: `${64 * scale}px ${height * scale}px`,
            backgroundPosition: `${-p.tex[0] * scale}px ${-p.tex[1] * scale}px`,
            imageRendering: "pixelated",
            transform: p.mirror ? "scaleX(-1)" : undefined,
          }}
        />
      ))}
    </div>
  );
}
