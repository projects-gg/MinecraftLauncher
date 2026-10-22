// Plan reminders (TEAM5): "Geliyorum" and "Belki" ring 10 minutes before the plan while the
// launcher runs, the tray included: a toast with "Göster" while the window is in front, a system
// notification and a taskbar flash while it is away. Nothing while the game runs, on
// do-not-disturb, while the crew is muted or with "Plan hatırlatmaları" off (it counts as told).
// One missed while the launcher was closed or the computer slept says the plan started, during
// its first quarter of an hour. No timer of its own: the call clock moves at each ring
// (use-call-clock.ts). Told reminders are remembered on this computer for a few days.
import { CalendarBlank } from "@phosphor-icons/react";
import { toast } from "@pl/ui";
import { useEffect, useState } from "react";
import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { SocialTeam } from "@/lib/api/social-types";
import { notify, requestAttention, windowAway } from "@/lib/attention";
import i18n from "@/lib/i18n";
import { setPageDuty } from "@/lib/page-duties";
import { onWindowAwayChange } from "@/lib/window-away";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { type DuePlan, duePlans, pruneTold, reminderAhead, withPlanNote } from "./crew-plans";
import { crewMutedNow } from "./mute";
import { useSocialPrefs, useSocialUi } from "./store";
import { useCallClock } from "./use-call-clock";

/** Plan ids whose reminder was told (or passed over), with when (ms). */
export const usePlanTold = create<{
  told: Record<string, number>;
  mark: (ids: string[], at: number) => void;
}>()(
  persist(
    (set) => ({
      told: {},
      mark: (ids, at) =>
        set((s) => ({
          told: pruneTold({ ...s.told, ...Object.fromEntries(ids.map((id) => [id, at])) }, at),
        })),
    }),
    { name: "pl.crew-plans", version: 1 },
  ),
);

/** A reminder toast stays this long. */
const TOAST_MS = 20_000;
/** After start the reminders wait this long before telling anything: the window is being shown. */
const SETTLE_MS = 4000;

function ReminderMark() {
  return <CalendarBlank size={16} weight="duotone" className="mt-0.5 text-gold-300" />;
}

/**
 * Tells one due reminder. False when it has to wait: the window is away and system notifications
 * are off (it is told when the window comes back).
 */
function tell(due: DuePlan, crew: SocialTeam): boolean {
  const phase = useLaunch.getState().phase.kind;
  if (phase === "launching" || phase === "running") return true;
  const text = withPlanNote(
    due.kind === "soon"
      ? i18n.t("crewWave2.plans.startsIn", { crew: crew.name, count: due.minutes })
      : i18n.t("crewWave2.plans.started", { crew: crew.name }),
    due.plan.note,
    i18n.t,
  );
  const showCrew = () => {
    const ui = useSocialUi.getState();
    ui.openPanel({ page: "team" });
    ui.setCrewTab(crew.id);
    ui.setCrewPane("crew");
  };
  if (windowAway()) {
    if (!usePrefs.getState().desktopNotifications) return false;
    void notify(crew.name, text, showCrew);
    requestAttention();
    return true;
  }
  toast(text, {
    duration: TOAST_MS,
    icon: <ReminderMark />,
    action: { label: i18n.t("social.notices.show"), onClick: showCrew },
  });
  return true;
}

/** Mounted once, by SocialHost. Renders nothing. */
export function PlanReminders() {
  const plans = useSocialUi((s) => s.view.plans);
  const crews = useSocialUi((s) => s.view.crews);
  const status = useSocialUi((s) => s.view.me?.status);
  const all = useSocialUi((s) => s.view.crewNotifications);
  const remind = useSocialPrefs((s) => s.notifyPlans);
  const told = usePlanTold((s) => s.told);
  const now = useCallClock((s) => s.now);
  const [settled, setSettled] = useState(false);

  useEffect(() => {
    const id = window.setTimeout(() => setSettled(true), SETTLE_MS);
    return () => window.clearTimeout(id);
  }, []);

  // A reminder that waited for the window is told when it comes back.
  useEffect(() => {
    const again = () => useCallClock.setState({ now: Date.now() });
    const stop = onWindowAwayChange(again);
    window.addEventListener("focus", again);
    return () => {
      stop();
      window.removeEventListener("focus", again);
    };
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `now` asks again (the call clock)
  useEffect(() => {
    const nowMs = Date.now();
    // A reminder ahead keeps the page awake in the tray (lib/page-duties.ts).
    setPageDuty("plans", remind && reminderAhead(plans, told, nowMs));
    if (!remind || !settled) return;
    const due = duePlans(plans, told, nowMs);
    if (!due.length) return;
    const handled = due.filter((one) => {
      const crew = crews.find((c) => c.id === one.plan.crew);
      // Gone from the crew, on do-not-disturb or muted: passed over, as told.
      if (!crew || status === "dnd" || crewMutedNow(crew, all, nowMs)) return true;
      return tell(one, crew);
    });
    if (handled.length) {
      usePlanTold.getState().mark(
        handled.map((one) => one.plan.id),
        nowMs,
      );
    }
  }, [plans, crews, status, all, remind, told, settled, now]);

  useEffect(() => () => setPageDuty("plans", false), []);
  return null;
}
