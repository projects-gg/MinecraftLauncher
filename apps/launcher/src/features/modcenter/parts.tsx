import { Progress } from "@pl/ui";
import type { JobProgress } from "@/lib/api";
import { modStageLabel } from "./notices";

/** A live percentage: Tektur digits (design.md: live numbers), the sign in the UI font. */
export function Percent({ value }: { value: number }) {
  return (
    <span className="font-display font-semibold text-gold-200 tabular-nums">
      <span className="font-sans font-medium">%</span>
      {Math.round(value)}
    </span>
  );
}

/** Stage, percentage and bar of a running mod center job (applying, switching, moving). */
export function JobLine({
  progress,
  label,
  muted = false,
}: {
  progress: JobProgress | null;
  /** Accessible name of the bar. */
  label: string;
  muted?: boolean;
}) {
  return (
    <div className="w-full">
      <p
        className={
          muted
            ? "mb-1.5 flex items-baseline justify-between gap-3 text-[12.5px] text-ash-300"
            : "mb-1.5 flex items-baseline justify-between gap-3 text-[13px] text-bone-100"
        }
        aria-live="polite"
      >
        <span>{modStageLabel(progress?.stage)}</span>
        {typeof progress?.percent === "number" ? <Percent value={progress.percent} /> : null}
      </p>
      <Progress value={progress?.percent} label={label} />
    </div>
  );
}
