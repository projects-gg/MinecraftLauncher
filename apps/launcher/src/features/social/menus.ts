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
    null,
    { action: "status" },
  ]);
}

// --- a friend's row ------------------------------------------------------------------------------

export type FriendAction =
  | "join"
  | "invite"
  | "message"
  | "favorite"
  | "notifyOnline"
  | "category"
  | "teamInvite"
  | "remove"
  | "block";

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
  /** The player's team and them: null without a team (or teams off). */
  team: "invitable" | "invited" | "member" | null;
}

export function friendMenu(input: FriendInput): Plan<FriendAction> {
  return tidy<FriendAction>([
    input.joinable ? { action: "join", disabled: input.gameBusy } : null,
    input.invite === "hidden" ? null : { action: "invite", disabled: input.invite === "waiting" },
    { action: "message" },
    null,
    { action: "favorite" },
    { action: "notifyOnline", checked: input.notifyOnline },
    input.categories ? { action: "category" } : null,
    input.team && input.team !== "member"
      ? { action: "teamInvite", disabled: input.team === "invited" }
      : null,
    null,
    { action: "remove" },
    { action: "block" },
  ]);
}

// --- a teammate's row --------------------------------------------------------------------------------

export type MemberAction =
  | "join"
  | "message"
  | "makeModerator"
  | "unmakeModerator"
  | "makeFounder"
  | "remove"
  | "block"
  | "unblock";

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

export type TeamAction = "open" | "toggle" | "call" | "copyCode" | "invite" | "edit" | Leaving;

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
}

export function teamMenu({
  team,
  friendsOutside,
  call = "hidden",
  canInvite = true,
  canEdit,
}: TeamInput): Plan<TeamAction> {
  const full = team.members.length >= team.limit;
  return tidy<TeamAction>([
    { action: "open" },
    { action: "toggle" },
    null,
    ...only<TeamAction>(
      call !== "hidden" && { action: "call", disabled: call === "waiting" },
      { action: "copyCode" },
      canInvite && { action: "invite", disabled: full || friendsOutside === 0 },
      (canEdit ?? team.role === "founder") && { action: "edit" },
    ),
    null,
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
