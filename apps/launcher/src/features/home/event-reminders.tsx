// Event reminders ("Hatırlat") on screen: the bell that sets one, and the leaf that tells the
// player 5 minutes before the event starts. The rules are in reminders.ts.
import { BellSimple } from "@phosphor-icons/react";
import { cn, easeOutExpo, IconButton, toast, useMotionPrefs, usePerf } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { type UseNavigateResult, useNavigate } from "@tanstack/react-router";
import { useAnimate } from "motion/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type EventItem } from "@/lib/api";
import { notify, requestAttention, windowAway } from "@/lib/attention";
import i18n from "@/lib/i18n";
import { openLink } from "@/lib/links";
import { onWindowAwayChange } from "@/lib/window-away";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { useReminders } from "@/stores/reminders";
import {
  type DueReminder,
  dueReminders,
  nextRing,
  reconcile,
  ringsAt,
  wakeDelay,
} from "./reminders";

/** The bell swings once when a reminder is set: ±14°, dying out, hung from its top. */
const SWING = [
  "rotate(0deg)",
  "rotate(14deg)",
  "rotate(-10deg)",
  "rotate(6deg)",
  "rotate(-3deg)",
  "rotate(0deg)",
];
const SWING_TIMES = [0, 0.18, 0.4, 0.62, 0.82, 1];

/** The mark of reminder toasts (the state is the icon's colour, docs/design.md §6). */
function ReminderMark() {
  return <BellSimple size={16} weight="duotone" className="mt-0.5 text-gold-300" />;
}

/**
 * The "Hatırlat" toggle beside an upcoming event: a quiet bell, gold and filled while a reminder
 * is set. Setting one swings it once (transform only); under reduced motion and in Performance
 * mode only the fill cross-fades. The first reminder ever explains itself in a toast.
 */
export function ReminderBell({ event, className }: { event: EventItem; className?: string }) {
  const { t } = useTranslation();
  const on = useReminders((s) => event.id in s.items);
  const { reduce } = useMotionPrefs();
  const { performanceMode } = usePerf();
  const [scope, animate] = useAnimate<HTMLSpanElement>();

  const press = () => {
    const reminders = useReminders.getState();
    const set = reminders.toggle(event);
    if (scope.current) {
      if (set && !reduce && !performanceMode) {
        animate(
          scope.current,
          { transform: SWING },
          { duration: 0.42, times: SWING_TIMES, ease: "easeInOut" },
        );
      } else {
        // Turned off mid-swing: it settles instead of jumping.
        animate(
          scope.current,
          { transform: "rotate(0deg)" },
          { duration: 0.12, ease: easeOutExpo },
        );
      }
    }
    // Less than five minutes left: the reminder itself comes at once instead. The explanation
    // follows the bell a beat later: mounting the first toast is a long task that would hold
    // back the bell's fill while its swing already plays.
    if (set && !reminders.explained && ringsAt(event.startsAt) > Date.now()) {
      reminders.setExplained();
      window.setTimeout(() => toast(t("reminders.explained"), { icon: <ReminderMark /> }), 180);
    }
  };

  return (
    <IconButton
      size="sm"
      label={t("reminders.bell")}
      tooltipSide="top"
      aria-pressed={on}
      data-bell=""
      onClick={press}
      className={className}
      icon={
        <span ref={scope} className="relative flex origin-[50%_14%]">
          <BellSimple
            size={16}
            className={cn("transition-opacity duration-200 ease-out", on && "opacity-0")}
          />
          <BellSimple
            size={16}
            weight="duotone"
            className={cn(
              "absolute inset-0 text-gold-300 transition-opacity duration-200 ease-out",
              !on && "opacity-0",
            )}
          />
        </span>
      }
    />
  );
}

/** Where "Göster" goes: the event's article, or its link (the event card's target). */
function eventTarget(event: EventItem, navigate: UseNavigateResult<string>) {
  const { newsId, url } = event;
  if (newsId) return () => void navigate({ to: "/", search: { news: newsId } });
  if (url) return () => openLink(url);
  return undefined;
}

/** A reminder toast stays this long. */
const TOAST_MS = 20_000;

/**
 * Tells one due reminder. Returns false when it has to wait: the window is away and system
 * notifications are off (it is told when the window comes back).
 */
function tell(due: DueReminder, navigate: UseNavigateResult<string>): boolean {
  // In game nothing shows (the reminder counts as told).
  const phase = useLaunch.getState().phase.kind;
  if (phase === "launching" || phase === "running") return true;
  const { event } = due;
  const soon = due.kind === "soon";
  if (windowAway()) {
    if (!usePrefs.getState().desktopNotifications) return false;
    void notify(
      event.title,
      soon ? i18n.t("reminders.startsIn", { count: due.minutes }) : i18n.t("reminders.running"),
    );
    requestAttention();
    return true;
  }
  const show = eventTarget(event, navigate);
  toast(
    soon
      ? i18n.t("reminders.startsInToast", { title: event.title, count: due.minutes })
      : i18n.t("reminders.runningToast", { title: event.title }),
    {
      duration: TOAST_MS,
      icon: <ReminderMark />,
      action: show ? { label: i18n.t("reminders.show"), onClick: show } : undefined,
    },
  );
  return true;
}

/** After start the reminders wait this long before telling anything: the window is being shown. */
const SETTLE_MS = 4000;

/**
 * The reminders the player set, told 5 minutes before each event starts (while the launcher
 * runs, the tray included): a toast with "Göster" while the window is in front, a system
 * notification and a taskbar flash while it is away, nothing while the game runs. A reminder
 * missed while the launcher was closed or the computer slept says the event is on, if it still
 * is. One timer, never longer than a minute. Renders nothing.
 */
export function EventReminders() {
  const navigate = useNavigate();
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  const events = data?.events;
  const items = useReminders((s) => s.items);
  const [settled, setSettled] = useState(false);
  const [wake, setWake] = useState(0);

  useEffect(() => {
    const id = window.setTimeout(() => setSettled(true), SETTLE_MS);
    return () => window.clearTimeout(id);
  }, []);

  // A reminder that waited for the window is told when it comes back.
  useEffect(() => {
    const again = () => setWake((n) => n + 1);
    const stop = onWindowAwayChange(again);
    window.addEventListener("focus", again);
    return () => {
      stop();
      window.removeEventListener("focus", again);
    };
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `wake` asks again (timer, window back)
  useEffect(() => {
    if (!events) return;
    const now = Date.now();
    const reminders = useReminders.getState();
    const current = reconcile(reminders.items, events, now);
    // Each store change runs this again with the new reminders.
    if (current !== reminders.items) {
      reminders.replace(current);
      return;
    }
    if (settled) {
      const told = dueReminders(current, events, now).filter((due) => tell(due, navigate));
      if (told.length) {
        reminders.markFired(
          told.map((due) => due.event.id),
          now,
        );
        return;
      }
    }
    const next = nextRing(current, events, now);
    if (next === undefined) return;
    const id = window.setTimeout(() => setWake((n) => n + 1), wakeDelay(next, now));
    return () => window.clearTimeout(id);
  }, [events, items, navigate, settled, wake]);

  return null;
}
