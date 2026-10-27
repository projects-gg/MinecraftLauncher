// Home screen's friends line: one quiet glass pill over the art, under the banners (model.ts →
// homeStrip decides which, most pressing first):
// - a friend's world invite (HOST1, before every other line): their face, "Mert seni dünyasına
//   çağırıyor", "Katıl" (it says where the join is in place) and × (closes the line here only);
// - a crewmate's call the player has not answered: their face with a small megaphone, "Ayşe ekibi
//   çağırıyor" (naming the crews with several) and "Geliyorum"; × closes it here, for this call
//   only (it is not an answer);
// - a crew plan under way (TEAM5): the crew's mark, the faces of those coming, "Gece Kartalları
//   planı başladı" and "Katıl"; one within the hour says who is coming ("21:00 planına 3 kişi
//   geliyor", "Geliyorum" while unanswered); × closes it here, for this plan only;
// - two or more crewmates on Projects: the crew's mark and their faces, "Ekibinden 4 kişi
//   Projects'te" ("Ekiplerinden 6 kişi…" across several crews, each person once) and "Katıl";
// - in game on a server a call may start from (CALL1): the crews' marks, "Ekibini yanına çağır"
//   and "Çağır" (one crew: called at once; several: the picker); × hides it on that server;
// - friends in game (or the one crewmate on Projects), with "Katıl" when some are on projects.gg;
// - before sign-in, the invitation: three faces, "Arkadaşlarınla birlikte oyna" and "Discord ile
//   giriş yap" (the opt-in screen's own words, SMALL12), which opens the friends panel on that
//   screen (what is shared, the privacy page, and the button that opens Discord);
// - signed in with nobody yet, the player's face, "Henüz arkadaşın yok" and "Arkadaş ekle".
// The invitations close for good with ×; the friends button then lights up once wherever it is,
// to show where friends live (from the keyboard the focus goes there too). A line that replaces
// another cross-fades in place; the strips below glide. A line about people opens a menu on a right
// click (Shift+F10, the menu key): its own actions (its × under its own name: "Çağrıyı gizle"),
// then one person's rows, or each of several people as a submenu (FIX2, SMALL10). "Katıl" is the
// event card's: secondary with the gold play glyph. Faces are round, as the pill is (SMALL10b).
import { Check, MegaphoneSimple, Play, UserPlus, UsersThree, X } from "@phosphor-icons/react";
import { ContextMenu, cn, easeOutExpo, IconButton, type MenuItem } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type ReactElement, type ReactNode, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { presetAvatar } from "@/features/avatars/presets";
import { homeInvite } from "@/features/host/rules";
import { useHostUi } from "@/features/host/store";
import { HomeWorldLine } from "@/features/host/world-invite";
import { api } from "@/lib/api";
import type { CrewCall, SocialSnapshot, TeamMember } from "@/lib/api/social-types";
import { dateTimeFormat } from "@/lib/format";
import { useLaunch } from "@/stores/launch";
import { entrySelector, usePlace } from "@/stores/placement";
import { attempt } from "./actions";
import { answerCall, callOrAsk } from "./call-actions";
import { crewsOfCall, unansweredCall } from "./calls";
import { answersBy, homePlan } from "./crew-plans";
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
import { joinOrAsk, PROJECTS, useThere } from "./join-dialog";
import { openMenuFromKey } from "./menu-key";
import {
  crewmatesOnProjects,
  type HomeHint,
  type HomeStrip,
  homeStrip,
  isJoinable,
  playingPeople,
} from "./model";
import { enter, leave, useSocialMotion } from "./motion";
import { MarkedSentence, markedName } from "./person-mark";
import { type PersonPlace, type PersonRef, usePeopleItems } from "./person-menu";
import { type FaceSource, SocialFace } from "./social-face";
import { useSocialPrefs, useSocialUi } from "./store";
import { CrewMark } from "./team-emblem";
import { useCallClock } from "./use-call-clock";

/** The opt-in screen's three faces: the same friends greet the player there. */
const INVITE_FACES: FaceSource[] = ["fox", "golden-dragon", "wizard"].map((slug) => ({
  name: presetAvatar(`preset:${slug}`)?.name ?? slug,
  avatar: `preset:${slug}`,
}));

/** Up to four round faces: inside the round pill a square's corners met its round end. */
function Faces({ sources }: { sources: Array<FaceSource & { id?: string }> }) {
  return (
    <span className="flex shrink-0 items-center">
      {sources.slice(0, 4).map((source, index) => (
        <span
          key={source.id ?? source.avatar ?? source.name}
          className="relative flex rounded-full ring-2 ring-obsidian-900"
          style={{ marginLeft: index ? -8 : 0, zIndex: 4 - index }}
        >
          <SocialFace source={source} size={26} className="overflow-hidden rounded-full" />
        </span>
      ))}
    </span>
  );
}

/** A line's own "Katıl" is the person's too: their rows follow without it. */
const SKIP_JOIN = ["join"] as const;

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
          {connect ? t("social.consent.signIn") : t("social.actions.add")}
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

/**
 * A line's right click (FIX2): its own actions, then one person's rows or a submenu for each of
 * several (person-menu.tsx `usePeopleItems`). Shift+F10 and the menu key on the focused line
 * open it too.
 */
function LineMenu({
  actions,
  people,
  place,
  children,
}: {
  actions: Array<MenuItem | null>;
  people: readonly PersonRef[];
  place?: PersonPlace;
  children: ReactElement;
}) {
  const submenus = usePeopleItems(people, place);
  const items = [...actions, ...(actions.length && submenus.length ? [null] : []), ...submenus];
  return (
    <ContextMenu items={items} disabled={!items.length}>
      {children}
    </ContextMenu>
  );
}

function PlayingLine() {
  const { t } = useTranslation();
  const view = useSocialUi((s) => s.view);
  const favorites = useSocialPrefs((s) => s.favorites);
  const openPanel = useSocialUi((s) => s.openPanel);
  const here = useThere(PROJECTS);
  const playing = playingPeople(view, favorites);
  const onProjects = playing.filter((f) => isJoinable(f.game));
  // projects.gg first: that is where "Katıl" leads.
  const shown = onProjects.length ? onProjects : playing;
  const first = shown[0];
  if (!first) return null;
  const others = shown.length - 1;
  const where = onProjects.length ? "projects" : "playing";
  const named = markedName(first.name, first.mark, t);
  const text = others
    ? t(`social.home.${where}With`, { name: named, count: others })
    : t(`social.home.${where}Alone`, { name: named });

  return (
    <>
      <LineMenu
        actions={
          onProjects.length
            ? [
                {
                  label: t("social.actions.join"),
                  icon: <Play size={16} />,
                  disabled: here,
                  onSelect: () => joinOrAsk(PROJECTS),
                },
              ]
            : []
        }
        people={shown}
        place={onProjects.length ? { skip: SKIP_JOIN } : undefined}
      >
        <button
          type="button"
          aria-label={`${text}. ${t("social.home.open")}`}
          onClick={() => openPanel({ page: "list" })}
          onKeyDown={(event) => {
            openMenuFromKey(event);
          }}
          className="flex min-w-0 cursor-pointer items-center gap-3 rounded-full py-0.5 pr-2.5 pl-0.5 outline-none transition-colors duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          <Faces sources={shown} />
          <span className="truncate text-[13.5px] font-medium text-bone-50">
            {/* The first friend's mark right after their name (SOC-VERIFY). */}
            <MarkedSentence
              text={(name) =>
                others
                  ? t(`social.home.${where}With`, { name, count: others })
                  : t(`social.home.${where}Alone`, { name })
              }
              name={first.name}
              mark={first.mark}
            />
          </span>
        </button>
      </LineMenu>
      {onProjects.length ? <JoinButton glyph /> : null}
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
  const joins = callWhere(call).kind !== "ready";
  const crew = crewsOfCall(call, view.crews)[0];
  const caller = callerIn(call, view.crews);
  const callerName = caller.name || t("teams.call.someone");
  const text = callSentenceOf(call, view.crews, t, false, markedName(callerName, caller.mark, t));
  const open = (from?: HTMLElement | null) => {
    openPanel({ page: "team", from });
    setCrewTab(crew?.id ?? null);
  };
  return (
    <>
      <LineMenu
        actions={[
          {
            label: t("teams.call.yes"),
            icon: joins ? <Play size={16} /> : <Check size={16} />,
            onSelect: () => void answerCall(call, "yes"),
          },
          { label: t("teams.open"), icon: <UsersThree size={16} />, onSelect: () => open() },
          // The line's × as well, by its name (FIX2 menu table; SMALL10: a bare "Kapat" did not
          // say what closes, nor that the call goes from Home only).
          {
            label: t("small10.menu.hideCall"),
            icon: <X size={16} />,
            onSelect: () => closeCall(call.id),
          },
        ]}
        people={call.mine ? [] : [caller]}
        place={{ crew, from: "team" }}
      >
        <button
          type="button"
          aria-label={`${text}. ${t("teams.call.open")}`}
          onClick={(event) => open(event.currentTarget)}
          onKeyDown={(event) => {
            openMenuFromKey(event);
          }}
          className="flex min-w-0 cursor-pointer items-center gap-3 rounded-full py-0.5 pr-2 pl-0.5 outline-none transition-colors duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          <span className="relative flex">
            <SocialFace source={caller} size={26} className="overflow-hidden rounded-full" />
            <span className="absolute -right-1.5 -bottom-1 flex size-[14px] items-center justify-center rounded-full bg-obsidian-950 text-gold-300 ring-1 ring-gold-400/40">
              <MegaphoneSimple size={9} weight="fill" aria-hidden />
            </span>
          </span>
          <span className="truncate text-[13.5px] font-medium text-bone-50">
            <MarkedSentence
              text={(name) => callSentenceOf(call, view.crews, t, false, name)}
              name={callerName}
              mark={caller.mark}
            />
          </span>
        </button>
      </LineMenu>
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
        label={t("small10.menu.hideCall")}
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
  const here = useThere(PROJECTS);
  const team = busiestCrew(view.crews);
  const crew = crewmatesOnProjects(view.crews);
  if (!team) return null;
  const text =
    view.crews.length > 1
      ? t("crews.home.crewProjects", { count: crew.length })
      : t("teams.home.crewProjects", { count: crew.length });
  const open = (from?: HTMLElement | null) => {
    openPanel({ page: "team", from });
    setCrewTab(team.id);
  };
  return (
    <>
      <LineMenu
        actions={[
          {
            label: t("social.actions.join"),
            icon: <Play size={16} />,
            disabled: here,
            onSelect: () => joinOrAsk(PROJECTS),
          },
          { label: t("teams.open"), icon: <UsersThree size={16} />, onSelect: () => open() },
        ]}
        people={crew}
        place={
          view.crews.length === 1
            ? { crew: team, from: "team", skip: SKIP_JOIN }
            : { from: "team", skip: SKIP_JOIN }
        }
      >
        <button
          type="button"
          aria-label={`${text}. ${t("teams.open")}`}
          onClick={(event) => open(event.currentTarget)}
          onKeyDown={(event) => {
            openMenuFromKey(event);
          }}
          className="flex min-w-0 cursor-pointer items-center gap-2.5 rounded-full py-0.5 pr-2.5 pl-1 outline-none transition-colors duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          <CrewMark look={team} size={28} />
          <Faces sources={crew} />
          <span className="truncate text-[13.5px] font-medium text-bone-50">{text}</span>
        </button>
      </LineMenu>
      <JoinButton glyph />
    </>
  );
}

/**
 * A crew plan under way ("… planı başladı", "Katıl") or within the hour ("21:00 planına 3 kişi
 * geliyor", "Geliyorum" while unanswered), with the faces of those coming; × closes it here.
 */
function PlanLine() {
  const { t } = useTranslation();
  const view = useSocialUi((s) => s.view);
  const closed = useSocialUi((s) => s.closedPlans);
  const closePlan = useSocialUi((s) => s.closePlan);
  const openPanel = useSocialUi((s) => s.openPanel);
  const setCrewTab = useSocialUi((s) => s.setCrewTab);
  const now = useCallClock((s) => s.now);
  const found = homePlan(view.plans, now, closed);
  const crew = found ? view.crews.find((one) => one.id === found.plan.crew) : undefined;
  if (!found || !crew) return null;
  const { plan, phase } = found;
  const coming = answersBy(plan).yes;
  const faces = coming
    .map((id) => crew.members.find((m) => m.id === id))
    .filter((m): m is TeamMember => Boolean(m) && !m?.blocked);
  const time = dateTimeFormat({ hour: "2-digit", minute: "2-digit" }).format(
    new Date(plan.at * 1000),
  );
  const text =
    phase === "live"
      ? t("crewWave2.plans.started", { crew: crew.name })
      : coming.length
        ? t("crewWave2.plans.home.soonComing", { time, count: coming.length })
        : t("crewWave2.plans.home.soon", { time });
  const open = (from?: HTMLElement | null) => {
    openPanel({ page: "team", from });
    setCrewTab(crew.id);
  };
  // The line's own actions on a right click, as its buttons have them (FIX2).
  const joins = phase === "live" && plan.myAnswer !== "no";
  const actions: MenuItem[] = [
    ...(joins
      ? [
          {
            label: t("social.actions.join"),
            icon: <Play size={16} />,
            onSelect: () => joinOrAsk(PROJECTS),
          },
        ]
      : !plan.myAnswer
        ? [
            {
              label: t("crewWave2.plans.yes"),
              icon: <Check size={16} />,
              onSelect: () => void attempt(() => api.social.answerPlan(plan.id, "yes")),
            },
          ]
        : []),
    { label: t("teams.open"), icon: <UsersThree size={16} />, onSelect: () => open() },
    {
      label: t("small10.menu.hidePlan"),
      icon: <X size={16} />,
      onSelect: () => closePlan(plan.id),
    },
  ];
  return (
    <>
      <LineMenu
        actions={actions}
        people={faces}
        place={{ crew, from: "team", ...(joins ? { skip: SKIP_JOIN } : {}) }}
      >
        <button
          type="button"
          aria-label={`${text}${plan.note ? `: ${plan.note}` : ""}. ${t("teams.open")}`}
          onClick={(event) => open(event.currentTarget)}
          onKeyDown={(event) => {
            openMenuFromKey(event);
          }}
          className="flex min-w-0 cursor-pointer items-center gap-2.5 rounded-full py-0.5 pr-2.5 pl-1 outline-none transition-colors duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          <CrewMark look={crew} size={28} />
          {faces.length ? <Faces sources={faces} /> : null}
          <span className="shrink-0 text-[13.5px] font-medium text-bone-50">{text}</span>
          {/* The plan's note (FIX2), quieter, cut where the pill ends. */}
          {plan.note ? (
            <span className="min-w-0 truncate text-[13px] text-ash-300">"{plan.note}"</span>
          ) : null}
        </button>
      </LineMenu>
      {joins ? (
        <JoinButton glyph />
      ) : !plan.myAnswer ? (
        <button
          type="button"
          onClick={() => void attempt(() => api.social.answerPlan(plan.id, "yes"))}
          className="group flex shrink-0 cursor-pointer rounded-full outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          <ActionLook icon={<Check size={13} weight="bold" aria-hidden />}>
            {t("crewWave2.plans.yes")}
          </ActionLook>
        </button>
      ) : null}
      <IconButton
        size="sm"
        label={t("small10.menu.hidePlan")}
        icon={<X size={13} weight="bold" />}
        onClick={() => closePlan(plan.id)}
        className="shrink-0 rounded-full"
      />
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

/** What Home's friends line is chosen from: the stores' values (the strip's hooks, or the stores
 * as they are now for `homeShowsCall`). */
interface HomeLineInputs {
  view: SocialSnapshot;
  closed: readonly HomeHint[];
  closedCalls: readonly string[];
  closedPlans: readonly string[];
  /** Where "Ekibini çağır" was closed (`callLineKey`). */
  closedOn: string | null;
  refusedUntil: number;
  lastCallAt: number | null;
  /** A game is being prepared or running. */
  busy: boolean;
  now: number;
  world: ReturnType<typeof homeInvite>;
}

/** The line Home shows (model.ts `homeStrip`); a friend's world invite (HOST1) before all: it asks
 * the player personally. */
function homeLine(input: HomeLineInputs): HomeStrip | "world" | null {
  if (input.world) return "world";
  const { view, now } = input;
  const callable =
    view.teams &&
    callLineShown({
      place: view.callPlace,
      activity: view.activity,
      closedOn: input.closedOn,
      crews: view.crews,
      features: view.crew,
      calls: view.calls,
      refusedUntil: input.refusedUntil,
      lastCallAt: input.lastCallAt,
      now,
    });
  const plan = homePlan(view.plans, now, input.closedPlans)?.phase ?? null;
  return homeStrip(view, input.closed, {
    closedCalls: input.closedCalls,
    gameBusy: input.busy,
    now,
    callable,
    plan,
  });
}

/** Home's friends lines on screen (Home is the page; the strip counts itself). */
let homeLinesShown = 0;

/**
 * Whether Home shows this crew call now, with its "Geliyorum" (the call line): a toast would say
 * the same call a second time (SMALL12; social-host). The line's own choice from the stores as they
 * are, the call counted in when the list has not brought it yet.
 */
export function homeShowsCall(call: CrewCall, now = Date.now()): boolean {
  if (!homeLinesShown) return false;
  const ui = useSocialUi.getState();
  const view = ui.view.calls.some((c) => c.id === call.id)
    ? ui.view
    : { ...ui.view, calls: [call, ...ui.view.calls] };
  const host = useHostUi.getState();
  const kind = homeLine({
    view,
    closed: useSocialPrefs.getState().closedHints,
    closedCalls: ui.closedCalls,
    closedPlans: ui.closedPlans,
    closedOn: ui.callLineClosed,
    refusedUntil: ui.callRefusedUntil,
    lastCallAt: ui.lastCallAt,
    busy: gameBusy(useLaunch.getState().phase.kind),
    now,
    world: host.view.enabled ? homeInvite(host.view.invites, host.homeClosed) : null,
  });
  return kind === "call" && unansweredCall(view.calls, now, ui.closedCalls)?.id === call.id;
}

export function FriendsHomeStrip() {
  const { reduce, layout } = useSocialMotion();
  const view = useSocialUi((s) => s.view);
  const closed = useSocialPrefs((s) => s.closedHints);
  const closedCalls = useSocialUi((s) => s.closedCalls);
  const closedPlans = useSocialUi((s) => s.closedPlans);
  const closedOn = useSocialUi((s) => s.callLineClosed);
  const refusedUntil = useSocialUi((s) => s.callRefusedUntil);
  const lastCallAt = useSocialUi((s) => s.lastCallAt);
  const busy = useLaunch((s) => gameBusy(s.phase.kind));
  const now = useCallClock((s) => s.now);
  const world = useHostUi((s) =>
    s.view.enabled ? homeInvite(s.view.invites, s.homeClosed) : null,
  );
  const kind = homeLine({
    view,
    closed,
    closedCalls,
    closedPlans,
    closedOn,
    refusedUntil,
    lastCallAt,
    busy,
    now,
    world,
  });
  useEffect(() => {
    homeLinesShown += 1;
    return () => {
      homeLinesShown -= 1;
    };
  }, []);

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
          {kind === "world" ? (
            world ? (
              <HomeWorldLine invite={world} />
            ) : null
          ) : kind === "playing" ? (
            <PlayingLine />
          ) : kind === "call" ? (
            <CallLine />
          ) : kind === "crew" ? (
            <CrewLine />
          ) : kind === "callCrew" ? (
            <CallCrewLine />
          ) : kind === "plan" ? (
            <PlanLine />
          ) : (
            <InviteLine hint={kind} />
          )}
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
