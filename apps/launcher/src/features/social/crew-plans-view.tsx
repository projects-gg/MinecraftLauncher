// "Planlar" on the crew page (TEAM5): up to three times to meet on Projects, soonest first, each
// with who planned it, Studio's event at that time, who is coming, and three answers ("Geliyorum"
// leads, the chosen one is checked). A plan under way says "Başladı" in jade with "Katıl". Those
// who may call the crew make plans with a short form in place: the day (today and the six after),
// the hour and the minute, starting at a sensible evening time, never one gone by. Its maker and
// the crew's leaders delete it ("Planı sil", asked once in place). Nothing moves by itself: a plan
// changes phase on the call clock (one timer, at the moment it changes), answers land in place.
import {
  CalendarBlank,
  CalendarPlus,
  Check,
  DotsThree,
  LockSimple,
  Trash,
} from "@phosphor-icons/react";
import { Button, cn, IconButton, Menu, Segmented, Select, toast } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { CrewPlan, PlanAnswer, SocialTeam, TeamMember } from "@/lib/api/social-types";
import { attempt } from "./actions";
import {
  answersBy,
  canPlan,
  clockOf,
  dayChip,
  dayOf,
  defaultPlanTime,
  eventDuring,
  MAX_PLANS,
  planDays,
  planPhase,
  plansAhead,
  plansOf,
  planTimes,
  planWhen,
  timeOn,
} from "./crew-plans";
import { JoinButton } from "./friend-row";
import { limitOf, opensIn } from "./limits";
import { glide, pop, rise, swap, useSocialMotion } from "./motion";
import { SocialFace } from "./social-face";
import { useSocialUi } from "./store";
import { useCallClock } from "./use-call-clock";

const ANSWERS: PlanAnswer[] = ["yes", "maybe", "no"];
const pad = (n: number) => String(n).padStart(2, "0");

/** The three answers: "Geliyorum" leads while unanswered; the chosen one is checked. */
function PlanAnswers({ plan }: { plan: CrewPlan }) {
  const { t } = useTranslation();
  const [pending, setPending] = useState<PlanAnswer | null>(null);
  const answered = pending ?? plan.myAnswer;
  // The answer shows at once; the plan's own answer takes over once it says the same.
  useEffect(() => {
    if (pending && plan.myAnswer === pending) setPending(null);
  }, [plan.myAnswer, pending]);
  const answer = async (choice: PlanAnswer) => {
    if (choice === answered) return;
    setPending(choice);
    const still = await attempt(() => api.social.answerPlan(plan.id, choice));
    if (still === undefined) {
      setPending(null);
      return;
    }
    if (still === false) {
      setPending(null);
      toast(t("crewWave2.plans.gone"));
    }
  };
  const variant = (choice: PlanAnswer): "primary" | "secondary" | "ghost" => {
    if (answered) return answered === choice ? "secondary" : "ghost";
    return choice === "yes" ? "primary" : choice === "maybe" ? "secondary" : "ghost";
  };
  return (
    <fieldset className="m-0 mt-3 flex min-w-0 gap-1.5 border-0 p-0">
      <legend className="sr-only">{t("crewWave2.plans.answer")}</legend>
      {ANSWERS.map((choice) => {
        const chosen = answered === choice;
        return (
          <Button
            key={choice}
            size="sm"
            aria-pressed={chosen}
            variant={variant(choice)}
            className={cn(
              "flex-1 transition-[box-shadow,background-color,color]",
              chosen && "ring-1 ring-gold-400/70",
            )}
            icon={chosen ? <Check size={14} weight="bold" className="text-gold-300" /> : undefined}
            onClick={() => void answer(choice)}
          >
            {t(`crewWave2.plans.${choice}`)}
          </Button>
        );
      })}
    </fieldset>
  );
}

/** Who is coming: their faces (up to five) and "3 kişi geliyor · 1 belki". */
function Coming({ plan, team }: { plan: CrewPlan; team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const by = answersBy(plan);
  const faces = by.yes
    .map((id) => team.members.find((m) => m.id === id))
    .filter((m): m is TeamMember => Boolean(m) && !m?.blocked);
  const words = [
    by.yes.length ? t("crewWave2.plans.coming", { count: by.yes.length }) : null,
    by.maybe.length ? t("crewWave2.plans.maybeCount", { count: by.maybe.length }) : null,
  ].filter(Boolean);
  return (
    <div className="mt-2.5 flex min-h-[22px] items-center gap-2.5">
      {faces.length ? (
        <span className="flex items-center">
          <AnimatePresence initial={false} mode="popLayout">
            {faces.slice(0, 5).map((member, index) => (
              <motion.span
                key={member.id}
                className="relative flex rounded-[4px] ring-2 ring-obsidian-900"
                style={{ marginLeft: index ? -6 : 0, zIndex: 5 - index }}
                {...pop(reduce, 0.16)}
              >
                <SocialFace source={member} size={20} />
              </motion.span>
            ))}
          </AnimatePresence>
        </span>
      ) : null}
      <span className="relative min-w-0 flex-1 truncate text-[12.5px] text-ash-300">
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span key={words.join("|")} className="block truncate" {...swap(reduce, 0.18)}>
            {words.length ? words.join(" · ") : t("crewWave2.plans.nobody")}
          </motion.span>
        </AnimatePresence>
      </span>
    </div>
  );
}

function PlanCard({
  plan,
  team,
  marked,
}: {
  plan: CrewPlan;
  team: SocialTeam;
  /** The form found this plan at the time it was asked for: drawn gold for a moment. */
  marked: boolean;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const now = useCallClock((s) => s.now);
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const live = planPhase(plan, now) === "live";
  const event = eventDuring(data?.events, plan.at);
  const by = plan.mine
    ? t("crewWave2.plans.byMe")
    : t("crewWave2.plans.by", { name: plan.byName ?? "?" });

  const cancel = async () => {
    setBusy(true);
    const done = await attempt(() => api.social.cancelPlan(plan.id));
    setBusy(false);
    setAsking(false);
    if (done !== undefined) announce(t("crewWave2.plans.canceled"));
  };

  return (
    <div
      className={cn(
        "rounded-[var(--radius-panel)] p-3 ring-1 transition-[box-shadow,background-color] duration-300 ease-out",
        live ? "bg-jade-400/[0.06] ring-jade-400/25" : "bg-obsidian-950/40 ring-white/8",
        marked && "ring-gold-400/60",
      )}
    >
      <div className="flex items-start gap-3">
        <span
          className={cn(
            "flex size-9 shrink-0 items-center justify-center rounded-[var(--radius-head)] ring-1",
            live
              ? "bg-jade-400/12 text-jade-400 ring-jade-400/25"
              : "bg-gold-500/10 text-gold-300 ring-gold-400/20",
          )}
        >
          <CalendarBlank size={18} weight="duotone" aria-hidden />
        </span>
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-2">
            <span className="truncate font-display text-[16px] leading-tight font-semibold text-bone-50 tabular-nums">
              {planWhen(plan.at, now, t)}
            </span>
            <AnimatePresence initial={false}>
              {live ? (
                <motion.span
                  key="live"
                  className="chamfer inline-flex h-[20px] shrink-0 items-center bg-jade-400/15 px-[7px] text-[11.5px] font-semibold text-jade-400 [--cut:5px]"
                  {...pop(reduce, 0.2)}
                >
                  {t("crewWave2.plans.live")}
                </motion.span>
              ) : null}
            </AnimatePresence>
          </span>
          <span className="mt-0.5 block truncate text-[12.5px] text-ash-400">
            {event ? `${by} · ${t("crewWave2.plans.event", { title: event.title })}` : by}
          </span>
        </span>
        {live && plan.myAnswer !== "no" ? <JoinButton /> : null}
        {plan.canCancel ? (
          <Menu
            items={[
              {
                label: t("crewWave2.plans.cancel"),
                icon: <Trash size={16} />,
                danger: true,
                onSelect: () => setAsking(true),
              },
            ]}
            trigger={
              <IconButton
                size="sm"
                label={t("social.actions.more")}
                icon={<DotsThree size={18} weight="bold" />}
                className="-mt-1 -mr-1"
              />
            }
          />
        ) : null}
      </div>
      <Coming plan={plan} team={team} />
      <AnimatePresence mode="popLayout" initial={false}>
        {asking ? (
          <motion.div key="ask" className="mt-3" {...rise(reduce, 4, 0.2)}>
            <p className="text-[13px] leading-snug text-bone-100">
              {t("crewWave2.plans.cancelAsk")}
            </p>
            <div className="mt-2 flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setAsking(false)}>
                {t("teams.cancel")}
              </Button>
              <Button size="sm" variant="danger" disabled={busy} onClick={() => void cancel()}>
                {t("crewWave2.plans.cancel")}
              </Button>
            </div>
          </motion.div>
        ) : (
          <motion.div key="answers" {...rise(reduce, 4, 0.2)}>
            <PlanAnswers plan={plan} />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

/** The plan form: the day, the hour and the minute; "Planla". */
function PlanForm({
  team,
  plans,
  onDone,
  onTaken,
}: {
  team: SocialTeam;
  plans: readonly CrewPlan[];
  onDone: () => void;
  /** A plan is already at that time: the card says so. */
  onTaken: (planId: string) => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  // The form's own clock: the days and times offered when it opened (minutes do not tick here).
  const [opened] = useState(() => Date.now());
  const days = useMemo(() => planDays(opened), [opened]);
  const [at, setAt] = useState(() => defaultPlanTime(opened));
  const [busy, setBusy] = useState(false);
  const [taken, setTaken] = useState(false);
  const day = dayOf(at, days) ?? days[0];
  const times = useMemo(() => (day ? planTimes(day, opened) : []), [day, opened]);
  const { hour, minute } = clockOf(at);
  const hours = [...new Set(times.map((time) => clockOf(time).hour))];
  const minutes = times
    .filter((time) => clockOf(time).hour === hour)
    .map((time) => clockOf(time).minute);

  const move = (next: number | null) => {
    if (next === null) return;
    setAt(next);
    setTaken(false);
  };
  const submit = async () => {
    const existing = plans.find((plan) => plan.at === at);
    if (existing) {
      setTaken(true);
      onTaken(existing.id);
      return;
    }
    setBusy(true);
    const made = await attempt(() => api.social.makePlan(team.id, at));
    setBusy(false);
    if (made === undefined) return;
    announce(t("crewWave2.plans.made", { when: planWhen(at, Date.now(), t) }));
    onDone();
  };

  if (!day) return null;
  return (
    <form
      className="rounded-[var(--radius-panel)] bg-obsidian-950/40 p-3 ring-1 ring-white/8"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <Segmented<string>
        value={day.key}
        onValueChange={(key) => {
          const next = days.find((one) => one.key === key);
          if (next) move(timeOn(next, hour, minute, opened));
        }}
        label={t("crewWave2.plans.form.day")}
        className="w-full"
        options={days.map((one) => ({ value: one.key, label: dayChip(one, t) }))}
      />
      <div className="mt-3 flex items-center gap-2">
        <span className="text-[13px] text-ash-300">{t("crewWave2.plans.form.hour")}</span>
        <Select<string>
          value={pad(hour)}
          onValueChange={(value) => move(timeOn(day, Number(value), minute, opened))}
          label={t("crewWave2.plans.form.hour")}
          className="w-[74px]"
          options={hours.map((h) => ({ value: pad(h), label: pad(h) }))}
        />
        <span aria-hidden className="font-display text-[15px] text-ash-400">
          :
        </span>
        <Select<string>
          value={pad(minute)}
          onValueChange={(value) => move(timeOn(day, hour, Number(value), opened))}
          label={t("crewWave2.plans.form.minute")}
          className="w-[74px]"
          options={minutes.map((m) => ({ value: pad(m), label: pad(m) }))}
        />
        <span className="flex-1" />
        <Button size="sm" variant="ghost" onClick={onDone}>
          {t("teams.cancel")}
        </Button>
        <Button type="submit" size="sm" variant="primary" disabled={busy}>
          {t("crewWave2.plans.form.submit")}
        </Button>
      </div>
      <AnimatePresence initial={false}>
        {taken ? (
          <motion.p
            key="taken"
            role="alert"
            className="mt-2.5 text-[12.5px] text-gold-200"
            {...rise(reduce, 4, 0.2)}
          >
            {t("crewWave2.plans.exists")}
          </motion.p>
        ) : null}
      </AnimatePresence>
    </form>
  );
}

/**
 * The crew's plans and "Plan yap" (those who may call the crew). Nothing at all while there are no
 * plans and the player may not make one, or on a service without plans.
 */
export function CrewPlans({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const features = useSocialUi((s) => s.view.crew);
  const me = useSocialUi((s) => s.view.me);
  const all = useSocialUi((s) => s.view.plans);
  const now = useCallClock((s) => s.now);
  const [forming, setForming] = useState(false);
  const [marked, setMarked] = useState<string | null>(null);
  const plans = plansOf(all, team.id, now);
  const may = canPlan(team, features);
  const full = plansAhead(all, team.id, now) >= MAX_PLANS;
  const limit = limitOf(me, "crewPlans", Date.now());
  // A plan the form ran into is drawn gold for a moment.
  useEffect(() => {
    if (!marked) return;
    const id = window.setTimeout(() => setMarked(null), 1600);
    return () => window.clearTimeout(id);
  }, [marked]);
  useEffect(() => {
    if (full || limit || !may) setForming(false);
  }, [full, limit, may]);
  if (!features.plans || (!plans.length && !may)) return null;

  return (
    <section className="mt-5">
      <div className="mb-2.5 flex min-h-8 items-center gap-2">
        <h3 className="min-w-0 flex-1 text-[12.5px] font-medium text-ash-400">
          {t("crewWave2.plans.title")}
        </h3>
        {may && !forming && !full && !limit ? (
          <Button
            size="sm"
            variant="ghost"
            icon={<CalendarPlus size={15} />}
            onClick={() => setForming(true)}
          >
            {t("crewWave2.plans.make")}
          </Button>
        ) : null}
      </div>
      {may && limit ? (
        <p className="mb-2 flex items-start gap-2.5 px-1 text-[13px] leading-snug text-ash-300">
          <LockSimple size={16} className="mt-px shrink-0 text-ash-400" aria-hidden />
          {t("crewWave2.plans.limited", { when: opensIn(t, limit.until, Date.now()) })}
        </p>
      ) : null}
      <ul className="flex flex-col gap-2">
        <AnimatePresence initial={false} mode="popLayout">
          {forming ? (
            <motion.li
              key="form"
              layout={layout ? "position" : false}
              transition={{ layout: glide }}
              {...rise(reduce, 6, 0.24)}
            >
              <PlanForm
                team={team}
                plans={plans}
                onDone={() => setForming(false)}
                onTaken={setMarked}
              />
            </motion.li>
          ) : null}
          {plans.map((plan) => (
            <motion.li
              key={plan.id}
              layout={layout ? "position" : false}
              transition={{ layout: glide }}
              {...rise(reduce, 6, 0.26)}
            >
              <PlanCard plan={plan} team={team} marked={marked === plan.id} />
            </motion.li>
          ))}
        </AnimatePresence>
      </ul>
      {may && full && !forming ? (
        <p className="mt-2 px-1 text-[12.5px] text-ash-400">{t("crewWave2.plans.full")}</p>
      ) : null}
    </section>
  );
}
