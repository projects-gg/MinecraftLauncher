import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import type { CrewPlan, TeamPermissions } from "@/lib/api/social-types";
import {
  answersBy,
  canPlan,
  dayOf,
  defaultPlanTime,
  duePlans,
  eventDuring,
  homePlan,
  LATE_TELL,
  nextPlanTick,
  PLAN_SOON,
  type PlanDay,
  planDays,
  planPhase,
  plansAhead,
  plansOf,
  planTimes,
  planWhen,
  pruneTold,
  REMINDER_CHECK_MS,
  REMINDER_LEAD,
  reminderAhead,
  timeOn,
} from "./crew-plans";

/** Local wall time (the tests read the same in any time zone). */
const local = (day: number, hour: number, minute = 0) =>
  new Date(2026, 9, day, hour, minute).getTime();
const sec = (ms: number) => ms / 1000;

const t = ((key: string, values?: Record<string, unknown>) =>
  values ? `${key} ${JSON.stringify(values)}` : key) as unknown as TFunction;

/** The `index`th day offered (the tests fail plainly when there is none). */
function nth(days: PlanDay[], index: number): PlanDay {
  const day = days[index];
  if (!day) throw new Error(`no day ${index}`);
  return day;
}

function plan(at: number, extra: Partial<CrewPlan> = {}): CrewPlan {
  return {
    id: `p${at}`,
    crew: "c1",
    by: "u1",
    byName: "Ayşe",
    mine: false,
    canCancel: false,
    at,
    startsAt: at,
    endsAt: at + 7200,
    createdAt: at - 3600,
    answers: [],
    ...extra,
  };
}

describe("the plan form", () => {
  it("offers whole quarters at least five minutes ahead, today and six more days", () => {
    // Wednesday 21 October 2026, 20:52.
    const now = local(21, 20, 52);
    const days = planDays(now);
    expect(days).toHaveLength(7);
    expect(days[0]?.offset).toBe(0);
    expect(days[6]?.offset).toBe(6);
    const today = planTimes(nth(days, 0), now);
    expect(today[0]).toBe(sec(local(21, 21)));
    expect(today.at(-1)).toBe(sec(local(21, 23, 45)));
    expect(today.every((at) => at % 900 === 0)).toBe(true);
    expect(planTimes(nth(days, 1), now)).toHaveLength(96);
  });

  it("starts on the next days once today is over, still seven of them", () => {
    const now = local(21, 23, 56);
    const days = planDays(now);
    expect(days[0]?.offset).toBe(1);
    expect(days).toHaveLength(7);
    // The seventh day ends where the service's seven days do.
    const last = planTimes(nth(days, 6), now);
    expect(last.at(-1)).toBeLessThanOrEqual(sec(now) + 7 * 86_400 - 300);
  });

  it("starts at tonight's nine, else the next whole hour, else tomorrow's nine", () => {
    expect(defaultPlanTime(local(21, 15, 10))).toBe(sec(local(21, 21)));
    expect(defaultPlanTime(local(21, 20, 40))).toBe(sec(local(21, 22)));
    expect(defaultPlanTime(local(21, 21, 10))).toBe(sec(local(21, 22)));
    expect(defaultPlanTime(local(21, 22, 20))).toBe(sec(local(21, 23)));
    expect(defaultPlanTime(local(21, 22, 40))).toBe(sec(local(22, 21)));
  });

  it("keeps the clock time when the day changes, as close as the day allows", () => {
    const now = local(21, 20, 52);
    const days = planDays(now);
    const tomorrow = nth(days, 1);
    expect(timeOn(tomorrow, 18, 30, now)).toBe(sec(local(22, 18, 30)));
    // Today 19:00 is gone: the first time left.
    expect(timeOn(nth(days, 0), 19, 0, now)).toBe(sec(local(21, 21)));
    expect(dayOf(sec(local(22, 18, 30)), days)?.offset).toBe(1);
  });
});

describe("a plan on screen", () => {
  const at = sec(local(21, 21));
  const one = plan(at);

  it("is ahead, soon, under way, then over", () => {
    expect(planPhase(one, (at - PLAN_SOON - 1) * 1000)).toBe("upcoming");
    expect(planPhase(one, (at - PLAN_SOON) * 1000)).toBe("soon");
    expect(planPhase(one, at * 1000)).toBe("live");
    expect(planPhase(one, (at + 7200) * 1000)).toBe("over");
  });

  it("shows the crew's plans soonest first, three ahead at most counted", () => {
    const now = (at - 60) * 1000;
    const plans = [plan(at + 3600), plan(at), plan(at - 7300), plan(at, { id: "x", crew: "c2" })];
    expect(plansOf(plans, "c1", now).map((p) => p.at)).toEqual([at, at + 3600]);
    expect(plansAhead(plans, "c1", now)).toBe(2);
  });

  it("may be made by those who may call the crew", () => {
    const permissions: TeamPermissions = {
      invite: "members",
      call: "moderators",
      kick: "moderators",
      edit: "leader",
      admit: "moderators",
    };
    expect(canPlan({ role: "member", moderator: false, permissions }, { plans: true })).toBe(false);
    expect(canPlan({ role: "member", moderator: true, permissions }, { plans: true })).toBe(true);
    expect(canPlan({ role: "founder", moderator: false, permissions }, { plans: false })).toBe(
      false,
    );
  });

  it("says who comes and when", () => {
    const answered = plan(at, {
      answers: [
        { id: "a", answer: "yes" },
        { id: "b", answer: "maybe" },
        { id: "c", answer: "yes" },
      ],
    });
    expect(answersBy(answered)).toEqual({ yes: ["a", "c"], maybe: ["b"], no: [] });
    expect(planWhen(at, local(21, 12), t)).toMatch(/^crewWave2\.plans\.today /);
    expect(planWhen(at, local(20, 12), t)).toMatch(/^crewWave2\.plans\.tomorrow /);
    expect(planWhen(at, local(18, 12), t)).toMatch(/^crewWave2\.plans\.onDay /);
  });

  it("names Studio's event at its time", () => {
    const events = [
      {
        id: "e1",
        title: "Bedwars",
        startsAt: new Date(local(21, 20)).toISOString(),
        endsAt: new Date(local(21, 22)).toISOString(),
      },
    ];
    expect(eventDuring(events, at)?.title).toBe("Bedwars");
    expect(eventDuring(events, sec(local(21, 22)))).toBeUndefined();
    expect(eventDuring(undefined, at)).toBeUndefined();
  });
});

describe("Home's plan line", () => {
  const at = sec(local(21, 21));

  it("tells of one under way first, else one within the hour, never a no", () => {
    const later = plan(at + 1800, { id: "later", myAnswer: "yes" });
    const live = plan(at - 600, { id: "live" });
    const no = plan(at, { id: "no", myAnswer: "no" });
    const now = at * 1000;
    expect(homePlan([later, live, no], now)?.plan.id).toBe("live");
    expect(homePlan([later, live, no], now)?.phase).toBe("live");
    expect(homePlan([later, no], now)?.plan.id).toBe("later");
    expect(homePlan([later, no], now, ["later"])).toBeNull();
    expect(homePlan([plan(at + PLAN_SOON + 60)], now)).toBeNull();
  });
});

describe("plan reminders", () => {
  const at = sec(local(21, 21));
  const yes = plan(at, { id: "yes", myAnswer: "yes" });
  const maybe = plan(at + 900, { id: "maybe", myAnswer: "maybe" });
  const none = plan(at, { id: "none" });

  it("ring ten minutes before for yes and maybe, once", () => {
    const ring = (at - REMINDER_LEAD) * 1000;
    expect(duePlans([yes, none], {}, ring - 1000)).toEqual([]);
    expect(duePlans([yes, none], {}, ring)).toEqual([{ plan: yes, kind: "soon", minutes: 10 }]);
    expect(duePlans([yes], { yes: ring }, ring + 60_000)).toEqual([]);
    expect(
      duePlans([yes, maybe], {}, (at + 900 - REMINDER_LEAD) * 1000).map((d) => d.plan.id),
    ).toEqual(["yes", "maybe"]);
  });

  it("say the plan started when the moment was missed, for a quarter of an hour", () => {
    expect(duePlans([yes], {}, (at + 60) * 1000)).toEqual([{ plan: yes, kind: "started" }]);
    expect(duePlans([yes], {}, (at + LATE_TELL) * 1000)).toEqual([]);
    expect(reminderAhead([yes], {}, (at + 60) * 1000)).toBe(true);
    expect(reminderAhead([yes], { yes: 1 }, (at - 3600) * 1000)).toBe(false);
    expect(reminderAhead([none], {}, (at - 3600) * 1000)).toBe(false);
  });

  it("wake the clock once per change, at most a quarter hour apart while one waits", () => {
    // Two hours before: the next moment is "within the hour", but a reminder waits.
    const early = (at - 7200) * 1000;
    expect(nextPlanTick([yes], early, true)).toBe(REMINDER_CHECK_MS);
    expect(nextPlanTick([yes], early, false)).toBe((7200 - PLAN_SOON) * 1000);
    expect(nextPlanTick([none], (at - 300) * 1000, true)).toBe(300 * 1000);
    expect(nextPlanTick([], early, true)).toBeNull();
    // Over: nothing more.
    expect(nextPlanTick([none], (at + 7200) * 1000, true)).toBeNull();
  });

  it("forget told reminders after a few days", () => {
    const now = local(21, 12);
    expect(pruneTold({ a: now - 1000, b: now - 4 * 86_400_000 }, now)).toEqual({ a: now - 1000 });
  });
});
