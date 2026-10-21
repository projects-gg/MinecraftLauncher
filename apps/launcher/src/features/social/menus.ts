// What the friends panel's menus offer. A right click, Shift+F10 or the menu key and the "…"
// buttons show the same rows, so these rules decide them once (tested in menus.test.ts); the
// components add the words, icons and actions. A row is an action, `null` a separator. Team and
// category rows only appear while the service offers teams and categories (an older service
// does not announce them).
import type { SocialTeam } from "@/lib/api/social-types";
import { MAX_GROUPS } from "./groups";

export interface Row<A extends string> {
  action: A;
  disabled?: boolean;
  /** A choice that is on or off (shown with a check). */
  checked?: boolean;
}

export type Plan<A extends string> = Array<Row<A> | null>;

/** How the player leaves their team: a member leaves; a founder disbands it, or closes it alone. */
export type Leaving = "leave" | "close" | "disband";

export function leavingOf(team: Pick<SocialTeam, "role" | "members">): Leaving {
  if (team.role !== "founder") return "leave";
  return team.members.length <= 1 ? "close" : "disband";
}

/** A plan as menu rows: `row` words each action; separators stay separators. */
export function rowsOf<A extends string, T>(
  plan: Plan<A>,
  row: (row: Row<A>) => T,
): Array<T | null> {
  return plan.map((entry) => (entry ? row(entry) : null));
}

/** Separators only between groups that have rows (none first, last or twice). */
function tidy<A extends string>(plan: Plan<A>): Plan<A> {
  const out: Plan<A> = [];
  for (const row of plan) {
    if (row === null && (out.length === 0 || out[out.length - 1] === null)) continue;
    out.push(row);
  }
  if (out[out.length - 1] === null) out.pop();
  return out;
}

// --- the panel (its empty space, its header) ----------------------------------------------------

export type PanelAction =
  | "addFriend"
  | "copyCode"
  | "newCategory"
  | "createTeam"
  | "openTeam"
  | "callCrews"
  | "status";

export interface PanelInput {
  /** The service keeps categories (it announced them). */
  categories: boolean;
  /** How many the player has (the service keeps 20). */
  categoryCount: number;
  /** The player has friends (a category holds friends; its name field sits under them). */
  hasFriends: boolean;
  /** Teams are on and the service offers them. */
  teams: boolean;
  /** The player is in a team: "Takımı aç" instead of "Takım kur". */
  inTeam: boolean;
  /** The player's friend code is known. */
  hasCode: boolean;
  /** Several crews (TEAM4): one more may be founded or joined ("Ekip kur" beside "Ekipleri aç"). */
  canAddCrew?: boolean;
  /** Several crews the player may call: "Ekiplerini çağır…" (the picker), waiting or not. */
  callCrews?: "ready" | "waiting" | "hidden";
}

export function panelMenu(input: PanelInput): Plan<PanelAction> {
  return tidy<PanelAction>([
    { action: "addFriend" },
    input.hasCode ? { action: "copyCode" } : null,
    null,
    input.categories && input.hasFriends
      ? { action: "newCategory", disabled: input.categoryCount >= MAX_GROUPS }
      : null,
    input.teams ? { action: input.inTeam ? "openTeam" : "createTeam" } : null,
    input.teams && input.inTeam && input.canAddCrew ? { action: "createTeam" } : null,
    input.teams && input.inTeam && input.callCrews && input.callCrews !== "hidden"
      ? { action: "callCrews", disabled: input.callCrews === "waiting" }
      : null,
    null,
    { action: "status" },
  ]);
}

// --- a friend's row ------------------------------------------------------------------------------

export type FriendAction =
  | "join"
  | "invite"
  | "chat"
  | "message"
  | "favorite"
  | "notifyOnline"
  | "category"
  | "teamInvite"
  | "remove"
  | "block"
  | "report";

export interface FriendInput {
  /** They are on projects.gg ("Katıl"). */
  joinable: boolean;
  /** A game is being prepared or running ("Katıl" waits). */
  gameBusy: boolean;
  /** "Birlikte oyna": may be sent, waits for its five minutes, or is not offered. */
  invite: "ready" | "waiting" | "hidden";
  /** "Çevrimiçi olunca bildir" is on for them. */
  notifyOnline: boolean;
  /** The service keeps categories. */
  categories: boolean;
  /**
   * The player's team and them: null without a team (or teams off). With several crews (TEAM4) it
   * is the best of them (one to invite to beats invited beats in all of them), and the row opens a
   * submenu of the crews.
   */
  team: "invitable" | "invited" | "member" | "full" | null;
  /** Chat is on (CHAT1): "Mesaj" opens the conversation. */
  chat?: boolean;
  /** "Rapor et" (MOD1). */
  report?: boolean;
}

export function friendMenu(input: FriendInput): Plan<FriendAction> {
  return tidy<FriendAction>([
    input.joinable ? { action: "join", disabled: input.gameBusy } : null,
    input.invite === "hidden" ? null : { action: "invite", disabled: input.invite === "waiting" },
    // A `null` here would be a separator: no row while chat is off.
    ...only<FriendAction>(Boolean(input.chat) && { action: "chat" }),
    { action: "message" },
    null,
    { action: "favorite" },
    { action: "notifyOnline", checked: input.notifyOnline },
    input.categories ? { action: "category" } : null,
    input.team && input.team !== "member"
      ? { action: "teamInvite", disabled: input.team === "invited" || input.team === "full" }
      : null,
    null,
    { action: "remove" },
    { action: "block" },
    input.report ? { action: "report" } : null,
  ]);
}

// --- a teammate's row --------------------------------------------------------------------------------

export type MemberAction =
  | "join"
  | "chat"
  | "message"
  | "makeModerator"
  | "unmakeModerator"
  | "makeFounder"
  | "remove"
  | "block"
  | "unblock"
  | "report";

export interface MemberInput {
  /** They are on projects.gg ("Katıl"). */
  joinable: boolean;
  gameBusy: boolean;
  /** The player leads the crew: hands the role on, names moderators, removes. */
  founder: boolean;
  /** The player blocked them (they see each other offline). */
  blocked: boolean;
  /** The service has moderators (`teamFeatures: roles`). */
  roles?: boolean;
  /** They are a moderator. */
  moderator?: boolean;
  /** The crew is at its moderators' limit (4): no one more can be named. */
  moderatorsFull?: boolean;
  /** The player may remove them (the leader anyone, a moderator plain members). */
  removable?: boolean;
  /** Chat is on and they are a friend (CHAT1): "Mesaj" opens the conversation. */
  chat?: boolean;
  /** "Rapor et" (MOD1). */
  report?: boolean;
}

/** The rows of one group that apply (a `null` in a plan is a separator, never a missing row). */
function only<A extends string>(...rows: Array<Row<A> | false>): Array<Row<A>> {
  return rows.filter((row): row is Row<A> => row !== false);
}

export function memberMenu(input: MemberInput): Plan<MemberAction> {
  const naming = input.founder && input.roles;
  return tidy<MemberAction>([
    ...only<MemberAction>(
      input.joinable && !input.blocked && { action: "join", disabled: input.gameBusy },
      Boolean(input.chat) && !input.blocked && { action: "chat" },
      { action: "message" },
    ),
    null,
    ...only<MemberAction>(
      Boolean(naming) &&
        (input.moderator
          ? { action: "unmakeModerator" }
          : { action: "makeModerator", disabled: Boolean(input.moderatorsFull) }),
      input.founder && { action: "makeFounder" },
      (input.removable ?? input.founder) && { action: "remove" },
    ),
    null,
    { action: input.blocked ? "unblock" : "block" },
    input.report ? { action: "report" } : null,
  ]);
}

// --- a category's header ---------------------------------------------------------------------------

export type CategoryAction = "toggle" | "rename" | "moveUp" | "moveDown" | "newCategory" | "remove";

export interface CategoryInput {
  /** One of the player's own (the implicit "Arkadaşlar" is neither renamed, moved nor deleted). */
  custom: boolean;
  canMoveUp: boolean;
  canMoveDown: boolean;
  /** Another category may be made (fewer than 20). */
  canAdd: boolean;
}

export function categoryMenu(input: CategoryInput): Plan<CategoryAction> {
  return tidy<CategoryAction>([
    { action: "toggle" },
    null,
    input.custom ? { action: "rename" } : null,
    input.custom ? { action: "moveUp", disabled: !input.canMoveUp } : null,
    input.custom ? { action: "moveDown", disabled: !input.canMoveDown } : null,
    { action: "newCategory", disabled: !input.canAdd },
    null,
    input.custom ? { action: "remove" } : null,
  ]);
}

// --- the team's header ------------------------------------------------------------------------------

export type TeamAction =
  | "open"
  | "toggle"
  | "call"
  | "copyCode"
  | "invite"
  | "edit"
  | "makeMain"
  | "moveUp"
  | "moveDown"
  | "mute"
  | "unmute"
  | "chat"
  | "report"
  | Leaving;

export interface TeamInput {
  team: Pick<SocialTeam, "role" | "members" | "invited" | "limit">;
  /** Friends who are not in the team yet (invited or not). */
  friendsOutside: number;
  /**
   * "Ekibi çağır": offered (and waiting out its five minutes, or not) or not at all (no calls on
   * the service, no right to call, or a call is on already). Older callers leave it out.
   */
  call?: "ready" | "waiting" | "hidden";
  /** The player may invite (the crew's permissions; everyone on a service without roles). */
  canInvite?: boolean;
  /** The player may edit the crew (the leader; a moderator the look where the crew lets them). */
  canEdit?: boolean;
  /**
   * "Sessize al" (its submenu says how long) or, while muted, "Sesi aç": only where the service
   * keeps the crew's notification settings. Older callers leave it out.
   */
  mute?: "muted" | "unmuted" | "hidden";
  /**
   * Several crews (TEAM4): where this one stands in the player's order. The first is the primary
   * crew; the others offer "Ana ekip yap". Left out with one crew.
   */
  order?: { first: boolean; last: boolean };
  /** Chat is on (CHAT1): "Sohbet" opens the crew's chat. */
  chat?: boolean;
  /** "Ekibi rapor et" (MOD1). */
  report?: boolean;
}

export function teamMenu({
  team,
  friendsOutside,
  call = "hidden",
  canInvite = true,
  canEdit,
  mute = "hidden",
  order,
  chat = false,
  report = false,
}: TeamInput): Plan<TeamAction> {
  const full = team.members.length >= team.limit;
  return tidy<TeamAction>([
    { action: "open" },
    { action: "toggle" },
    null,
    ...only<TeamAction>(
      call !== "hidden" && { action: "call", disabled: call === "waiting" },
      chat && { action: "chat" },
      { action: "copyCode" },
      canInvite && { action: "invite", disabled: full || friendsOutside === 0 },
      (canEdit ?? team.role === "founder") && { action: "edit" },
    ),
    null,
    ...(order
      ? only<TeamAction>(
          !order.first && { action: "makeMain" },
          { action: "moveUp", disabled: order.first },
          { action: "moveDown", disabled: order.last },
        )
      : []),
    null,
    mute === "hidden" ? null : { action: mute === "muted" ? "unmute" : "mute" },
    null,
    report ? { action: "report" } : null,
    { action: leavingOf(team) },
  ]);
}

// --- a crew call's row --------------------------------------------------------------------------

export type CallAction = "yes" | "soon" | "no" | "join" | "open";

export interface CallInput {
  /** The caller plays on projects.gg ("Katıl"). */
  joinable: boolean;
  gameBusy: boolean;
  /** The player's answer so far (shown with a check). */
  answer?: "yes" | "soon" | "no";
}

/** A teammate's call in the list: the three answers, joining the caller, the crew page. */
export function callMenu(input: CallInput): Plan<CallAction> {
  return tidy<CallAction>([
    { action: "yes", checked: input.answer === "yes" },
    { action: "soon", checked: input.answer === "soon" },
    { action: "no", checked: input.answer === "no" },
    null,
    ...only<CallAction>(input.joinable && { action: "join", disabled: input.gameBusy }, {
      action: "open",
    }),
  ]);
}
