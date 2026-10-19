import { useEffect, useState } from "react";
import { cn } from "../lib/cn";
import { usePerf } from "../perf";

const DAY_MS = 86_400_000;

/**
 * The digits a countdown shows `ms` before its moment: days (while there are any), hours and
 * minutes, and the seconds on the last day only (`seconds` is null before it). Further away a
 * ticking seconds cell said nothing the minutes did not, and every change on screen keeps WebView2
 * drawing for about a second: on Home it was the last thing awake when nobody used the launcher,
 * ~1 % of a CPU core at 144 Hz, also behind other windows (OPT2).
 */
export function countdownCells(ms: number) {
  const total = Math.max(0, Math.floor(ms / 1000));
  const days = Math.floor(total / 86400);
  return {
    days,
    hours: Math.floor((total % 86400) / 3600),
    minutes: Math.floor((total % 3600) / 60),
    seconds: days > 0 ? null : total % 60,
  };
}

/**
 * How long until the shown digits change, in ms, `ms` before the moment; null once it has come.
 * The countdown renders just then instead of every second: a minute apart while days are left.
 */
export function countdownTickMs(ms: number): number | null {
  if (ms <= 0) return null;
  const unit = ms >= DAY_MS ? 60_000 : 1000;
  // The digits round down, so they change 1 ms past the boundary below.
  return (ms % unit) + 1;
}

/**
 * Event countdown in Tektur digits. Units are localized by the caller. The day cell shows only
 * while there are days left ("00 gün" said nothing) and the seconds only on the last day; hours
 * stay even at zero, so it is always three cells. It stops ticking while nobody can see it
 * (`usePerf().unseen`) and is right again the moment it is back: the time is read at every
 * render.
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
  const { unseen } = usePerf();
  const [ticks, tick] = useState(0);
  const left = to.getTime() - Date.now();
  const p = countdownCells(left);
  const wait = unseen ? null : countdownTickMs(left);
  useEffect(() => {
    // Armed again after every tick, also when the next wait equals the last one.
    void ticks;
    if (wait === null) return;
    const id = window.setTimeout(() => tick((n) => n + 1), wait);
    return () => window.clearTimeout(id);
  }, [wait, ticks]);
  const cells: Array<[number, string]> =
    p.seconds === null
      ? [
          [p.days, units.days],
          [p.hours, units.hours],
          [p.minutes, units.minutes],
        ]
      : [
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
