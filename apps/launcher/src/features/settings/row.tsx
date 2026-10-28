import { cn, Switch, scrollBehavior } from "@pl/ui";
import { motion } from "motion/react";
import {
  type ComponentProps,
  cloneElement,
  createContext,
  isValidElement,
  type ReactNode,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";

/** How long the row a landing asked for stays lit: its plate fades in, holds, fades out. */
export const ARRIVAL_MS = 1800;

/** The row the page just landed on (`Row`'s `setting`), and when: it lights up once. */
export const SettingArrival = createContext<{ id: string; at: number } | null>(null);

/** A quiet plate behind the row for a moment. Opacity only, so reduced motion keeps it. */
function ArrivalPlate() {
  return (
    <motion.span
      aria-hidden
      className="pointer-events-none absolute -inset-x-3 inset-y-1 -z-10 rounded-md bg-white/[0.06]"
      initial={{ opacity: 0 }}
      animate={{ opacity: [0, 1, 1, 0] }}
      transition={{ duration: ARRIVAL_MS / 1000, times: [0, 0.12, 0.7, 1], ease: "easeOut" }}
    />
  );
}

/**
 * One setting: its title and help on the left, its control on the right (a longer label grows
 * left, never wraps the control). `helpId` names the help line, so a control other than a switch
 * can be described by it. `setting` names the row for a landing (`useSettingLanding`).
 */
export function Row({
  title,
  help,
  helpId: helpIdProp,
  setting,
  children,
}: {
  title: string;
  help?: ReactNode;
  helpId?: string;
  setting?: string;
  children: ReactNode;
}) {
  const id = useId();
  const helpId = helpIdProp ?? `${id}-help`;
  const arrival = useContext(SettingArrival);
  // A row whose control is a switch also toggles on a click on its words: its title and help
  // become the switch's label (a target as wide as the row, POLISH2). The switch is named by
  // the title and described by the help (POLISH3: a screen reader read the title only).
  const toggle =
    isValidElement<ComponentProps<typeof Switch>>(children) && children.type === Switch;
  const control = toggle
    ? cloneElement(children, { id, describedBy: help ? helpId : undefined })
    : children;
  const words = (
    <>
      <span className="block text-[13.5px] font-medium text-bone-50">{title}</span>
      {help ? (
        <span id={helpId} className="mt-0.5 block max-w-[56ch] text-[12.5px] text-ash-300">
          {help}
        </span>
      ) : null}
    </>
  );
  return (
    <div
      data-setting={setting}
      className={cn(
        "flex items-center justify-between gap-6 border-b border-white/6 py-3.5 last:border-b-0",
        // The switch lifts while the pointer is on its words too (`hovered:`).
        toggle && "switch-row",
        // A row a landing may light up holds its plate behind its words.
        setting && "relative isolate",
      )}
    >
      {setting && arrival?.id === setting ? <ArrivalPlate key={arrival.at} /> : null}
      {toggle ? (
        <label
          htmlFor={id}
          className={cn("min-w-0", children.props.disabled ? "cursor-default" : "cursor-pointer")}
        >
          {words}
        </label>
      ) : (
        <div className="min-w-0">{words}</div>
      )}
      <div className="shrink-0">{control}</div>
    </div>
  );
}

/**
 * Lands the page on the row `focus` names (`/settings?section=…&focus=…`: the command palette's
 * settings rows, BETA-FB; its Enter flipped Performance mode where the player meant to find it).
 * The row scrolls into view, its control takes the focus (Space flips a switch) and its plate
 * lights up once; then `done` drops the request from the address. Two frames first: the palette
 * hands the focus back to the page as it closes. Returns the arrival for `SettingArrival`.
 */
export function useSettingLanding(focus: string | undefined, done: () => void) {
  const [arrival, setArrival] = useState<{ id: string; at: number } | null>(null);
  const finish = useRef(done);
  finish.current = done;
  useEffect(() => {
    if (!focus) return;
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        const row = document.querySelector<HTMLElement>(`[data-setting="${focus}"]`);
        if (row) {
          row.scrollIntoView({ block: "nearest", behavior: scrollBehavior() });
          const control =
            row.querySelector<HTMLElement>('[role="switch"]') ??
            row.querySelector<HTMLElement>("button, input, select, textarea");
          control?.focus({ preventScroll: true });
          setArrival({ id: focus, at: performance.now() });
        }
        finish.current();
      });
    });
    return () => cancelAnimationFrame(frame);
  }, [focus]);
  useEffect(() => {
    if (!arrival) return;
    const timer = window.setTimeout(() => setArrival(null), ARRIVAL_MS);
    return () => window.clearTimeout(timer);
  }, [arrival]);
  return arrival;
}

/**
 * Help lines that take turns in one grid cell (a row's help that follows its choice): the cell
 * keeps the height of the tallest, so nothing below moves; the old line fades out (100 ms), the
 * new one in (150 ms after 60 ms), as `SwapLabel` does for buttons. Screen readers get the shown
 * line from a live text beside it.
 */
export function SwapLine({ lines, active }: { lines: string[]; active: number }) {
  return (
    <>
      <span aria-hidden className="grid">
        {lines.map((line, i) => (
          <span
            key={line}
            className={cn(
              "[grid-area:1/1] transition-opacity",
              i === active ? "opacity-100 delay-[60ms] duration-150" : "opacity-0 duration-100",
            )}
          >
            {line}
          </span>
        ))}
      </span>
      {/* Heard when the choice, or the game opening, changes the line. */}
      <span className="sr-only" aria-live="polite">
        {lines[active]}
      </span>
    </>
  );
}
