import { useEffect, useState } from "react";
import { cn } from "../lib/cn";
import { usePerf } from "../perf";

export type CountdownUnit = "days" | "hours" | "minutes" | "seconds";

/**
 * The cells a countdown shows `ms` before its moment, largest first: days, hours and minutes while
 * days are left; hours and minutes on the last day; the minutes alone in the last hour; the
 * seconds only in the last minute. A ticking seconds cell said nothing the minutes did not, and
 * every change on screen keeps WebView2 drawing for about a second: once a minute is enough
 * (OPT2, HOME-TASTE).
 */
export function countdownCells(ms: number): Array<[value: number, unit: CountdownUnit]> {
  const total = Math.max(0, Math.floor(ms / 1000));
  const days = Math.floor(total / 86400);
  const hours = Math.floor((total % 86400) / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  if (days > 0) {
    return [
      [days, "days"],
      [hours, "hours"],
      [minutes, "minutes"],
    ];
  }
  if (hours > 0) {
    return [
      [hours, "hours"],
      [minutes, "minutes"],
    ];
  }
  if (total >= 60) return [[minutes, "minutes"]];
  return [[total, "seconds"]];
}

/**
 * How long until the shown digits change, in ms, `ms` before the moment; null once it has come.
 * The countdown renders just then instead of every second: a minute apart until the last minute.
 */
export function countdownTickMs(ms: number): number | null {
  if (ms <= 0) return null;
  const unit = ms >= 60_000 ? 60_000 : 1000;
  // The digits round down, so they change 1 ms past the boundary below.
  return (ms % unit) + 1;
}

/**
 * Event countdown in Tektur digits. Units are localized by the caller. It shows only the cells
 * that say something (`countdownCells`), changes once a minute until the last minute, and stops
 * while nobody can see it (`usePerf().unseen`); it is right again the moment it is back: the time
 * is read at every render.
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
  units: Record<CountdownUnit, string>;
  className?: string;
}) {
  const { unseen } = usePerf();
  const [ticks, tick] = useState(0);
  const left = to.getTime() - Date.now();
  const cells = countdownCells(left);
  const wait = unseen ? null : countdownTickMs(left);
  useEffect(() => {
    // Armed again after every tick, also when the next wait equals the last one.
    void ticks;
    if (wait === null) return;
    const id = window.setTimeout(() => tick((n) => n + 1), wait);
    return () => window.clearTimeout(id);
  }, [wait, ticks]);
  return (
    <div className={cn("flex items-end gap-2.5", className)} role="timer">
      {cells.map(([value, unit]) => (
        <div key={unit} className="flex flex-col items-start">
          <span className="font-display text-[22px] leading-none font-semibold text-bone-50 tabular-nums">
            {String(value).padStart(2, "0")}
          </span>
          <span className="mt-1 w-0 text-[11px] font-medium whitespace-nowrap text-ash-300">
            {units[unit]}
          </span>
        </div>
      ))}
    </div>
  );
}
