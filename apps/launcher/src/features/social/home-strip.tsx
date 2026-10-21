// Home screen's friends line: one quiet glass pill over the art, under the banners (model.ts →
// homeStrip decides which, most pressing first):
// - a crewmate's call the player has not answered: their face with a small megaphone, "Ayşe ekibi
//   çağırıyor" (naming the crews with several) and "Geliyorum"; × closes it here, for this call
//   only (it is not an answer);
// - two or more crewmates on Projects: the crew's mark and their faces, "Ekibinden 4 kişi
//   Projects'te" ("Ekiplerinden 6 kişi…" across several crews, each person once) and "Katıl";
// - in game on a server a call may start from (CALL1): the crews' marks, "Ekibini yanına çağır"
//   and "Çağır" (one crew: called at once; several: the picker); × hides it on that server;
// - friends in game (or the one crewmate on Projects), with "Katıl" when some are on projects.gg;
// - before sign-in, the invitation: three faces, "Arkadaşlarınla birlikte oyna" and "Discord ile
//   bağlan", which opens the friends panel on its opt-in screen (what is shared, the privacy page,
//   and the button that opens Discord);
// - signed in with nobody yet, the player's face, "Henüz arkadaşın yok" and "Arkadaş ekle".
// The invitations close for good with ×; the friends button then lights up once wherever it is,
// to show where friends live (from the keyboard the focus goes there too). A line that replaces
// another cross-fades in place; the strips below glide.
import { MegaphoneSimple, Play, UserPlus, X } from "@phosphor-icons/react";
import { cn, easeOutExpo, IconButton } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { presetAvatar } from "@/features/avatars/presets";
import { useLaunch } from "@/stores/launch";
import { entrySelector, usePlace } from "@/stores/placement";
import { answerCall, callOrAsk } from "./call-actions";
import { crewsOfCall, unansweredCall } from "./calls";
import {
  busiestCrew,
  callableCrews,
  callerIn,
  callLineKey,
  callLineShown,
  callSentenceOf,
  callWhere,
  crewsWithCall,
} from "./crews";
import { DiscordMark } from "./discord-button";
import { JoinButton } from "./friend-row";
import { gameBusy } from "./join";
import { crewmatesOnProjects, type HomeHint, homeStrip, isJoinable, playingPeople } from "./model";
import { enter, leave, useSocialMotion } from "./motion";
import { type FaceSource, SocialFace } from "./social-face";
import { useSocialPrefs, useSocialUi } from "./store";
import { CrewMark } from "./team-emblem";
import { useCallClock } from "./use-call-clock";

/** The opt-in screen's three faces: the same friends greet the player there. */
const INVITE_FACES: FaceSource[] = ["fox", "golden-dragon", "wizard"].map((slug) => ({
  name: presetAvatar(`preset:${slug}`)?.name ?? slug,
  avatar: `preset:${slug}`,
}));

function Faces({ sources }: { sources: Array<FaceSource & { id?: string }> }) {
  return (
    <span className="flex shrink-0 items-center">
      {sources.slice(0, 4).map((source, index) => (
        <span
          key={source.id ?? source.avatar ?? source.name}
          className="relative flex rounded-[5px] ring-2 ring-obsidian-900"
          style={{ marginLeft: index ? -8 : 0, zIndex: 4 - index }}
        >
          <SocialFace source={source} size={26} />
        </span>
      ))}
    </span>
  );
}

/**
 * The line's action, drawn as a secondary button inside the line's own button: one control for
 * the keyboard, and the press squeezes where the player looks.
 */
function ActionLook({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return (
    <span className="relative isolate inline-flex h-8 shrink-0 items-center gap-1.5 px-3 text-[12.5px] font-medium whitespace-nowrap text-bone-100 transition-[transform,scale] duration-150 ease-out [--cut:6px] group-active:scale-[0.97]">
      <span
        aria-hidden
        className="chamfer absolute inset-0 -z-10 bg-white/12 transition-colors group-hover:bg-white/20"
      />
      <span
        aria-hidden
        className="chamfer absolute inset-px -z-10 bg-obsidian-800/85 transition-colors group-hover:bg-obsidian-700/90"
      />
      {icon}
      <span className="leading-none">{children}</span>
    </span>
  );
}

function InviteLine({ hint }: { hint: HomeHint }) {
  const { t } = useTranslation();
  const me = useSocialUi((s) => s.view.me);
  const openPanel = useSocialUi((s) => s.openPanel);
  const cueEntry = useSocialUi((s) => s.cueEntry);
  const closeHint = useSocialPrefs((s) => s.closeHint);
  const place = usePlace("friends");
  const connect = hint === "connect";
  const faces = connect ? INVITE_FACES : me ? [me] : [];

  return (
    <>
      <button
        type="button"
        // Signed out the panel shows its opt-in screen whatever the page.
        onClick={(event) =>
          openPanel({ page: connect ? "list" : "add", from: event.currentTarget })
        }
        className={cn(
          "group flex min-w-0 cursor-pointer items-center gap-3 rounded-full py-0.5 pr-0.5 outline-none transition-colors duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300",
          faces.length ? "pl-0.5" : "pl-3",
        )}
      >
        <Faces sources={faces} />
        <span className="truncate text-[13.5px] font-medium text-bone-50">
          {connect ? t("social.consent.title") : t("social.list.emptyTitle")}
        </span>
        <ActionLook
          icon={connect ? <DiscordMark height={12} /> : <UserPlus size={15} aria-hidden />}
        >
          {connect ? t("social.account.connect") : t("social.actions.add")}
        </ActionLook>
      </button>
      <IconButton
        size="sm"
        label={t("social.actions.dismiss")}
        icon={<X size={13} weight="bold" />}
        onClick={(event) => {
          // A click without a pointer (Enter or Space) came from the keyboard.
          const fromKeyboard = event.detail === 0;
          closeHint(hint);
          cueEntry();
          if (fromKeyboard) {
            document.querySelector<HTMLElement>(entrySelector("friends", place))?.focus();
          }
        }}
        className="shrink-0 rounded-full"
      />
    </>
  );
}

function PlayingLine() {
  const { t } = useTranslation();
  const view = useSocialUi((s) => s.view);
  const favorites = useSocialPrefs((s) => s.favorites);
  const openPanel = useSocialUi((s) => s.openPanel);
  const playing = playingPeople(view, favorites);
  const onProjects = playing.filter((f) => isJoinable(f.game));
  // projects.gg first: that is where "Katıl" leads.
  const shown = onProjects.length ? onProjects : playing;
  const first = shown[0];
  if (!first) return null;
  const others = shown.length - 1;
  const where = onProjects.length ? "projects" : "playing";
  const text = others
    ? t(`social.home.${where}With`, { name: first.name, count: others })
    : t(`social.home.${where}Alone`, { name: first.name });

  return (
    <>
      <button
        type="button"
        aria-label={`${text}. ${t("social.home.open")}`}
        onClick={() => openPanel({ page: "list" })}
        className="flex min-w-0 cursor-pointer items-center gap-3 rounded-full py-0.5 pr-2.5 pl-0.5 outline-none transition-colors duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        <Faces sources={shown} />
        <span className="truncate text-[13.5px] font-medium text-bone-50">{text}</span>
      </button>
      {onProjects.length ? <JoinButton /> : null}
    </>
  );
}

/** A crewmate's call the player has not answered: "Geliyorum" answers (and joins from Projects). */
function CallLine() {
  const { t } = useTranslation();
  const view = useSocialUi((s) => s.view);
  const closed = useSocialUi((s) => s.closedCalls);
  const closeCall = useSocialUi((s) => s.closeCall);
  const openPanel = useSocialUi((s) => s.openPanel);
  const now = useCallClock((s) => s.now);
  const setCrewTab = useSocialUi((s) => s.setCrewTab);
  const call = unansweredCall(view.calls, now, closed);
  if (!call) return null;
  const text = callSentenceOf(call, view.crews, t);
  const joins = callWhere(call).kind !== "ready";
  return (
    <>
      <button
        type="button"
        aria-label={`${text}. ${t("teams.call.open")}`}
        onClick={(event) => {
          openPanel({ page: "team", from: event.currentTarget });
          setCrewTab(crewsOfCall(call, view.crews)[0]?.id ?? null);
        }}
        className="flex min-w-0 cursor-pointer items-center gap-3 rounded-full py-0.5 pr-2 pl-0.5 outline-none transition-colors duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        <span className="relative flex">
          <SocialFace source={callerIn(call, view.crews)} size={26} />
          <span className="absolute -right-1.5 -bottom-1 flex size-[14px] items-center justify-center rounded-full bg-obsidian-950 text-gold-300 ring-1 ring-gold-400/40">
            <MegaphoneSimple size={9} weight="fill" aria-hidden />
          </span>
        </span>
        <span className="truncate text-[13.5px] font-medium text-bone-50">{text}</span>
      </button>
      <button
        type="button"
        onClick={() => void answerCall(call, "yes")}
        className="group flex shrink-0 cursor-pointer rounded-full outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        <ActionLook icon={joins ? <Play size={12} weight="fill" aria-hidden /> : null}>
          {t("teams.call.yes")}
        </ActionLook>
      </button>
      <IconButton
        size="sm"
        label={t("social.actions.dismiss")}
        icon={<X size={13} weight="bold" />}
        onClick={() => closeCall(call.id)}
        className="shrink-0 rounded-full"
      />
    </>
  );
}

/** Two or more crewmates on Projects (across the crews, each once): the mark of the crew with
 * the most of them, their faces, then "Katıl". */
function CrewLine() {
  const { t } = useTranslation();
  const view = useSocialUi((s) => s.view);
  const openPanel = useSocialUi((s) => s.openPanel);
  const setCrewTab = useSocialUi((s) => s.setCrewTab);
  const team = busiestCrew(view.crews);
  const crew = crewmatesOnProjects(view.crews);
  if (!team) return null;
  const text =
    view.crews.length > 1
      ? t("crews.home.crewProjects", { count: crew.length })
      : t("teams.home.crewProjects", { count: crew.length });
  return (
    <>
      <button
        type="button"
        aria-label={`${text}. ${t("teams.open")}`}
        onClick={(event) => {
          openPanel({ page: "team", from: event.currentTarget });
          setCrewTab(team.id);
        }}
        className="flex min-w-0 cursor-pointer items-center gap-2.5 rounded-full py-0.5 pr-2.5 pl-1 outline-none transition-colors duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        <CrewMark look={team} size={28} />
        <Faces sources={crew} />
        <span className="truncate text-[13.5px] font-medium text-bone-50">{text}</span>
      </button>
      <JoinButton />
    </>
  );
}

/**
 * In game where a call may start: "Ekibini yanına çağır" with the marks of the crews to call.
 * "Çağır" calls the one crew at once, or opens the picker for several; × hides it on this server.
 */
function CallCrewLine() {
  const { t } = useTranslation();
  const view = useSocialUi((s) => s.view);
  const closeCallLine = useSocialUi((s) => s.closeCallLine);
  const openCallPicker = useSocialUi((s) => s.openCallPicker);
  const now = useCallClock((s) => s.now);
  const busy = crewsWithCall(view.calls, view.crews, now);
  const crews = callableCrews(view.crews, view.crew).filter((crew) => !busy.has(crew.id));
  const [first] = crews;
  if (!first) return null;
  const several = crews.length > 1;
  const text = several ? t("crews.call.homeMany") : t("crews.call.homeOne");
  return (
    <>
      <button
        type="button"
        aria-label={text}
        onClick={() => (several ? openCallPicker([]) : void callOrAsk([first.id]))}
        className="group flex min-w-0 cursor-pointer items-center gap-3 rounded-full py-0.5 pr-0.5 pl-1 outline-none transition-colors duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        <span className="flex shrink-0 items-center">
          {crews.slice(0, 3).map((crew, index) => (
            <span
              key={crew.id}
              className="relative flex"
              style={{ marginLeft: index ? -10 : 0, zIndex: 3 - index }}
            >
              <CrewMark look={crew} size={28} />
            </span>
          ))}
        </span>
        <span className="truncate text-[13.5px] font-medium text-bone-50">{text}</span>
        <ActionLook icon={<MegaphoneSimple size={13} weight="fill" aria-hidden />}>
          {t("crews.call.homeAction")}
        </ActionLook>
      </button>
      <IconButton
        size="sm"
        label={t("social.actions.dismiss")}
        icon={<X size={13} weight="bold" />}
        onClick={() => closeCallLine(callLineKey(view.activity))}
        className="shrink-0 rounded-full"
      />
    </>
  );
}

export function FriendsHomeStrip() {
  const { reduce, layout } = useSocialMotion();
  const view = useSocialUi((s) => s.view);
  const closed = useSocialPrefs((s) => s.closedHints);
  const closedCalls = useSocialUi((s) => s.closedCalls);
  const closedOn = useSocialUi((s) => s.callLineClosed);
  const refusedUntil = useSocialUi((s) => s.callRefusedUntil);
  const lastCallAt = useSocialUi((s) => s.lastCallAt);
  const busy = useLaunch((s) => gameBusy(s.phase.kind));
  const now = useCallClock((s) => s.now);
  const callable =
    view.teams &&
    callLineShown({
      place: view.callPlace,
      activity: view.activity,
      closedOn,
      crews: view.crews,
      features: view.crew,
      calls: view.calls,
      refusedUntil,
      lastCallAt,
      now,
    });
  const kind = homeStrip(view, closed, { closedCalls, gameBusy: busy, now, callable });

  return (
    <AnimatePresence initial={false} mode="popLayout">
      {kind ? (
        <motion.div
          key={kind}
          data-friends-strip={kind}
          // Glides with the banners above it (LayoutGroup on Home).
          layout={layout ? "position" : false}
          transition={{ layout: { duration: 0.32, ease: easeOutExpo } }}
          className={cn(
            "glass flex w-fit max-w-[560px] items-center rounded-full py-1.5 pl-1.5",
            kind === "playing" || kind === "crew" ? "gap-3 pr-1.5" : "gap-1 pr-1",
          )}
          initial={{ opacity: 0, transform: reduce ? "none" : "translateY(8px)" }}
          animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.42) }}
          exit={{
            opacity: 0,
            transform: reduce ? "none" : "translateY(4px)",
            transition: leave(0.3),
          }}
        >
          {kind === "playing" ? (
            <PlayingLine />
          ) : kind === "call" ? (
            <CallLine />
          ) : kind === "crew" ? (
            <CrewLine />
          ) : kind === "callCrew" ? (
            <CallCrewLine />
          ) : (
            <InviteLine hint={kind} />
          )}
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
