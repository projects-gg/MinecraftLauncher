// "Planlar" on the crew page (TEAM5): up to three times to meet on Projects, soonest first, each
// with who planned it, Studio's event at that time, who is coming, and three answers ("Geliyorum"
// leads, the chosen one is checked). A plan under way says "Başladı" in jade with "Katıl". Those
// who may call the crew make plans with a short form in place: the day (today and the six after),
// the hour and the minute, starting at a sensible evening time, never one gone by. Its maker and
// the crew's leaders delete it ("Planı sil", asked once in place). Nothing moves by itself: a plan
// changes phase on the call clock (one timer, at the moment it changes), answers land in place.
//
// FIX2 (the owner, 22 Oct): a plan carries a short note ("Bedwars, sesli gelin"), written in the
// form or later by its maker (pressing the note, or "Not ekle" / "Notu düzenle" in its menu, while
// it has not started), cleared by its maker or the crew's leaders ("Notu kaldır"); the note is
// checked as chat is, its reason in place. "Rapor et" in the card's menu reports its maker with the
// plan. Who answered opens under the faces (the chevron): coming, maybe, can't come, five each and
// "N kişi daha", every person with the menu they have everywhere. The card glides to its new size
// (a transform layout animation) while what comes fades in and what goes fades out.
import {
  CalendarBlank,
  CalendarPlus,
  CaretDown,
  Check,
  DotsThree,
  Flag,
  LockSimple,
  NotePencil,
  PencilSimple,
  Play,
  Question,
  Trash,
  X,
} from "@phosphor-icons/react";
import {
  Button,
  ContextMenu,
  cn,
  easeOutExpo,
  IconButton,
  Input,
  Menu,
  type MenuItem,
  Segmented,
  Select,
  toast,
} from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type FormEvent, type RefObject, useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce, friendlyError } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { CrewPlan, PlanAnswer, SocialTeam, TeamMember } from "@/lib/api/social-types";
import { attempt } from "./actions";
import { holdFocus } from "./chat/focus";
import { BadgedName } from "./crew-badge";
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
import { leaveOnEscape } from "./edit-escape";
import { JoinButton } from "./friend-row";
import { joinOrAsk, PROJECTS, useThere } from "./join-dialog";
import { limitOf, opensIn } from "./limits";
import { planMenu, rowsOf } from "./menus";
import { enter, glide, leave, pop, rise, swap, useSocialMotion } from "./motion";
import { PersonContextMenu, PersonMoreButton } from "./person-menu";
import {
  cleanPlanNote,
  type NoteProblem,
  PLAN_NOTE_COUNT_FROM,
  PLAN_NOTE_MAX,
  planNoteLength,
  planNoteProblem,
} from "./plan-note";
import { SocialAvatar, SocialFace } from "./social-face";
import { useSocialUi } from "./store";
import { RoleBadge } from "./team-member";
import { useCallClock } from "./use-call-clock";

const ANSWERS: PlanAnswer[] = ["yes", "maybe", "no"];
const pad = (n: number) => String(n).padStart(2, "0");
/** People a group of answers shows before "N kişi daha". */
const GROUP_SHOWN = 5;
/** The card's corners (`--radius-panel`), kept by the layout animation's scale correction. */
const RADIUS = 10;

/** The three answers: "Geliyorum" leads while unanswered; the chosen one is checked. */
function PlanAnswers({
  answered,
  onAnswer,
}: {
  answered: PlanAnswer | undefined;
  onAnswer: (choice: PlanAnswer) => void;
}) {
  const { t } = useTranslation();
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
            onClick={() => onAnswer(choice)}
          >
            {t(`crewWave2.plans.${choice}`)}
          </Button>
        );
      })}
    </fieldset>
  );
}

/** The crewmates who said `ids`, as the crew lists them (answers of people gone are not shown). */
function peopleOf(team: SocialTeam, ids: readonly string[]): TeamMember[] {
  return ids
    .map((id) => team.members.find((m) => m.id === id))
    .filter((m): m is TeamMember => Boolean(m));
}

/**
 * Who is coming: their faces (up to five) and "3 kişi geliyor · 1 belki"; the whole line opens who
 * answered what, its chevron turning.
 */
function Coming({
  plan,
  team,
  open,
  listId,
  onToggle,
}: {
  plan: CrewPlan;
  team: SocialTeam;
  open: boolean;
  listId: string;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const by = answersBy(plan);
  const faces = peopleOf(team, by.yes).filter((m) => !m.blocked);
  const words = [
    by.yes.length ? t("crewWave2.plans.coming", { count: by.yes.length }) : null,
    by.maybe.length ? t("crewWave2.plans.maybeCount", { count: by.maybe.length }) : null,
  ].filter(Boolean);
  const said = words.length ? words.join(" · ") : t("crewWave2.plans.nobody");
  const anyone = plan.answers.length > 0;
  const content = (
    <>
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
          <motion.span key={said} className="block truncate" {...swap(reduce, 0.18)}>
            {said}
          </motion.span>
        </AnimatePresence>
      </span>
    </>
  );
  if (!anyone) {
    return <div className="mt-2.5 flex min-h-[26px] items-center gap-2.5">{content}</div>;
  }
  return (
    <button
      type="button"
      aria-expanded={open}
      aria-controls={listId}
      aria-label={`${said}. ${t("fix2.answers.toggle")}`}
      onClick={onToggle}
      className="group/coming -mx-1.5 mt-2 flex min-h-[30px] w-[calc(100%+12px)] cursor-pointer items-center gap-2.5 rounded-md px-1.5 text-left outline-none transition-colors duration-150 hover:bg-white/[0.04] focus-visible:ring-2 focus-visible:ring-gold-300"
    >
      {content}
      <CaretDown
        size={13}
        weight="bold"
        aria-hidden
        className={cn(
          "shrink-0 text-ash-400 transition-[rotate,color] duration-200 ease-[cubic-bezier(0.16,1,0.3,1)] group-hover/coming:text-bone-100",
          open && "rotate-180",
        )}
      />
    </button>
  );
}

/** One person who answered: face, name, role; their menu on right click and "…". */
function AnswerRow({ member, team }: { member: TeamMember; team: SocialTeam }) {
  const { t } = useTranslation();
  const place = { crew: team, from: "team" as const };
  const row = (
    <div className="group/answer flex h-8 items-center gap-2.5 rounded-md pr-0.5 pl-1 transition-colors duration-150 hover:bg-white/[0.04] has-[[data-state=open]]:bg-white/[0.05]">
      <SocialAvatar
        source={member}
        size={24}
        state={member.me || member.blocked ? undefined : member.state}
        dnd={member.dnd}
      />
      {/* The crew tag they wear before the name (TEAM6). */}
      <BadgedName
        name={member.name}
        badge={member.badge}
        wearer={member.id}
        mark={member.mark}
        size="sm"
        className="flex-1"
        nameClassName={cn(
          "text-[13px]",
          member.me ? "font-semibold text-bone-50" : "text-bone-100",
        )}
      >
        <RoleBadge member={member} size={11} />
      </BadgedName>
      {member.me ? (
        <span className="pr-1.5 text-[12px] text-ash-400">{t("fix2.menu.you")}</span>
      ) : (
        <PersonMoreButton
          person={member}
          place={place}
          className="opacity-0 transition-opacity duration-150 group-hover/answer:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
        />
      )}
    </div>
  );
  return member.me ? (
    row
  ) : (
    <PersonContextMenu person={member} place={place}>
      {row}
    </PersonContextMenu>
  );
}

const GROUP_LOOK: Record<PlanAnswer, { icon: typeof Check; tone: string }> = {
  yes: { icon: Check, tone: "text-jade-400" },
  maybe: { icon: Question, tone: "text-gold-300" },
  no: { icon: X, tone: "text-ash-400" },
};

/** Who answered `answer`: five, then "N kişi daha" (and back). */
function AnswerGroup({
  answer,
  people,
  team,
  all,
  onAll,
}: {
  answer: PlanAnswer;
  people: TeamMember[];
  team: SocialTeam;
  all: boolean;
  onAll: (all: boolean) => void;
}) {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const label = t(`fix2.answers.${answer}`);
  const { icon: Icon, tone } = GROUP_LOOK[answer];
  const shown = all ? people : people.slice(0, GROUP_SHOWN);
  const hidden = people.length - GROUP_SHOWN;
  return (
    <section aria-label={`${label}: ${people.length}`}>
      <h4 className="flex h-6 items-center gap-1.5 px-1 text-[12px] font-medium text-ash-300">
        <Icon size={12} weight="bold" className={tone} aria-hidden />
        {label}
        <span className="font-display text-[12.5px] text-bone-50 tabular-nums">
          {people.length}
        </span>
      </h4>
      <ul className="flex flex-col">
        <AnimatePresence initial={false}>
          {shown.map((member) => (
            <motion.li
              key={member.id}
              layout={layout ? "position" : false}
              transition={{ layout: glide }}
              initial={{ opacity: 0, transform: reduce ? "none" : "translateY(-3px)" }}
              animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.22) }}
              exit={{ opacity: 0, transition: leave(0.14) }}
            >
              <AnswerRow member={member} team={team} />
            </motion.li>
          ))}
        </AnimatePresence>
      </ul>
      {hidden > 0 ? (
        <motion.div layout={layout ? "position" : false} transition={{ layout: glide }}>
          <button
            type="button"
            onClick={() => onAll(!all)}
            className="mt-0.5 ml-1 flex h-7 cursor-pointer items-center gap-1.5 rounded-md px-1.5 text-[12.5px] font-medium text-ash-300 outline-none transition-[color,background-color] duration-150 hover:bg-white/[0.04] hover:text-bone-100 focus-visible:ring-2 focus-visible:ring-gold-300"
          >
            <CaretDown
              size={11}
              weight="bold"
              aria-hidden
              className={cn(
                "transition-[rotate] duration-200 ease-[cubic-bezier(0.16,1,0.3,1)]",
                all && "rotate-180",
              )}
            />
            <span className="relative grid">
              <AnimatePresence initial={false} mode="popLayout">
                <motion.span key={all ? "less" : "more"} {...swap(reduce, 0.16)}>
                  {all ? t("fix2.answers.less") : t("fix2.answers.more", { count: hidden })}
                </motion.span>
              </AnimatePresence>
            </span>
          </button>
        </motion.div>
      ) : null}
    </section>
  );
}

/** Who answered what, in three groups (only the answers the service shows the player). */
function AnswerList({
  plan,
  team,
  id,
  all,
  onAll,
}: {
  plan: CrewPlan;
  team: SocialTeam;
  id: string;
  all: readonly PlanAnswer[];
  onAll: (answer: PlanAnswer, all: boolean) => void;
}) {
  const by = answersBy(plan);
  return (
    <div id={id} className="mt-1.5 flex flex-col gap-1.5 border-white/8 border-t pt-2">
      {ANSWERS.map((answer) => {
        const people = peopleOf(team, by[answer]);
        if (!people.length) return null;
        return (
          <AnswerGroup
            key={answer}
            answer={answer}
            people={people}
            team={team}
            all={all.includes(answer)}
            onAll={(next) => onAll(answer, next)}
          />
        );
      })}
    </div>
  );
}

/** A note's field: one line, the reason in place, the count near the limit. */
function NoteInput({
  value,
  onChange,
  problem,
  error,
  lineId,
  inputRef,
  onEscape,
}: {
  value: string;
  onChange: (value: string) => void;
  problem: NoteProblem | null;
  error: string | null;
  lineId: string;
  inputRef?: RefObject<HTMLInputElement | null>;
  /** Esc in the field (before the panel hears it): leaves the edit. */
  onEscape?: () => void;
}) {
  const { t } = useTranslation();
  const [focused, setFocused] = useState(false);
  const latest = useRef(onEscape);
  latest.current = onEscape;
  useEffect(() => {
    if (!focused || !latest.current) return;
    // Before the panel hears it; an IME's Esc stays the IME's (edit-escape.ts).
    return leaveOnEscape(
      () => true,
      () => latest.current?.(),
    );
  }, [focused]);
  const length = planNoteLength(value);
  const showCount = length >= PLAN_NOTE_COUNT_FROM;
  return (
    <div className="relative">
      <Input
        ref={inputRef}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        placeholder={t("fix2.note.placeholder")}
        aria-label={t("fix2.note.label")}
        aria-describedby={problem || error || showCount ? lineId : undefined}
        invalid={Boolean(problem || error)}
        maxLength={PLAN_NOTE_MAX * 5}
        enterKeyHint="done"
        className={cn("transition-[border-color,box-shadow,padding]", showCount && "pr-[52px]")}
      />
      <span
        aria-hidden={!showCount}
        className={cn(
          "pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-[11.5px] tabular-nums transition-opacity duration-150",
          showCount ? "opacity-100" : "opacity-0",
          length > PLAN_NOTE_MAX ? "text-flare-300" : "text-ash-400",
        )}
      >
        {t("fix2.note.count", { count: length })}
      </span>
    </div>
  );
}

/** The reason a note cannot go, or the service's refusal, in place under the field. */
function NoteLine({ id, text }: { id: string; text: string | null }) {
  const { reduce } = useSocialMotion();
  return (
    <p id={id} className="relative min-h-[20px] px-0.5 pt-1.5 text-[12px] leading-none">
      <AnimatePresence initial={false}>
        {text ? (
          <motion.span
            key={text}
            role="alert"
            className="block text-flare-300"
            initial={{ opacity: 0, transform: reduce ? "none" : "translateY(-2px)" }}
            animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.18) }}
            exit={{ opacity: 0, transition: leave(0.12) }}
          >
            {text}
          </motion.span>
        ) : null}
      </AnimatePresence>
    </p>
  );
}

function problemText(problem: NoteProblem | null, t: (key: string) => string): string | null {
  return problem ? t(`fix2.note.${problem}`) : null;
}

/** The maker changes the note in place: the field (the caret at the end), "Vazgeç" and "Kaydet". */
function NoteEditor({ plan, onDone }: { plan: CrewPlan; onDone: () => void }) {
  const { t } = useTranslation();
  const [value, setValue] = useState(plan.note ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const lineId = useId();
  // Started from a menu: the field keeps the focus the closing menu would take back.
  useEffect(() => (input.current ? holdFocus(input.current) : undefined), []);
  const problem = planNoteProblem(value);
  const changed = cleanPlanNote(value) !== (plan.note ?? "");
  const save = async (event?: FormEvent) => {
    event?.preventDefault();
    if (problem || busy) return;
    if (!changed) {
      onDone();
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const note = cleanPlanNote(value);
      await api.social.setPlanNote(plan.id, note);
      announce(note ? t("fix2.note.saved") : t("fix2.note.cleared"));
      onDone();
    } catch (failure) {
      setError(friendlyError(failure, t("social.actions.failed")).message);
    } finally {
      setBusy(false);
    }
  };
  return (
    <form className="mt-2.5" onSubmit={(event) => void save(event)}>
      <div className="flex items-center gap-1.5">
        <div className="min-w-0 flex-1">
          <NoteInput
            value={value}
            onChange={(next) => {
              setValue(next);
              setError(null);
            }}
            problem={problem}
            error={error}
            lineId={lineId}
            inputRef={input}
            onEscape={onDone}
          />
        </div>
        <Button size="sm" variant="ghost" onClick={onDone}>
          {t("fix2.note.cancel")}
        </Button>
        <Button size="sm" variant="primary" type="submit" disabled={Boolean(problem) || busy}>
          {t("fix2.note.save")}
        </Button>
      </div>
      <NoteLine id={lineId} text={error ?? problemText(problem, t)} />
    </form>
  );
}

/** The note as the crew reads it; its maker presses it to change it (before the plan starts). */
function NoteText({ note, onEdit }: { note: string; onEdit?: () => void }) {
  const { t } = useTranslation();
  const words = (
    <span className="min-w-0 flex-1 text-[13px] leading-snug text-bone-100 break-words">
      <span className="sr-only">{t("fix2.note.label")}: </span>"{note}"
    </span>
  );
  if (!onEdit) {
    return <p className="mt-2 flex items-start gap-2 pl-12">{words}</p>;
  }
  return (
    <button
      type="button"
      onClick={onEdit}
      aria-label={`${t("fix2.note.edit")}: ${note}`}
      className="group/note mt-1.5 -mr-1 ml-10.5 flex w-[calc(100%-42px+4px)] cursor-pointer items-start gap-2 rounded-md py-0.5 pr-1 pl-1.5 text-left outline-none transition-colors duration-150 hover:bg-white/[0.04] focus-visible:ring-2 focus-visible:ring-gold-300"
    >
      {words}
      <PencilSimple
        size={13}
        aria-hidden
        className="mt-0.5 shrink-0 text-ash-400 opacity-0 transition-opacity duration-150 group-hover/note:opacity-100 group-focus-visible/note:opacity-100"
      />
    </button>
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
  const { reduce, layout } = useSocialMotion();
  const now = useCallClock((s) => s.now);
  const notes = useSocialUi((s) => s.view.crew.planNotes);
  const here = useThere(PROJECTS);
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [open, setOpen] = useState(false);
  const [all, setAll] = useState<PlanAnswer[]>([]);
  const [editing, setEditing] = useState(false);
  const [pending, setPending] = useState<PlanAnswer | null>(null);
  const listId = useId();
  const live = planPhase(plan, now) === "live";
  const event = eventDuring(data?.events, plan.at);
  const by = plan.mine
    ? t("crewWave2.plans.byMe")
    : t("crewWave2.plans.by", { name: plan.byName ?? "?" });
  const canWrite = notes && Boolean(plan.canWriteNote) && !live;
  const answered = pending ?? plan.myAnswer;
  const maker = team.members.find((m) => m.id === plan.by);

  // The answer shows at once; the plan's own answer takes over once it says the same.
  useEffect(() => {
    if (pending && plan.myAnswer === pending) setPending(null);
  }, [plan.myAnswer, pending]);
  // A plan that started is not written any more (its note may still be cleared).
  useEffect(() => {
    if (!canWrite) setEditing(false);
  }, [canWrite]);

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

  const cancel = async () => {
    setBusy(true);
    const done = await attempt(() => api.social.cancelPlan(plan.id));
    setBusy(false);
    setAsking(false);
    if (done !== undefined) announce(t("crewWave2.plans.canceled"));
  };

  const clearNote = async () => {
    const done = await attempt(() => api.social.setPlanNote(plan.id, ""));
    if (done !== undefined) announce(t("fix2.note.cleared"));
  };

  // The card's menu ("…" and a right click): the answers, "Katıl", the note, report and delete.
  const items = rowsOf(
    planMenu({
      answer: answered,
      joinable: live && answered !== "no",
      here,
      notes,
      hasNote: Boolean(plan.note),
      canWrite,
      canClear: Boolean(plan.canClearNote),
      report: !plan.mine && Boolean(maker),
      cancel: plan.canCancel,
    }),
    (row): MenuItem | null => {
      switch (row.action) {
        case "yes":
        case "maybe":
        case "no": {
          const choice = row.action;
          return {
            label: t(`crewWave2.plans.${choice}`),
            checked: Boolean(row.checked),
            radio: true,
            onSelect: () => void answer(choice),
          };
        }
        case "join":
          return {
            label: t("social.actions.join"),
            icon: <Play size={16} />,
            disabled: row.disabled,
            onSelect: () => joinOrAsk(PROJECTS),
          };
        case "writeNote":
        case "editNote":
          return {
            label: row.action === "writeNote" ? t("fix2.note.write") : t("fix2.note.edit"),
            icon: <NotePencil size={16} />,
            onSelect: () => setEditing(true),
          };
        case "clearNote":
          return {
            label: t("fix2.note.clear"),
            icon: <X size={16} />,
            onSelect: () => void clearNote(),
          };
        case "report":
          return {
            label: t("reports.action"),
            icon: <Flag size={16} />,
            onSelect: () =>
              useSocialUi.getState().openReport({
                target: { kind: "player", id: plan.by, plan: { crew: plan.crew, id: plan.id } },
                name: maker?.name ?? plan.byName ?? "",
                person: maker,
                plan: { when: planWhen(plan.at, Date.now(), t), note: plan.note },
              }),
          };
        case "cancel":
          return {
            label: t("crewWave2.plans.cancel"),
            icon: <Trash size={16} />,
            danger: true,
            onSelect: () => setAsking(true),
          };
      }
    },
  );

  // The card re-measures itself only when what it holds changes size.
  const shape = [
    open,
    all.join(),
    editing,
    asking,
    plan.note ?? "",
    plan.answers.length,
    live,
  ].join("|");
  const glides = layout ? "position" : false;

  return (
    <ContextMenu items={items}>
      <motion.div
        layout={layout}
        layoutDependency={shape}
        transition={{ layout: glide }}
        style={{ borderRadius: RADIUS }}
        className={cn(
          "relative overflow-hidden p-3 ring-1 transition-[box-shadow,background-color] duration-300 ease-out",
          live ? "bg-jade-400/[0.06] ring-jade-400/25" : "bg-obsidian-950/40 ring-white/8",
          marked && "ring-gold-400/60",
        )}
      >
        <motion.div
          layout={glides}
          layoutDependency={shape}
          transition={{ layout: glide }}
          className="flex items-start gap-3"
        >
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
          {live && answered !== "no" ? <JoinButton /> : null}
          <Menu
            items={items}
            trigger={
              <IconButton
                size="sm"
                label={t("social.actions.more")}
                icon={<DotsThree size={18} weight="bold" />}
                className="-mt-1 -mr-1"
              />
            }
          />
        </motion.div>
        <AnimatePresence mode="popLayout" initial={false}>
          {editing ? (
            <motion.div
              key="editor"
              layout={glides}
              transition={{ layout: glide }}
              {...rise(reduce, 3, 0.2)}
            >
              <NoteEditor plan={plan} onDone={() => setEditing(false)} />
            </motion.div>
          ) : plan.note ? (
            <motion.div
              key={`note:${plan.note}`}
              layout={glides}
              transition={{ layout: glide }}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: { ...enter(0.22), delay: 0.04 } }}
              exit={{ opacity: 0, transition: leave(0.12) }}
            >
              <NoteText note={plan.note} onEdit={canWrite ? () => setEditing(true) : undefined} />
            </motion.div>
          ) : null}
        </AnimatePresence>
        <motion.div layout={glides} layoutDependency={shape} transition={{ layout: glide }}>
          <Coming
            plan={plan}
            team={team}
            open={open && plan.answers.length > 0}
            listId={listId}
            onToggle={() => setOpen((was) => !was)}
          />
        </motion.div>
        <AnimatePresence mode="popLayout" initial={false}>
          {open && plan.answers.length > 0 ? (
            <motion.div
              key="answers"
              layout={glides}
              transition={{ layout: glide }}
              initial={{ opacity: 0, transform: reduce ? "none" : "translateY(-4px)" }}
              animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.24) }}
              exit={{ opacity: 0, transition: { duration: 0.12, ease: easeOutExpo } }}
            >
              <AnswerList
                plan={plan}
                team={team}
                id={listId}
                all={all}
                onAll={(group, show) =>
                  setAll((was) => (show ? [...was, group] : was.filter((g) => g !== group)))
                }
              />
            </motion.div>
          ) : null}
        </AnimatePresence>
        <AnimatePresence mode="popLayout" initial={false}>
          {asking ? (
            <motion.div
              key="ask"
              className="mt-3"
              layout={glides}
              transition={{ layout: glide }}
              {...rise(reduce, 4, 0.2)}
            >
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
            <motion.div
              key="answers-buttons"
              layout={glides}
              transition={{ layout: glide }}
              {...rise(reduce, 4, 0.2)}
            >
              <PlanAnswers answered={answered} onAnswer={(choice) => void answer(choice)} />
            </motion.div>
          )}
        </AnimatePresence>
      </motion.div>
    </ContextMenu>
  );
}

/** The plan form: the day, the hour and the minute, its note; "Planla". */
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
  const notes = useSocialUi((s) => s.view.crew.planNotes);
  // The form's own clock: the days and times offered when it opened (minutes do not tick here).
  const [opened] = useState(() => Date.now());
  const days = useMemo(() => planDays(opened), [opened]);
  const [at, setAt] = useState(() => defaultPlanTime(opened));
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [taken, setTaken] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const lineId = useId();
  const day = dayOf(at, days) ?? days[0];
  const times = useMemo(() => (day ? planTimes(day, opened) : []), [day, opened]);
  const { hour, minute } = clockOf(at);
  const hours = [...new Set(times.map((time) => clockOf(time).hour))];
  const minutes = times
    .filter((time) => clockOf(time).hour === hour)
    .map((time) => clockOf(time).minute);
  const problem = notes ? planNoteProblem(note) : null;

  const move = (next: number | null) => {
    if (next === null) return;
    setAt(next);
    setTaken(false);
    setError(null);
  };
  const submit = async () => {
    if (problem || busy) return;
    const existing = plans.find((plan) => plan.at === at);
    if (existing) {
      setTaken(true);
      onTaken(existing.id);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const written = notes ? cleanPlanNote(note) : "";
      await api.social.makePlan(team.id, at, written || undefined);
      announce(t("crewWave2.plans.made", { when: planWhen(at, Date.now(), t) }));
      onDone();
    } catch (failure) {
      // Said in place, under the form (a note turned away, today's plans used up).
      setError(friendlyError(failure, t("social.actions.failed")).message);
    } finally {
      setBusy(false);
    }
  };

  if (!day) return null;
  const line = error ?? problemText(problem, t) ?? (taken ? t("crewWave2.plans.exists") : null);
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
        <Button type="submit" size="sm" variant="primary" disabled={busy || Boolean(problem)}>
          {t("crewWave2.plans.form.submit")}
        </Button>
      </div>
      {notes ? (
        <div className="mt-2.5">
          <NoteInput
            value={note}
            onChange={(next) => {
              setNote(next);
              setError(null);
            }}
            problem={problem}
            error={error}
            lineId={lineId}
          />
        </div>
      ) : null}
      {notes ? (
        <NoteLine id={lineId} text={line} />
      ) : (
        <AnimatePresence initial={false}>
          {line ? <TakenLine key={line} text={line} /> : null}
        </AnimatePresence>
      )}
    </form>
  );
}

/** Without notes (an older service): the form's one line as it was. */
function TakenLine({ text }: { text: string }) {
  const { reduce } = useSocialMotion();
  return (
    <motion.p role="alert" className="mt-2.5 text-[12.5px] text-gold-200" {...rise(reduce, 4, 0.2)}>
      {text}
    </motion.p>
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
      {/* "Plan yap" ends where the members' "Katıl" plates do, in the same plate (LANDING-L: the two
          stood out of line): their rows keep 8 px on the right and, after an 8 px gap, the place of
          their "⋯" (SMALLS-L), so this row keeps both too. */}
      <div className="mb-2.5 flex min-h-8 items-center gap-2 pr-2">
        <h3 className="min-w-0 flex-1 text-[13px] font-semibold text-ash-300">
          {t("crewWave2.plans.title")}
        </h3>
        {may && !forming && !full && !limit ? (
          <Button
            size="sm"
            variant="secondary"
            icon={<CalendarPlus size={15} />}
            onClick={() => setForming(true)}
          >
            {t("crewWave2.plans.make")}
          </Button>
        ) : null}
        <span aria-hidden className="size-8 shrink-0" />
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
          {/* Empty, the title alone read as a list that failed to load (SMALL12). */}
          {!plans.length && !forming ? (
            <motion.li
              key="none"
              className="text-[13px] text-ash-400"
              layout={layout ? "position" : false}
              transition={{ layout: glide }}
              {...rise(reduce, 0, 0.2)}
            >
              {t("crewWave2.plans.none")}
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
