// "Ekibi çağır" on screen (TEAM2 §4.3, CALL1, TEAM4). The crew card's strip turns into the call: the
// caller sees "Çağrın sürüyor", where (Projects or the server's address), the minutes left and
// "Bitir"; a teammate sees who calls, where, and three answers, "Geliyorum" in gold when it can
// join the caller there. Under both, three boxes show who is coming, who comes soon and who
// cannot. The same call sits under its crew's header in the friend list, on Home and in a toast
// when it arrives; with several crews those name the crews it calls. Nothing moves by itself: the
// minutes change once a minute, an answer fades in where it lands. A call from before CALL1 (an
// older service) still shows as "Hazır mısın?".
import { Check, Clock, MegaphoneSimple, Play, UsersThree, X } from "@phosphor-icons/react";
import { Button, ContextMenu, cn, IconButton, type MenuItem, toast } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { CallAnswer, CrewCall, SocialTeam, TeamMember } from "@/lib/api/social-types";
import { answerCall, endCall } from "./call-actions";
import { answersBy, crewsOfCall, minutesLeft } from "./calls";
import { type CallWhere, callerIn, callSentenceOf, callWhere, callWhereLine } from "./crews";
import { joinCall, PROJECTS, useThere } from "./join-dialog";
import { callMenu, rowsOf } from "./menus";
import { pop, swap, useSocialMotion } from "./motion";
import { usePeopleItems } from "./person-menu";
import { type JoinTarget, joinTarget } from "./play-together";
import { SocialAvatar, SocialFace } from "./social-face";
import { useSocialUi } from "./store";
import { CrewMark } from "./team-emblem";
import { useCallClock } from "./use-call-clock";

const ANSWERS: CallAnswer[] = ["yes", "soon", "no"];

/** Where "Katıl" on a call leads (its "Katıl" is off once the player's game is there). */
function callTarget(where: CallWhere): JoinTarget {
  return where.kind === "server"
    ? (joinTarget({ where: "other", host: where.host }) ?? PROJECTS)
    : PROJECTS;
}

/** "Projects'te" / "mc.example.com sunucusunda" (jade: "Geliyorum" goes there), or "Hazır
 * mısın?" for a call from before CALL1. */
function WhereLine({ call, className }: { call: CrewCall; className?: string }) {
  const { t } = useTranslation();
  const where = callWhere(call);
  return (
    <span
      className={cn(
        "block truncate text-[12.5px]",
        where.kind === "ready" ? "text-ash-300" : "text-jade-400",
        className,
      )}
    >
      {callWhereLine(where, t)}
    </span>
  );
}

/** "17 dk", read as "17 dakika kaldı". Changes once a minute (the call clock), never by seconds. */
export function MinutesLeft({ call, className }: { call: CrewCall; className?: string }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const now = useCallClock((s) => s.now);
  const minutes = minutesLeft(call, now);
  return (
    <span className={cn("relative inline-flex font-display text-[13px] tabular-nums", className)}>
      <span className="sr-only">{t("teams.call.leftLong", { count: minutes })}</span>
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span key={minutes} aria-hidden {...swap(reduce, 0.18)}>
          {t("teams.call.left", { count: minutes })}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

/** The three answers: "Geliyorum" leads (gold when it joins the caller), the chosen one is checked. */
export function AnswerButtons({
  call,
  size = "sm",
  className,
}: {
  call: CrewCall;
  size?: "sm" | "md";
  className?: string;
}) {
  const { t } = useTranslation();
  const answered = call.myAnswer;
  const joinable = callWhere(call).kind !== "ready";
  /** Unanswered: "Geliyorum" leads (gold when it joins the caller), "Gelemem" stays quiet.
   * Answered: the chosen one is checked and gold-edged, the others step back. */
  const variant = (answer: CallAnswer): "primary" | "secondary" | "ghost" => {
    if (answered) return answered === answer ? "secondary" : "ghost";
    if (answer === "yes") return joinable ? "primary" : "secondary";
    return answer === "soon" ? "secondary" : "ghost";
  };
  return (
    <fieldset className={cn("m-0 flex min-w-0 gap-1.5 border-0 p-0", className)}>
      <legend className="sr-only">{t("teams.call.answer")}</legend>
      {ANSWERS.map((answer) => {
        const chosen = answered === answer;
        const joins = answer === "yes" && joinable && !answered;
        return (
          <Button
            key={answer}
            size={size}
            aria-pressed={chosen}
            variant={variant(answer)}
            className={cn(
              "flex-1 transition-[box-shadow,background-color,color]",
              chosen && "ring-1 ring-gold-400/70",
            )}
            icon={
              chosen ? (
                <Check size={14} weight="bold" className="text-gold-300" />
              ) : joins ? (
                <Play size={13} weight="fill" />
              ) : undefined
            }
            onClick={() => void answerCall(call, answer)}
          >
            {t(`teams.call.${answer}`)}
          </Button>
        );
      })}
    </fieldset>
  );
}

function Faces({ members }: { members: TeamMember[] }) {
  const { reduce } = useSocialMotion();
  return (
    <span className="flex min-h-5 items-center">
      <AnimatePresence initial={false} mode="popLayout">
        {members.slice(0, 4).map((member, index) => (
          <motion.span
            key={member.id}
            className="relative flex rounded-[4px] ring-2 ring-obsidian-900"
            style={{ marginLeft: index ? -6 : 0, zIndex: 4 - index }}
            {...pop(reduce, 0.16)}
          >
            <SocialFace source={member} size={20} />
          </motion.span>
        ))}
      </AnimatePresence>
    </span>
  );
}

interface Box {
  answer: CallAnswer;
  icon: ReactNode;
  label: string;
}

/** One answer's box. Its right click lists the people in it, each with their own menu (FIX2). */
function AnswerBox({ box, who, team }: { box: Box; who: TeamMember[]; team: SocialTeam }) {
  const { reduce } = useSocialMotion();
  const items = usePeopleItems(who, { crew: team, from: "team" });
  return (
    <ContextMenu items={items} disabled={!items.length}>
      <li
        className="flex min-w-0 flex-col gap-1.5 rounded-md bg-obsidian-950/45 px-2.5 py-2 ring-1 ring-white/6"
        aria-label={`${box.label}: ${who.length}`}
      >
        <span className="flex items-center justify-between gap-1 text-[12px] text-ash-300">
          <span className="flex min-w-0 items-center gap-1">
            <span className={box.answer === "yes" && who.length ? "text-jade-400" : "text-ash-400"}>
              {box.icon}
            </span>
            <span className="truncate">{box.label}</span>
          </span>
          <span className="relative inline-flex font-display text-[13px] font-semibold text-bone-50 tabular-nums">
            <AnimatePresence mode="popLayout" initial={false}>
              <motion.span key={who.length} {...swap(reduce, 0.16)}>
                {who.length}
              </motion.span>
            </AnimatePresence>
          </span>
        </span>
        <Faces members={who} />
      </li>
    </ContextMenu>
  );
}

/** Who answered what: three quiet boxes with faces and counts (this crew's people). */
export function AnswerBoxes({ call, team }: { call: CrewCall; team: SocialTeam }) {
  const { t } = useTranslation();
  const by = answersBy(call);
  const people = (ids: string[]) =>
    ids
      .map((id) => team.members.find((m) => m.id === id))
      .filter((m): m is TeamMember => Boolean(m));
  const boxes: Box[] = [
    { answer: "yes", icon: <Check size={13} weight="bold" />, label: t("teams.call.saidYes") },
    { answer: "soon", icon: <Clock size={13} weight="bold" />, label: t("teams.call.saidSoon") },
    { answer: "no", icon: <X size={13} weight="bold" />, label: t("teams.call.saidNo") },
  ];
  return (
    <ul className="grid grid-cols-3 gap-1.5">
      {boxes.map((box) => (
        <AnswerBox key={box.answer} box={box} who={people(by[box.answer])} team={team} />
      ))}
    </ul>
  );
}

/** The crew card's strip while a call is on: the player's own, or a teammate's. */
export function CallStrip({ call, team }: { call: CrewCall; team: SocialTeam }) {
  const { t } = useTranslation();
  const crews = useSocialUi((s) => s.view.crews);
  const caller = callerIn(call, [team, ...crews]);
  return (
    <div className="space-y-2.5">
      {call.mine ? (
        <div className="flex items-center gap-2.5">
          <span className="flex size-9 shrink-0 items-center justify-center rounded-[var(--radius-head)] bg-gold-500/12 text-gold-300 ring-1 ring-gold-400/25">
            <MegaphoneSimple size={18} weight="duotone" aria-hidden />
          </span>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[14px] font-semibold text-bone-50">
              {t("teams.call.mine")}
            </span>
            {/* Where the crew is called to; "Hazır mısın?" is the teammates' question. */}
            {callWhere(call).kind !== "ready" ? <WhereLine call={call} /> : null}
          </span>
          <MinutesLeft call={call} className="text-ash-300" />
          <Button size="sm" variant="ghost" onClick={() => void endCall(call)}>
            {t("teams.call.end")}
          </Button>
        </div>
      ) : (
        <>
          <div className="flex items-center gap-2.5">
            <SocialAvatar source={caller} size={36} />
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[14px] font-semibold text-bone-50">
                {callSentenceOf(call, crews, t, true)}
              </span>
              <WhereLine call={call} />
            </span>
            <MinutesLeft call={call} className="text-ash-300" />
          </div>
          <AnswerButtons call={call} />
        </>
      )}
      <AnswerBoxes call={call} team={team} />
    </div>
  );
}

/** A teammate's call in the friend list, under the header of the first crew it names (the invite
 * rows' family). Its right-click menu answers too, joins the caller and opens the crew; the caller's
 * own menu follows (FIX2). */
export function CallRow({ call, team }: { call: CrewCall; team: SocialTeam }) {
  const { t } = useTranslation();
  const crews = useSocialUi((s) => s.view.crews);
  const caller = callerIn(call, [team, ...crews]);
  const where = callWhere(call);
  const here = useThere(callTarget(where));
  const openTeam = useSocialUi((s) => s.openTeam);
  const callerItems = usePeopleItems([caller], { crew: team, from: "team" });
  const rows = rowsOf(
    callMenu({ joinable: where.kind !== "ready", here, answer: call.myAnswer }),
    (row): MenuItem | null => {
      switch (row.action) {
        case "yes":
        case "soon":
        case "no": {
          const answer = row.action;
          return {
            label: t(`teams.call.${answer}`),
            checked: Boolean(row.checked),
            radio: true,
            onSelect: () => void answerCall(call, answer),
          };
        }
        case "join":
          return {
            label: t("social.actions.join"),
            icon: <Play size={16} />,
            disabled: row.disabled,
            onSelect: () => joinCall(where),
          };
        case "open":
          return {
            label: t("teams.open"),
            icon: <UsersThree size={16} />,
            onSelect: () => openTeam(undefined, team.id),
          };
      }
    },
  );
  const items = callerItems.length ? [...rows, null, ...callerItems] : rows;
  return (
    <ContextMenu items={items}>
      <div className="mb-1.5 rounded-[var(--radius-panel)] bg-gold-500/[0.07] py-2 pr-1.5 pl-2 ring-1 ring-gold-400/15">
        <div className="flex items-center gap-3">
          <span className="relative flex">
            <SocialAvatar source={caller} size={36} />
            <span className="absolute -right-1 -bottom-1 flex size-4 items-center justify-center rounded-full bg-obsidian-900 text-gold-300">
              <MegaphoneSimple size={10} weight="fill" aria-hidden />
            </span>
          </span>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[13.5px] font-semibold text-bone-50">
              {callSentenceOf(call, crews, t)}
            </span>
            <WhereLine call={call} />
          </span>
          <MinutesLeft call={call} className="pr-1 text-ash-300" />
        </div>
        <AnswerButtons call={call} className="mt-2" />
      </div>
    </ContextMenu>
  );
}

function CallToast({
  toastId,
  call,
  joined,
}: {
  toastId: string | number;
  call: CrewCall;
  joined: boolean;
}) {
  const { t } = useTranslation();
  const crews = useSocialUi((s) => s.view.crews);
  const live = useSocialUi((s) => s.view.calls.find((c) => c.id === call.id)) ?? call;
  const caller = callerIn(live, crews);
  const where = callWhere(live);
  const mark = crewsOfCall(live, crews)[0];
  // A "Hazır mısın?" call's caller who moved went to projects.gg.
  const here = useThere(callTarget(where.kind === "ready" ? { kind: "projects" } : where));
  const name = caller.name || t("teams.call.someone");
  return (
    <div className="w-full font-normal">
      <div className="flex items-center gap-3">
        <span className="relative flex">
          <SocialAvatar source={caller} size={40} />
          {mark ? (
            <span className="absolute -right-1.5 -bottom-1.5">
              <CrewMark look={mark} size={22} />
            </span>
          ) : null}
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-[13.5px] leading-snug font-semibold text-bone-50">
            {joined
              ? where.kind === "server"
                ? t("crews.call.arrivedAt", { name, host: where.host })
                : t("teams.call.arrived", { name })
              : callSentenceOf(live, crews, t)}
          </p>
          <WhereLine call={live} />
        </div>
        <IconButton
          label={t("social.actions.dismiss")}
          icon={<X size={15} />}
          size="sm"
          className="-mr-1 self-start"
          onClick={() => toast.dismiss(toastId)}
        />
      </div>
      {joined ? (
        <Button
          size="sm"
          variant="primary"
          className="mt-2.5 w-full"
          disabled={here}
          onClick={() => {
            toast.dismiss(toastId);
            // A "Hazır mısın?" call's caller who moved went to projects.gg.
            joinCall(where.kind === "ready" ? { kind: "projects" } : where);
          }}
        >
          {t("social.actions.join")}
        </Button>
      ) : (
        <div
          className="mt-2.5"
          onClickCapture={() => window.setTimeout(() => toast.dismiss(toastId), 0)}
        >
          <AnswerButtons call={live} />
        </div>
      )}
    </div>
  );
}

/**
 * A teammate's call arriving (or the caller moving on for a call the player is coming to):
 * 20 seconds, then it waits in the crew card, the list and on Home for the rest of its time.
 */
export function showCallToast(call: CrewCall, joined = false) {
  toast.custom((id) => <CallToast toastId={id} call={call} joined={joined} />, {
    id: `call:${call.id}:${joined ? "join" : "new"}`,
    duration: 20_000,
    classNames: { content: "min-w-0 flex-1", title: "w-full" },
  });
}
