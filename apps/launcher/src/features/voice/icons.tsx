// "Sağırlaştırılmış" as players say it, the headphones off (VOICE-KEYS): Phosphor's Headphones (bold)
// with the slash Phosphor's own slashed icons carry (MicrophoneSlash's line, the same gap cut out of
// the shape). Phosphor 2.1 has no HeadphonesSlash; the overlay draws the same cut (Icon.DEAF).
import { useId } from "react";

/** Phosphor's Headphones, bold weight (MIT). */
const HEADPHONES_BOLD =
  "M204.73,51.85A108.07,108.07,0,0,0,20,128v56a28,28,0,0,0,28,28H64a28,28,0,0,0,28-28V144a28,28,0,0,0-28-28H44.84A84.05,84.05,0,0,1,128,44h.64a83.7,83.7,0,0,1,82.52,72H192a28,28,0,0,0-28,28v40a28,28,0,0,0,28,28h16a28,28,0,0,0,28-28V128A107.34,107.34,0,0,0,204.73,51.85ZM64,140a4,4,0,0,1,4,4v40a4,4,0,0,1-4,4H48a4,4,0,0,1-4-4V140Zm148,44a4,4,0,0,1-4,4H192a4,4,0,0,1-4-4V144a4,4,0,0,1,4-4h20Z";

/** The slash, as MicrophoneSlash draws it: from the top left to the bottom right. */
const SLASH = { x1: 48, y1: 48, x2: 208, y2: 224 } as const;
/** The bold stroke, and the gap cut on each side of it. */
const STROKE = 24;
const GAP = 16;

export function HeadphonesSlash({
  size = "1em",
  className,
}: {
  size?: number | string;
  className?: string;
}) {
  // A mask id the url() reference reads safely.
  const id = `hs${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 256 256"
      width={size}
      height={size}
      fill="currentColor"
      className={className}
      aria-hidden
    >
      <mask id={id} maskUnits="userSpaceOnUse" x="0" y="0" width="256" height="256">
        <rect width="256" height="256" fill="#fff" />
        <line {...SLASH} stroke="#000" strokeWidth={STROKE + 2 * GAP} strokeLinecap="round" />
      </mask>
      <path d={HEADPHONES_BOLD} mask={`url(#${id})`} />
      <line {...SLASH} stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  );
}
