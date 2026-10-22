// One menu for a person, wherever they show (FIX2; the owner: "oyuncunun aklına gelebilecek
// refleksif yerlere böyle işlevler kazandırmak lazım"): a friend in the list, a crewmate in a crew
// or on the crew page, "Arkadaşlarını davet et", a plan's or a call's answers, a message's sender
// and a chat's header, Home's lines, requests. The rows and their order come from
// menus.ts → `personMenu`; this file words them, gives them their icons and does them. What needs
// one sentence first (unfriend, block, hand on the leadership, remove from the crew) is asked in
// the row's own card where it has one, else in a small dialog here.
import {
  Bell,
  ChatCircle,
  Check,
  Crown,
  DiscordLogo,
  DotsThree,
  Flag,
  FolderSimple,
  FolderSimplePlus,
  PaperPlaneTilt,
  Play,
  Prohibit,
  ShieldStar,
  Star,
  UserCheck,
  UserMinus,
  UserPlus,
  UsersThree,
  X,
} from "@phosphor-icons/react";
import { Button, ContextMenu, Dialog, IconButton, Menu, type MenuItem, toast } from "@pl/ui";
import type { TFunction } from "i18next";
import { type ReactElement, useRef } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { SocialProfile, SocialSnapshot, SocialTeam, TeamMember } from "@/lib/api/social-types";
import { attempt, inviteCoolingDown, inviteFriend } from "./actions";
import { directConversation } from "./chat/conversations";
import { type InviteState, inviteTargets } from "./crews";
import { openInDiscord } from "./discord-link";
import { REST } from "./groups";
import { joinOrAsk, PROJECTS, useThere } from "./join-dialog";
import { personMenu, rowsOf } from "./menus";
import { isJoinable, notifiesOnline } from "./model";
import { canInvite } from "./play-together";
import { type SocialPage, useSocialPrefs, useSocialUi } from "./store";
import { CrewMark } from "./team-emblem";
import { afterHandingOn, canRemove, MAX_MODERATORS } from "./teams";

/** Who the menu is about, as the place showing them knows them. */
export interface PersonRef extends Pick<SocialProfile, "id" | "name"> {
  username?: string;
  avatar?: string;
  avatarUrl?: string;
  discordUrl?: string;
}

/** What needs one sentence first. */
export type PersonQuestion = "removeFriend" | "block" | "makeFounder" | "removeMember";

/** The friend list's categories, for "Kategoriye taşı" (only the list has them). */
export interface PersonCategories {
  groups: Array<{ id: string; name: string }>;
  /** The friend's category id, or REST. */
  current: string;
  onMove: (group: string) => void;
  onNew: () => void;
}

/** Where the person shows, which decides a few rows. */
export interface PersonPlace {
  /** The crew the row shows them in: its roles and the player's rights there. */
  crew?: SocialTeam;
  /** The row is a request: its answers lead (a join request names its crew). */
  request?: "incoming" | "outgoing" | { join: SocialTeam };
  /** Their conversation is the one on screen: no "Mesaj gönder". */
  inConversation?: boolean;
  /** The friend list's category move. */
  categories?: PersonCategories | null;
  /** The page a conversation opened from here goes back to. */
  from?: SocialPage;
  /** Asks in the row's own card (friend and crewmate rows); elsewhere the dialog below asks. */
  ask?: (question: PersonQuestion) => void;
}

// --- the questions ---------------------------------------------------------------------------------

interface Asked {
  question: PersonQuestion;
  person: PersonRef;
  crew?: SocialTeam;
}

const usePersonAsk = create<{ asked: Asked | null }>()(() => ({ asked: null }));

/** Asks the one sentence (the dialog, mounted once by SocialHost). */
export function askAboutPerson(asked: Asked) {
  // After the menu that asked has let go of the focus.
  window.setTimeout(() => usePersonAsk.setState({ asked }), 0);
}

/** Does what was asked about; true when it went through. */
export async function answerAboutPerson(
  { question, person, crew }: Asked,
  t: TFunction,
): Promise<boolean> {
  const done = await attempt(() => {
    switch (question) {
      case "removeFriend":
        return api.social.removeFriend(person.id);
      case "block":
        return api.social.block(person.id);
      case "makeFounder":
        return crew ? api.social.transferTeam(crew.id, person.id) : Promise.resolve();
      case "removeMember":
        return crew ? api.social.removeTeamMember(crew.id, person.id) : Promise.resolve();
    }
  });
  if (done === undefined) return false;
  announce(
    {
      removeFriend: t("social.actions.removed", { name: person.name }),
      block: t("social.actions.blocked", { name: person.name }),
      makeFounder: t("teams.member.madeFounder", { name: person.name }),
      removeMember: t("teams.member.removed", { name: person.name }),
    }[question],
  );
  return true;
}

/** What a question says: its title, its one sentence, its button. */
export function questionText(
  { question, person, crew }: Asked,
  t: TFunction,
  roles: boolean,
): { title: string; sentence: string; action: string; danger: boolean } {
  switch (question) {
    case "removeFriend":
      return {
        title: t("social.actions.remove"),
        sentence: t("social.actions.removeAsk", { name: person.name }),
        action: t("social.actions.remove"),
        danger: true,
      };
    case "block":
      return {
        title: t("social.actions.block"),
        sentence: t("social.actions.blockAsk"),
        action: t("social.actions.block"),
        danger: true,
      };
    case "makeFounder":
      return {
        title: t("teams.member.makeFounder"),
        sentence:
          crew && afterHandingOn(crew, person.id, roles) === "moderator"
            ? t("teams.member.makeFounderAskModerator", { name: person.name })
            : t("teams.member.makeFounderAsk", { name: person.name }),
        action: t("teams.member.makeFounder"),
        danger: false,
      };
    case "removeMember":
      return {
        title: t("teams.member.remove"),
        sentence: t("teams.member.removeAsk", { name: person.name }),
        action: t("teams.member.remove"),
        danger: true,
      };
  }
}

/** The dialog the person menu asks in, where the row has no card of its own. */
export function PersonAskDialog() {
  const { t } = useTranslation();
  const asked = usePersonAsk((s) => s.asked);
  const roles = useSocialUi((s) => s.view.crew.roles);
  // The dialog keeps its words while it fades out.
  const last = useRef<Asked | null>(null);
  if (asked) last.current = asked;
  const shown = asked ?? last.current;
  const close = () => usePersonAsk.setState({ asked: null });
  if (!shown) return null;
  const text = questionText(shown, t, roles);
  return (
    <Dialog
      open={asked !== null}
      onOpenChange={(open) => {
        if (!open) close();
      }}
      title={text.title}
      description={text.sentence}
      closeLabel={t("common.close")}
      width={420}
      footer={
        <>
          <Button variant="ghost" onClick={close}>
            {t("social.actions.cancel")}
          </Button>
          <Button
            variant={text.danger ? "danger" : "primary"}
            onClick={() => {
              close();
              void answerAboutPerson(shown, t);
            }}
          >
            {text.action}
          </Button>
        </>
      }
    />
  );
}

// --- the rows ---------------------------------------------------------------------------------------

/** The order "Ekibe davet et" picks the crew in when several could take the friend. */
const INVITE_ORDER: readonly InviteState[] = ["invitable", "invited", "full", "member"];

/** What every person's menu reads: the lists, the player's own settings, whether a game runs. */
export interface PersonEnv {
  t: TFunction;
  view: SocialSnapshot;
  cooldown: Record<string, number>;
  favorites: readonly string[];
  notifies: (id: string) => boolean;
  /** The player's game is on projects.gg already ("Katıl" is off). */
  here: boolean;
}

export function usePersonEnv(): PersonEnv {
  const { t } = useTranslation();
  const view = useSocialUi((s) => s.view);
  const cooldown = useSocialUi((s) => s.inviteCooldown);
  const prefs = useSocialPrefs();
  const here = useThere(PROJECTS);
  return {
    t,
    view,
    cooldown,
    favorites: prefs.favorites,
    notifies: (id) => notifiesOnline(prefs, id),
    here,
  };
}

/** The person's menu rows, worded, with their icons and what they do. */
export function personItems(
  person: PersonRef,
  place: PersonPlace,
  env: PersonEnv,
): Array<MenuItem | null> {
  const { t, view, here } = env;
  const ui = useSocialUi.getState();
  const prefs = useSocialPrefs.getState();
  const me = view.me?.id;
  const friend = view.friends.find((f) => f.id === person.id);
  // Their place in the row's crew, else in any of the player's crews.
  const member: TeamMember | undefined =
    place.crew?.members.find((m) => m.id === person.id) ??
    view.crews.flatMap((crew) => crew.members).find((m) => m.id === person.id);
  const blocked = view.blocked.some((b) => b.id === person.id) || Boolean(member?.blocked);
  const presence = friend ?? member;
  const joinable = presence?.state === "playing" && isJoinable(presence.game);
  const request =
    place.request === undefined
      ? undefined
      : typeof place.request === "string"
        ? place.request
        : "join";
  const joinCrew = typeof place.request === "object" ? place.request.join : undefined;
  const discordUrl = friend?.discordUrl ?? member?.discordUrl ?? person.discordUrl;
  const crewmate = view.crews.some((crew) => crew.members.some((m) => m.id === person.id && !m.me));
  const addFriend =
    !friend && crewmate && view.crew.crewFriendRequests
      ? view.outgoing.some((r) => r.id === person.id)
        ? "sent"
        : view.incoming.some((r) => r.id === person.id)
          ? "received"
          : "ready"
      : "hidden";
  const favorite = env.favorites.includes(person.id);
  const notify = env.notifies(person.id);

  // "Ekibe davet et": the crews that let the player invite, the best first.
  const targets = friend && view.teams ? inviteTargets(view.crews, person.id, view.crew) : [];
  const best = [...targets].sort(
    (a, b) => INVITE_ORDER.indexOf(a.state) - INVITE_ORDER.indexOf(b.state),
  )[0];
  const inviteTo = (crew: SocialTeam) => () => {
    void attempt(() => api.social.inviteToTeam(crew.id, person.id)).then((done) => {
      if (done === undefined) return;
      announce(t("teams.invite.sent"));
      toast.success(t("teams.invite.sent"));
    });
  };

  const crew = place.crew;
  const crewMember = crew?.members.find((m) => m.id === person.id);
  const roles = view.crew.roles;
  const plan = personMenu({
    me: person.id === me,
    friend: Boolean(friend),
    blocked,
    request,
    joinable,
    here,
    invite: !canInvite(view.activity, view.me?.privacy.shareServer ?? false)
      ? "hidden"
      : inviteCoolingDown(person.id, env.cooldown)
        ? "waiting"
        : "ready",
    chat: Boolean(view.chat) && Boolean(me) && !place.inConversation,
    discord: Boolean(discordUrl),
    addFriend: request ? "hidden" : addFriend,
    notifyOnline: notify,
    categories: Boolean(place.categories),
    team: best?.state ?? null,
    crew:
      crew && crewMember && !crewMember.me
        ? {
            roles,
            leader: crew.role === "founder",
            moderator: crewMember.moderator,
            moderatorsFull: crew.members.filter((m) => m.moderator).length >= MAX_MODERATORS,
            removable: canRemove(crew, crewMember, roles),
          }
        : undefined,
    // The service takes a report about a friend, a crewmate, or whoever asked the player.
    report:
      Boolean(friend) ||
      crewmate ||
      request === "incoming" ||
      request === "join" ||
      view.incoming.some((r) => r.id === person.id),
  });

  const ask = (question: PersonQuestion) => {
    if (place.ask) place.ask(question);
    else askAboutPerson({ question, person, crew });
  };

  return rowsOf(plan, (row): MenuItem | null => {
    switch (row.action) {
      case "acceptRequest":
        return {
          label: t("social.actions.accept"),
          icon: <Check size={16} />,
          onSelect: () =>
            void attempt(() =>
              joinCrew
                ? api.social.answerTeamRequest(joinCrew.id, person.id, true)
                : api.social.accept(person.id),
            ).then((done) => {
              if (done === undefined) return;
              announce(
                joinCrew
                  ? t("crewWave2.requests.accepted", { name: person.name })
                  : t("social.actions.accepted", { name: person.name }),
              );
            }),
        };
      case "declineRequest":
        return {
          label: t("social.actions.decline"),
          icon: <X size={16} />,
          onSelect: () =>
            void attempt(() =>
              joinCrew
                ? api.social.answerTeamRequest(joinCrew.id, person.id, false)
                : api.social.decline(person.id),
            ),
        };
      case "cancelRequest":
        return {
          label: t("social.actions.cancelRequest"),
          icon: <X size={16} />,
          onSelect: () => void attempt(() => api.social.cancelRequest(person.id)),
        };
      case "join":
        return {
          label: t("social.actions.join"),
          icon: <Play size={16} />,
          disabled: row.disabled,
          onSelect: () => joinOrAsk(PROJECTS),
        };
      case "invite":
        return {
          label: row.disabled ? t("social.actions.invited") : t("social.actions.invite"),
          icon: <PaperPlaneTilt size={16} />,
          disabled: row.disabled,
          onSelect: () => void inviteFriend(person),
        };
      case "chat":
        return {
          label: t("fix2.menu.chat"),
          icon: <ChatCircle size={16} />,
          onSelect: () => {
            if (me) ui.openChat(directConversation(me, person.id), { from: place.from ?? "list" });
          },
        };
      case "message":
        return {
          label: t("social.actions.message"),
          icon: <DiscordLogo size={16} />,
          onSelect: () => {
            if (discordUrl) void openInDiscord({ id: person.id, discordUrl });
          },
        };
      case "addFriend":
        return {
          label: row.disabled ? t("fix2.menu.requestSent") : t("fix2.menu.addFriend"),
          icon: <UserPlus size={16} />,
          disabled: row.disabled,
          checked: row.checked ? true : undefined,
          onSelect: () =>
            void attempt(() => api.social.requestCrewmate(person.id)).then((outcome) => {
              if (outcome === undefined) return;
              const said =
                outcome === "friends"
                  ? t("social.actions.accepted", { name: person.name })
                  : t("fix2.menu.requestSentTo", { name: person.name });
              toast.success(said);
              announce(said);
            }),
        };
      case "acceptFriend":
        return {
          label: t("fix2.menu.acceptFriend"),
          icon: <UserCheck size={16} />,
          onSelect: () =>
            void attempt(() => api.social.accept(person.id)).then((done) => {
              if (done !== undefined) announce(t("social.actions.accepted", { name: person.name }));
            }),
        };
      case "favorite":
        return {
          label: favorite ? t("social.actions.unfavorite") : t("social.actions.favorite"),
          icon: <Star size={16} weight={favorite ? "fill" : "regular"} />,
          onSelect: () => prefs.toggleFavorite(person.id),
        };
      case "notifyOnline":
        return {
          label: t("friendMenus.notifyOnline"),
          icon: <Bell size={16} weight={notify ? "fill" : "regular"} />,
          checked: notify,
          onSelect: () => prefs.setOnlineAlert(person.id, !notify),
        };
      case "category": {
        const categories = place.categories;
        if (!categories) return null;
        return {
          label: t("friendGroups.moveTo"),
          icon: <FolderSimple size={16} />,
          onSelect: () => {},
          items: [
            ...categories.groups.map((g) => ({
              label: g.name,
              checked: categories.current === g.id,
              radio: true,
              onSelect: () => categories.onMove(g.id),
            })),
            {
              label: t("friendGroups.rest"),
              checked: categories.current === REST,
              radio: true,
              onSelect: () => categories.onMove(REST),
            },
            null,
            {
              label: t("friendGroups.addMenu"),
              icon: <FolderSimplePlus size={16} />,
              onSelect: categories.onNew,
            },
          ],
        };
      }
      case "teamInvite":
        if (!best) return null;
        return {
          label: best.state === "invited" ? t("teams.invite.sent") : t("teams.invite.toTeam"),
          icon: <UsersThree size={16} />,
          disabled: row.disabled,
          onSelect: inviteTo(best.crew),
          items:
            view.crew.multi && targets.length > 1
              ? targets.map(({ crew: target, state }) => ({
                  label:
                    state === "full" ? `${target.name} (${t("crews.invite.full")})` : target.name,
                  icon: <CrewMark look={target} size={16} />,
                  disabled: state !== "invitable",
                  checked: state === "invited" || state === "member" ? true : undefined,
                  onSelect: inviteTo(target),
                }))
              : undefined,
        };
      case "makeModerator":
      case "unmakeModerator": {
        const naming = row.action === "makeModerator";
        return {
          label: naming ? t("teams.member.makeModerator") : t("teams.member.unmakeModerator"),
          icon: <ShieldStar size={16} />,
          disabled: row.disabled,
          onSelect: () => {
            if (!crew) return;
            void attempt(() => api.social.setModerator(crew.id, person.id, naming)).then((done) => {
              if (done === undefined) return;
              announce(
                naming
                  ? t("teams.member.madeModerator", { name: person.name })
                  : t("teams.member.unmadeModerator", { name: person.name }),
              );
            });
          },
        };
      }
      case "makeFounder":
        return {
          label: t("teams.member.makeFounder"),
          icon: <Crown size={16} />,
          onSelect: () => ask("makeFounder"),
        };
      case "removeMember":
        return {
          label: t("teams.member.remove"),
          icon: <UserMinus size={16} />,
          onSelect: () => ask("removeMember"),
        };
      case "removeFriend":
        return {
          label: t("social.actions.remove"),
          icon: <UserMinus size={16} />,
          onSelect: () => ask("removeFriend"),
        };
      case "block":
        return {
          label: t("social.actions.block"),
          icon: <Prohibit size={16} />,
          danger: true,
          onSelect: () => ask("block"),
        };
      case "unblock":
        return {
          label: t("social.actions.unblock"),
          icon: <Prohibit size={16} />,
          onSelect: () =>
            void attempt(() => api.social.unblock(person.id)).then((done) => {
              if (done !== undefined) {
                announce(t("social.actions.unblocked", { name: person.name }));
              }
            }),
        };
      case "report":
        return {
          label: t("reports.action"),
          icon: <Flag size={16} />,
          onSelect: () =>
            ui.openReport({
              target: { kind: "player", id: person.id },
              name: person.name,
              person: {
                name: person.name,
                avatar: person.avatar ?? friend?.avatar ?? member?.avatar ?? "discord",
                avatarUrl: person.avatarUrl ?? friend?.avatarUrl ?? member?.avatarUrl,
              },
            }),
        };
    }
  });
}

/** One person's menu rows. */
export function usePersonItems(person: PersonRef, place: PersonPlace = {}): Array<MenuItem | null> {
  return personItems(person, place, usePersonEnv());
}

/**
 * Lines that stand for several people (Home's lines, a "Birlikte oyna" invite): one submenu per
 * person, each with their whole menu (the line puts its own actions first).
 */
export function usePeopleItems(people: readonly PersonRef[], place: PersonPlace = {}): MenuItem[] {
  const env = usePersonEnv();
  return people
    .map((person) => ({ person, items: personItems(person, place, env) }))
    .filter(({ items }) => items.length > 0)
    .map(({ person, items }) => ({
      label: person.name,
      onSelect: () => {},
      items,
    }));
}

/** Someone's right-click menu (Shift+F10 and the menu key from a focused row, too). */
export function PersonContextMenu({
  person,
  place,
  children,
}: {
  person: PersonRef;
  place?: PersonPlace;
  /** One element: it gets the right click's handlers. */
  children: ReactElement;
}) {
  const items = usePersonItems(person, place);
  return (
    <ContextMenu items={items} disabled={!items.length}>
      {children}
    </ContextMenu>
  );
}

/** Someone's "…" (the same rows as the right click). */
export function PersonMoreButton({
  person,
  place,
  className,
}: {
  person: PersonRef;
  place?: PersonPlace;
  className?: string;
}) {
  const { t } = useTranslation();
  const items = usePersonItems(person, place);
  if (!items.length) return null;
  return (
    <Menu
      items={items}
      trigger={
        <IconButton
          label={t("friendMenus.options", { name: person.name })}
          icon={<DotsThree size={18} weight="bold" />}
          size="sm"
          className={className}
        />
      }
    />
  );
}
