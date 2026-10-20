// "Ekibi çağır" on screen (TEAM2 §4.3). The crew card's strip turns into the call: the caller sees
// "Çağrın sürüyor", the minutes left and "Bitir"; a teammate sees who calls, where (Projects, or
// "Hazır mısın?") and three answers, "Geliyorum" in gold when it can join the caller. Under both,
// three boxes show who is coming, who comes soon and who cannot. The same call sits under the
// crew's header in the friend list, on Home and in a toast when it arrives. Nothing moves by itself:
// the minutes change once a minute, an answer fades in where it lands.
import { Check, Clock, MegaphoneSimple, Play, UsersThree, X } from "@phosphor-icons/react";
import { Button, ContextMenu, cn, IconButton, type MenuItem, toast } from "@pl/ui";
import type { TFunction } from "i18next";
import { AnimatePresence, motion } from "motion/react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { CallAnswer, CrewCall, SocialTeam, TeamMember } from "@/lib/api/social-types";
import { useLaunch } from "@/stores/launch";
import { answerCall, endCall } from "./call-actions";
import { answersBy, minutesLeft } from "./calls";
import { gameBusy } from "./join";
import { joinCall } from "./join-dialog";
import { callMenu, rowsOf } from "./menus";
import { pop, swap, useSocialMotion } from "./motion";
import { type FaceSource, SocialAvatar, SocialFace } from "./social-face";
import { useSocialUi } from "./store";
import { CrewMark } from "./team-emblem";
import { useCallClock } from "./use-call-clock";

const ANSWERS: CallAnswer[] = ["yes", "soon", "no"];

/** The caller as the crew shows them (a name even when they are not in the list yet). */
export function callerOf(
  call: CrewCall,
  team: SocialTeam | undefined,
): FaceSource & { id: string } {
  const member = team?.members.find((m) => m.id === call.from);
  return {
    id: call.from,
    name: member?.name ?? call.name ?? "",
    avatar: member?.avatar ?? call.avatar,
    avatarUrl: member?.avatarUrl ?? call.avatarUrl,
  };
}

/** "Ayşe ekibi çağırıyor" (or "Bir ekip arkadaşın" while the name is not known). */
export function callSentence(call: CrewCall, team: SocialTeam | undefined, t: TFunction) {
  const name = callerOf(call, team).name || t("teams.call.someone");
  return t("teams.call.theirs", { name });
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
  /** Unanswered: "Geliyorum" leads (gold when it joins the caller), "Gelemem" stays quiet.
   * Answered: the chosen one is checked and gold-edged, the others step back. */
  const variant = (answer: CallAnswer): "primary" | "secondary" | "ghost" => {
    if (answered) return answered === answer ? "secondary" : "ghost";
    if (answer === "yes") return call.kind === "join" ? "primary" : "secondary";
    return answer === "soon" ? "secondary" : "ghost";
  };
  return (
    <fieldset className={cn("m-0 flex min-w-0 gap-1.5 border-0 p-0", className)}>
      <legend className="sr-only">{t("teams.call.answer")}</legend>
      {ANSWERS.map((answer) => {
        const chosen = answered === answer;
        const joins = answer === "yes" && call.kind === "join" && !answered;
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

/** Who answered what: three quiet boxes with faces and counts. */
export function AnswerBoxes({ call, team }: { call: CrewCall; team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const by = answersBy(call);
  const people = (ids: string[]) =>
    ids
      .map((id) => team.members.find((m) => m.id === id))
      .filter((m): m is TeamMember => Boolean(m));
  const boxes: Array<{ answer: CallAnswer; icon: ReactNode; label: string }> = [
    { answer: "yes", icon: <Check size={13} weight="bold" />, label: t("teams.call.saidYes") },
    { answer: "soon", icon: <Clock size={13} weight="bold" />, label: t("teams.call.saidSoon") },
    { answer: "no", icon: <X size={13} weight="bold" />, label: t("teams.call.saidNo") },
  ];
  return (
    <ul className="grid grid-cols-3 gap-1.5">
      {boxes.map((box) => {
        const who = people(by[box.answer]);
        return (
          <li
            key={box.answer}
            className="flex min-w-0 flex-col gap-1.5 rounded-md bg-obsidian-950/45 px-2.5 py-2 ring-1 ring-white/6"
            aria-label={`${box.label}: ${who.length}`}
          >
            <span className="flex items-center justify-between gap-1 text-[12px] text-ash-300">
              <span className="flex min-w-0 items-center gap-1">
                <span
                  className={box.answer === "yes" && who.length ? "text-jade-400" : "text-ash-400"}
                >
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
        );
      })}
    </ul>
  );
}

/** The crew card's strip while a call is on: the player's own, or a teammate's. */
export function CallStrip({ call, team }: { call: CrewCall; team: SocialTeam }) {
  const { t } = useTranslation();
  const caller = callerOf(call, team);
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
            <span className="block text-[12.5px] text-ash-300">
              {call.kind === "join" ? t("social.presence.projects") : t("teams.call.ready")}
            </span>
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
                {callSentence(call, team, t)}
              </span>
              <span
                className={cn(
                  "block text-[12.5px]",
                  call.kind === "join" ? "text-jade-400" : "text-ash-300",
                )}
              >
                {call.kind === "join" ? t("social.presence.projects") : t("teams.call.ready")}
              </span>
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

/** A teammate's call in the friend list, right under the crew's header (the invite rows' family).
 * Its right-click menu answers too, joins the caller on Projects and opens the crew. */
export function CallRow({ call, team }: { call: CrewCall; team: SocialTeam }) {
  const { t } = useTranslation();
  const caller = callerOf(call, team);
  const busy = useLaunch((s) => gameBusy(s.phase.kind));
  const openTeam = useSocialUi((s) => s.openTeam);
  const items = rowsOf(
    callMenu({ joinable: call.kind === "join", gameBusy: busy, answer: call.myAnswer }),
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
            onSelect: () => joinCall(),
          };
        case "open":
          return {
            label: t("teams.open"),
            icon: <UsersThree size={16} />,
            onSelect: () => openTeam(),
          };
      }
    },
  );
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
              {callSentence(call, team, t)}
            </span>
            <span
              className={cn(
                "block truncate text-[12.5px]",
                call.kind === "join" ? "text-jade-400" : "text-ash-300",
              )}
            >
              {call.kind === "join" ? t("social.presence.projects") : t("teams.call.ready")}
            </span>
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
  const team = useSocialUi((s) => s.view.team);
  const live = useSocialUi((s) => s.view.calls.find((c) => c.id === call.id)) ?? call;
  const caller = callerOf(live, team);
  const busy = useLaunch((s) => gameBusy(s.phase.kind));
  return (
    <div className="w-full font-normal">
      <div className="flex items-center gap-3">
        <span className="relative flex">
          <SocialAvatar source={caller} size={40} />
          {team ? (
            <span className="absolute -right-1.5 -bottom-1.5">
              <CrewMark look={team} size={22} />
            </span>
          ) : null}
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-[13.5px] leading-snug font-semibold text-bone-50">
            {joined
              ? t("teams.call.arrived", { name: caller.name || t("teams.call.someone") })
              : callSentence(live, team, t)}
          </p>
          <p
            className={cn(
              "truncate text-[12.5px]",
              live.kind === "join" ? "text-jade-400" : "text-ash-300",
            )}
          >
            {live.kind === "join" ? t("social.presence.projects") : t("teams.call.ready")}
          </p>
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
          disabled={busy}
          onClick={() => {
            toast.dismiss(toastId);
            joinCall();
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
 * A teammate's call arriving (or their arrival on Projects for a call the player is coming to):
 * 20 seconds, then it waits in the crew card, the list and on Home for the rest of its time.
 */
export function showCallToast(call: CrewCall, joined = false) {
  toast.custom((id) => <CallToast toastId={id} call={call} joined={joined} />, {
    id: `call:${call.id}:${joined ? "join" : "new"}`,
    duration: 20_000,
    classNames: { content: "min-w-0 flex-1", title: "w-full" },
  });
}
