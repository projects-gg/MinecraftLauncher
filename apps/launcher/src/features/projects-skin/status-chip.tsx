// The preview's state chip: "Projects'te", "Önizleme" or "Sırada" (skin1.md §13.5). Every wording
// sits in one grid cell, so the chip keeps the widest one's width and nothing beside it moves; the
// old words fade out (100 ms) and the new ones in (150 ms after 60 ms). "Sırada" carries a gold
// dot that breathes (1.2 s) only while `breathing` (a job in line, the window in sight, at most a
// minute; never under reduced motion or in Performance mode).
import { cn } from "@pl/ui";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { ChipKind } from "./rules";

const KINDS: readonly ChipKind[] = ["projects", "preview", "inLine"];

function BreathingDot({ breathing }: { breathing: boolean }) {
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const dot = ref.current;
    if (!dot || !breathing) return;
    const animation = dot.animate([{ opacity: 1 }, { opacity: 0.3 }, { opacity: 1 }], {
      duration: 1200,
      iterations: Number.POSITIVE_INFINITY,
      easing: "ease-in-out",
    });
    return () => animation.cancel();
  }, [breathing]);
  return <span ref={ref} aria-hidden className="size-[6px] shrink-0 rounded-full bg-gold-400" />;
}

export function StatusChip({
  kind,
  breathing,
  className,
}: {
  kind: ChipKind | null;
  breathing: boolean;
  className?: string;
}) {
  const { t } = useTranslation();
  return (
    <span
      role="status"
      className={cn(
        "chamfer inline-grid h-[22px] items-center px-[8px] text-[11.5px] font-semibold whitespace-nowrap [--cut:5px]",
        "transition-[background-color,color,opacity] duration-150",
        kind === null && "opacity-0",
        kind === "projects" ? "bg-gold-500/28 text-gold-200" : "bg-obsidian-950/70 text-bone-100",
        className,
      )}
    >
      {KINDS.map((option) => (
        <span
          key={option}
          aria-hidden={option === kind ? undefined : true}
          className={cn(
            "flex items-center gap-1.5 [grid-area:1/1] transition-opacity",
            option === kind ? "opacity-100 delay-[60ms] duration-150" : "opacity-0 duration-100",
          )}
        >
          {option === "inLine" ? <BreathingDot breathing={breathing && kind === "inLine"} /> : null}
          {t(`projectsSkin.chip.${option}`)}
        </span>
      ))}
    </span>
  );
}
