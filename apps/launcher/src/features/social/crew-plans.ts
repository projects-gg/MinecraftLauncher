// Crew plans ("Planlar", TEAM5): a time to meet on Projects and who is coming. Pure rules of the UI
// (no React), each with a unit test (crew-plans.test.ts): the times the form offers (whole quarters
// of an hour today and on the next six days, never one gone by), a plan's phase on this computer's
// clock, the line Home shows, when a reminder rings and the next moment anything about plans
// changes on screen. A plan's `at` is the service's time (what is shown); `startsAt` and `endsAt`
// are the same moments on this computer's clock (what timers and phases use).
import type { TFunction } from "i18next";
import type { CrewPlan, PlanAnswer, SocialTeam } from "@/lib/api/social-types";
import type { EventItem } from "@/lib/api/types";
import { dateTimeFormat } from "@/lib/format";
import { allows } from "./teams";

/** Plans start on a whole quarter of an hour (seconds). */
export const PLAN_STEP = 900;
/** The form offers today and the six days after (all within the service's 7 days ahead). */
export const PLAN_DAYS = 7;
/** A crew has at most this many plans ahead. */
export const MAX_PLANS = 3;
/** "Geliyorum" and "Belki" ring this long before the plan (seconds). */
export const REMINDER_LEAD = 600;
/** Home's line tells about a plan this long before it starts (seconds). */
export const PLAN_SOON = 3600;
/** The earliest time offered is this far ahead (seconds): the hub's clock may run a little ahead. */
export const MIN_LEAD = 300;
/** A reminder missed while the launcher was closed or the computer slept is still told this long
 * after the start ("Plan başladı"); later the crew page and Home show the plan under way. */
export const LATE_TELL = 900;
/** While a reminder waits, the clock looks again at least this often (ms): a computer that slept
 * past a ring hears it within this long of waking (timers do not count the time asleep). */
export const REMINDER_CHECK_MS = 15 * 60_000;

const DAY_MS = 86_400_000;

/** Local midnight of the day of `ms`, `days` later. */
function midnight(ms: number, days = 0): number {
  const date = new Date(ms);
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days).getTime();
}

const pad = (n: number) => String(n).padStart(2, "0");

/** A day the form offers. */
export interface PlanDay {
  /** Local `YYYY-MM-DD`. */
  key: string;
  /** Days after today: 0 today, 1 tomorrow. */
  offset: number;
  /** Its local midnight (ms). */
  start: number;
}

function dayAt(nowMs: number, offset: number): PlanDay {
  const start = midnight(nowMs, offset);
  const date = new Date(start);
  return {
    key: `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`,
    offset,
    start,
  };
}

/** The day's whole quarters of an hour the form offers (Unix seconds): at least 5 minutes ahead,
 * at most 7 days. */
export function planTimes(day: PlanDay, nowMs: number): number[] {
  const now = nowMs / 1000;
  const end = midnight(day.start, 1) / 1000;
  const out: number[] = [];
  for (let at = Math.ceil(day.start / 1000 / PLAN_STEP) * PLAN_STEP; at < end; at += PLAN_STEP) {
    if (at >= now + MIN_LEAD && at <= now + PLAN_DAYS * 86_400 - MIN_LEAD) out.push(at);
  }
  return out;
}

/** Today (while a quarter of it is left) and the days after: seven days in all. */
export function planDays(nowMs: number): PlanDay[] {
  const days: PlanDay[] = [];
  for (let offset = 0; offset <= PLAN_DAYS && days.length < PLAN_DAYS; offset += 1) {
    const day = dayAt(nowMs, offset);
    if (planTimes(day, nowMs).length) days.push(day);
  }
  return days;
}

/** The day a time falls on, among the offered ones. */
export function dayOf(at: number, days: readonly PlanDay[]): PlanDay | undefined {
  const start = midnight(at * 1000);
  return days.find((day) => day.start === start);
}

/**
 * Where the form starts: tonight at 21:00 while that is half an hour away or more, else the next
 * whole hour at least half an hour away (before midnight), else tomorrow at 21:00.
 */
export function defaultPlanTime(nowMs: number): number {
  const now = new Date(nowMs);
  const at = (days: number, hour: number) =>
    new Date(now.getFullYear(), now.getMonth(), now.getDate() + days, hour).getTime();
  const halfHour = 30 * 60_000;
  const tonight = at(0, 21);
  if (tonight - nowMs >= halfHour) return tonight / 1000;
  let hour = at(0, now.getHours() + 1);
  if (hour - nowMs < halfHour) hour += 3_600_000;
  return (hour < at(1, 0) ? hour : at(1, 21)) / 1000;
}

/** A local hour and minute of a time (Unix seconds). */
export function clockOf(at: number): { hour: number; minute: number } {
  const date = new Date(at * 1000);
  return { hour: date.getHours(), minute: date.getMinutes() };
}

/**
 * The offered time closest to a day and a wall clock time: the same hour and minute on that day,
 * else the day's first time at or after it, else its last one (today, past times are not offered).
 */
export function timeOn(day: PlanDay, hour: number, minute: number, nowMs: number): number | null {
  const times = planTimes(day, nowMs);
  const wanted = new Date(day.start);
  wanted.setHours(hour, minute, 0, 0);
  const target = wanted.getTime() / 1000;
  return times.find((at) => at >= target) ?? times.at(-1) ?? null;
}

// --- a plan on screen ---------------------------------------------------------------------------

export type PlanPhase = "upcoming" | "soon" | "live" | "over";

/** Ahead, within the hour, under way (its two hours), or over. */
export function planPhase(plan: Pick<CrewPlan, "startsAt" | "endsAt">, nowMs: number): PlanPhase {
  const now = nowMs / 1000;
  if (now >= plan.endsAt) return "over";
  if (now >= plan.startsAt) return "live";
  return plan.startsAt - now <= PLAN_SOON ? "soon" : "upcoming";
}

/** The crew's plans on screen (not over), soonest first. */
export function plansOf(plans: readonly CrewPlan[], crew: string, nowMs: number): CrewPlan[] {
  return plans
    .filter((plan) => plan.crew === crew && planPhase(plan, nowMs) !== "over")
    .sort((a, b) => a.at - b.at || a.id.localeCompare(b.id));
}

/** The crew's plans still ahead: the service allows three (those under way do not count). */
export function plansAhead(plans: readonly CrewPlan[], crew: string, nowMs: number): number {
  return plans.filter((plan) => plan.crew === crew && plan.startsAt > nowMs / 1000).length;
}

/** The player may plan here: the service has plans and their role may call the crew (the right
 * to call covers plans). */
export function canPlan(
  team: Pick<SocialTeam, "role" | "moderator" | "permissions">,
  features: { plans: boolean },
): boolean {
  return features.plans && allows(team.permissions.call, team);
}

/** Who said what, in the order the service sent them. */
export function answersBy(plan: Pick<CrewPlan, "answers">): Record<PlanAnswer, string[]> {
  const by: Record<PlanAnswer, string[]> = { yes: [], maybe: [], no: [] };
  for (const { id, answer } of plan.answers) by[answer].push(id);
  return by;
}

/** "Bugün 21:00", "Yarın 20:30", "Cumartesi 21:00": the plan's time on its day ("bugün 21:00"
 * inside a sentence). */
export function planWhen(at: number, nowMs: number, t: TFunction, inSentence = false): string {
  const date = new Date(at * 1000);
  const time = dateTimeFormat({ hour: "2-digit", minute: "2-digit" }).format(date);
  const days = Math.round((midnight(at * 1000) - midnight(nowMs)) / DAY_MS);
  if (days === 0)
    return t(inSentence ? "crewWave2.plans.todayIn" : "crewWave2.plans.today", { time });
  if (days === 1) {
    return t(inSentence ? "crewWave2.plans.tomorrowIn" : "crewWave2.plans.tomorrow", { time });
  }
  const day = dateTimeFormat({ weekday: "long" }).format(date);
  return t("crewWave2.plans.onDay", { day, time });
}

/** The day chip's word: "Bugün", "Yarın", then the weekday's short name ("Cmt"). */
export function dayChip(day: PlanDay, t: TFunction): string {
  if (day.offset === 0) return t("crewWave2.plans.form.today");
  if (day.offset === 1) return t("crewWave2.plans.form.tomorrow");
  return dateTimeFormat({ weekday: "short" }).format(new Date(day.start));
}

/** Studio's event at the plan's time, if there is one (its title shows on the card). */
export function eventDuring(events: readonly EventItem[] | undefined, at: number) {
  const ms = at * 1000;
  return events?.find((event) => Date.parse(event.startsAt) <= ms && ms < Date.parse(event.endsAt));
}

// --- Home's line ---------------------------------------------------------------------------------

/**
 * The plan Home's line tells about: one under way, else one starting within the hour, in the
 * player's crews; never one they said "Gelemem" to, nor one whose line they closed here.
 */
export function homePlan(
  plans: readonly CrewPlan[],
  nowMs: number,
  closed: readonly string[] = [],
): { plan: CrewPlan; phase: "live" | "soon" } | null {
  const shown = plans
    .filter((plan) => plan.myAnswer !== "no" && !closed.includes(plan.id))
    .map((plan) => ({ plan, phase: planPhase(plan, nowMs) }))
    .filter(
      (item): item is { plan: CrewPlan; phase: "live" | "soon" } =>
        item.phase === "live" || item.phase === "soon",
    )
    .sort(
      (a, b) => Number(b.phase === "live") - Number(a.phase === "live") || a.plan.at - b.plan.at,
    );
  return shown[0] ?? null;
}

// --- reminders -------------------------------------------------------------------------------------

/** "Geliyorum" and "Belki" ring; "Gelemem" and no answer do not. */
export function remindable(plan: Pick<CrewPlan, "myAnswer">): boolean {
  return plan.myAnswer === "yes" || plan.myAnswer === "maybe";
}

/** When a plan's reminder rings (Unix seconds, this computer's clock). */
export function ringsAt(plan: Pick<CrewPlan, "startsAt">): number {
  return plan.startsAt - REMINDER_LEAD;
}

/** A reminder whose moment has come: the plan starts in a few minutes, or started just now. */
export type DuePlan =
  | { plan: CrewPlan; kind: "soon"; minutes: number }
  | { plan: CrewPlan; kind: "started" };

/**
 * The reminders to tell now, soonest first: answered "Geliyorum" or "Belki", their moment passed,
 * not told yet (`told`: plan ids). One missed while the launcher was closed or the computer slept
 * says that the plan started, during its first quarter of an hour; later nothing.
 */
export function duePlans(
  plans: readonly CrewPlan[],
  told: Readonly<Record<string, number>>,
  nowMs: number,
): DuePlan[] {
  const now = nowMs / 1000;
  const due: DuePlan[] = [];
  for (const plan of plans) {
    if (!remindable(plan) || plan.id in told || ringsAt(plan) > now) continue;
    if (now < plan.startsAt) {
      due.push({ plan, kind: "soon", minutes: Math.max(1, Math.ceil((plan.startsAt - now) / 60)) });
    } else if (now < plan.startsAt + LATE_TELL) {
      due.push({ plan, kind: "started" });
    }
  }
  return due.sort((a, b) => a.plan.startsAt - b.plan.startsAt);
}

/** Some reminder is still ahead (it keeps the page awake in the tray, lib/page-duties.ts). */
export function reminderAhead(
  plans: readonly CrewPlan[],
  told: Readonly<Record<string, number>>,
  nowMs: number,
): boolean {
  const now = nowMs / 1000;
  return plans.some(
    (plan) => remindable(plan) && !(plan.id in told) && now < plan.startsAt + LATE_TELL,
  );
}

/**
 * How long until anything about plans changes on screen (ms), or null: a plan comes within the
 * hour, rings, starts or ends. While a reminder is ahead the wait is at most `REMINDER_CHECK_MS`.
 */
export function nextPlanTick(
  plans: readonly CrewPlan[],
  nowMs: number,
  remind: boolean,
): number | null {
  const now = nowMs / 1000;
  let next = Number.POSITIVE_INFINITY;
  let waiting = false;
  for (const plan of plans) {
    const moments = [plan.startsAt - PLAN_SOON, plan.startsAt, plan.endsAt];
    if (remind && remindable(plan)) {
      moments.push(ringsAt(plan), plan.startsAt + LATE_TELL);
      if (now < plan.startsAt + LATE_TELL) waiting = true;
    }
    for (const moment of moments) if (moment > now && moment < next) next = moment;
  }
  if (next === Number.POSITIVE_INFINITY) return null;
  const delay = (next - now) * 1000;
  return waiting ? Math.min(delay, REMINDER_CHECK_MS) : delay;
}

/** Told reminders older than this go from the list (ms). */
const TOLD_KEPT_MS = 3 * DAY_MS;

/** The told list without entries of plans long gone (it stays small). */
export function pruneTold(
  told: Readonly<Record<string, number>>,
  nowMs: number,
): Record<string, number> {
  return Object.fromEntries(Object.entries(told).filter(([, at]) => nowMs - at < TOLD_KEPT_MS));
}
