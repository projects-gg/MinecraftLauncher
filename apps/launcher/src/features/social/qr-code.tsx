// The friend code QR, drawn from the module path the backend computes: dark modules on warm
// paper (every phone camera reads dark-on-light), crisp at any size, with a quiet margin.
import type { QrCode as QrData } from "@/lib/api/social-types";

const MARGIN = 2;

export function QrCode({ qr, size, label }: { qr: QrData; size: number; label: string }) {
  const box = qr.size + MARGIN * 2;
  return (
    <svg
      role="img"
      aria-label={label}
      width={size}
      height={size}
      viewBox={`${-MARGIN} ${-MARGIN} ${box} ${box}`}
      shapeRendering="crispEdges"
      className="shrink-0 rounded-md"
    >
      <rect x={-MARGIN} y={-MARGIN} width={box} height={box} fill="var(--color-bone-100)" />
      <path d={qr.path} fill="var(--color-obsidian-900)" />
    </svg>
  );
}
