import { useEffect, useState } from "react";
import { cn } from "../lib/cn";
import { usePerf } from "../perf";

function parts(ms: number) {
  const total = Math.max(0, Math.floor(ms / 1000));
  return {
    days: Math.floor(total / 86400),
    hours: Math.floor((total % 86400) / 3600),
    minutes: Math.floor((total % 3600) / 60),
    seconds: total % 60,
  };
}

/**
 * Event countdown in Tektur digits. Units are localized by the caller. The day cell shows only
 * while there are days left ("00 gün" said nothing); hours stay even at zero. It stops ticking
 * while nobody can see it (`usePerf().unseen`) and is right again the moment it is back.
 *
 * Every cell is as wide as its digits (tabular, so "00" and "59" match) and a longer unit hangs
 * into the gap after it ("dakika", "saniye"): the digit groups keep one rhythm in every language
 * and the timer never grows next to the card's "Katıl" (POLISH2). Units stay about as short as
 * the digits (design guide §7).
 */
export function Countdown({
  to,
  units,
  className,
}: {
  to: Date;
  units: { days: string; hours: string; minutes: string; seconds: string };
  className?: string;
}) {
  const [now, setNow] = useState(() => Date.now());
  const { unseen } = usePerf();
  useEffect(() => {
    if (unseen) return;
    setNow(Date.now());
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [unseen]);
  const p = parts(to.getTime() - now);
  const cells: Array<[number, string]> = [
    ...(p.days > 0 ? [[p.days, units.days] as [number, string]] : []),
    [p.hours, units.hours],
    [p.minutes, units.minutes],
    [p.seconds, units.seconds],
  ];
  return (
    <div className={cn("flex items-end gap-2.5", className)} role="timer">
      {cells.map(([value, unit]) => (
        <div key={unit} className="flex flex-col items-start">
          <span className="font-display text-[22px] leading-none font-semibold text-bone-50 tabular-nums">
            {String(value).padStart(2, "0")}
          </span>
          <span className="mt-1 w-0 text-[11px] font-medium whitespace-nowrap text-ash-300">
            {unit}
          </span>
        </div>
      ))}
    </div>
  );
}
