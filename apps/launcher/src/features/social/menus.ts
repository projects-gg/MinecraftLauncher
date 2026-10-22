// What the friends panel's menus offer. A right click, Shift+F10 or the menu key and the "…"
// buttons show the same rows, so these rules decide them once (tested in menus.test.ts); the
// components add the words, icons and actions (person-menu.tsx for people). A row is an action,
// `null` a separator. Team and category rows only appear while the service offers teams and
// categories (an older service does not announce them).
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

// --- a person (FIX2: the same menu wherever a player shows) --------------------------------------

/**
 * Everything the player may do about someone, in one order everywhere: a friend in the list, a
 * crewmate in a crew, someone in a plan's or a call's answers, a message's sender, a request. The
 * groups: the request the row is about (its answers lead), being together (join, play together,
 * message, Discord, add as a friend), the friend list (favourite, online notice, category, crew
 * invite), the crew the row shows them in (moderator, leader, remove), and the relationship
 * (unfriend, block, report). A row shows only where the rules and the player's rights allow it.
 */
export type PersonAction =
  | "acceptRequest"
  | "declineRequest"
  | "cancelRequest"
  | "join"
  | "invite"
  | "chat"
  | "message"
  | "addFriend"
  | "acceptFriend"
  | "favorite"
  | "notifyOnline"
  | "category"
  | "teamInvite"
  | "makeModerator"
  | "unmakeModerator"
  | "makeFounder"
  | "removeMember"
  | "removeFriend"
  | "block"
  | "unblock"
  | "report";

/** Their place in the crew a row shows them in, and what the player may do about it. */
export interface PersonCrew {
  /** The service has moderators (`teamFeatures: roles`). */
  roles: boolean;
  /** The player leads the crew: names moderators and hands the leadership on. */
  leader: boolean;
  /** They are one of its moderators. */
  moderator: boolean;
  /** The crew has its 4 moderators: no one more can be named. */
  moderatorsFull: boolean;
  /** The player may remove them (the leader anyone, a moderator plain members where allowed). */
  removable: boolean;
}

export interface PersonInput {
  /** The player themself: no menu at all. */
  me?: boolean;
  /** On the player's friend list. */
  friend: boolean;
  /** The player blocked them. */
  blocked: boolean;
  /**
   * The row is a request (its answers lead): a friend request from them (`incoming`), the player's
   * to them (`outgoing`), or theirs to join one of the player's crews (`join`).
   */
  request?: "incoming" | "outgoing" | "join";
  /** They are on projects.gg ("Katıl"). */
  joinable?: boolean;
  /**
   * The player's own game is there already: "Katıl" is off. (FIX2: a game open elsewhere or
   * opening goes there after one question, "Bağlan".)
   */
  here?: boolean;
  /** "Birlikte oyna" (friends): may be sent, waits for its five minutes, or is not offered. */
  invite?: "ready" | "waiting" | "hidden";
  /** Chat is on and this is not their conversation on screen (friends): "Mesaj gönder". */
  chat?: boolean;
  /** Their Discord profile is known (friends, crewmates): "Discord'dan mesaj at". */
  discord?: boolean;
  /**
   * "Arkadaş ekle" (a crewmate who is not a friend, the service lets crewmates add each other):
   * may be sent, was sent and waits, or they asked first ("İsteği kabul et").
   */
  addFriend?: "ready" | "sent" | "received" | "hidden";
  /** "Çevrimiçi olunca bildir" is on for them (friends). */
  notifyOnline?: boolean;
  /** The friend list's categories are here (their "Kategoriye taşı"). */
  categories?: boolean;
  /**
   * The player's crews and them (friends): null without one to invite to. With several crews the
   * best of them (one to invite to beats invited beats in all of them); the row opens a submenu.
   */
  team?: "invitable" | "invited" | "member" | "full" | null;
  /** The crew the row shows them in. */
  crew?: PersonCrew;
  /** "Rapor et": the service takes a report about them from here (MOD1). */
  report?: boolean;
}

/** The rows of one group that apply (a `null` in a plan is a separator, never a missing row). */
function only<A extends string>(...rows: Array<Row<A> | false>): Array<Row<A>> {
  return rows.filter((row): row is Row<A> => row !== false);
}

export function personMenu(input: PersonInput): Plan<PersonAction> {
  if (input.me) return [];
  const { friend, blocked, request, crew } = input;
  const together = !blocked;
  const addFriend = input.addFriend ?? "hidden";
  return tidy<PersonAction>([
    ...only<PersonAction>(
      (request === "incoming" || request === "join") && { action: "acceptRequest" },
      (request === "incoming" || request === "join") && { action: "declineRequest" },
      request === "outgoing" && { action: "cancelRequest" },
    ),
    null,
    ...only<PersonAction>(
      together && Boolean(input.joinable) && { action: "join", disabled: Boolean(input.here) },
      together &&
        friend &&
        (input.invite ?? "hidden") !== "hidden" && {
          action: "invite",
          disabled: input.invite === "waiting",
        },
      together && friend && Boolean(input.chat) && { action: "chat" },
      Boolean(input.discord) && { action: "message" },
      // On a request's own row its answers say it all.
      together &&
        !friend &&
        !request &&
        addFriend !== "hidden" &&
        (addFriend === "received"
          ? { action: "acceptFriend" }
          : { action: "addFriend", disabled: addFriend === "sent", checked: addFriend === "sent" }),
    ),
    null,
    ...only<PersonAction>(
      friend && { action: "favorite" },
      friend && { action: "notifyOnline", checked: Boolean(input.notifyOnline) },
      friend && Boolean(input.categories) && { action: "category" },
      friend &&
        Boolean(input.team) &&
        input.team !== "member" && {
          action: "teamInvite",
          disabled: input.team === "invited" || input.team === "full",
        },
    ),
    null,
    ...(crew
      ? only<PersonAction>(
          crew.leader &&
            crew.roles &&
            (crew.moderator
              ? { action: "unmakeModerator" }
              : { action: "makeModerator", disabled: crew.moderatorsFull }),
          crew.leader && { action: "makeFounder" },
          crew.removable && { action: "removeMember" },
        )
      : []),
    null,
    ...only<PersonAction>(
      friend && { action: "removeFriend" },
      { action: blocked ? "unblock" : "block" },
      Boolean(input.report) && { action: "report" },
    ),
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

// --- a crew plan's card (TEAM5, its note FIX2) ---------------------------------------------------------

export type PlanAction =
  | "yes"
  | "maybe"
  | "no"
  | "join"
  | "writeNote"
  | "editNote"
  | "clearNote"
  | "report"
  | "cancel";

export interface PlanInput {
  /** The player's answer so far (shown with a check). */
  answer?: "yes" | "maybe" | "no";
  /** It is under way and the player has not said "Gelemem": "Katıl". */
  joinable: boolean;
  /** The player's game is on projects.gg already: "Katıl" is off. */
  here: boolean;
  /** The service keeps notes (`planNotes`). */
  notes: boolean;
  /** It has a note. */
  hasNote: boolean;
  /** The player may write its note: they made it and it has not started. */
  canWrite: boolean;
  /** The player may clear its note: they made it, or lead or moderate the crew. */
  canClear: boolean;
  /** Someone else's plan: "Rapor et" reports its maker, the plan as evidence. */
  report: boolean;
  /** "Planı sil": its maker, the crew's leader and moderators. */
  cancel: boolean;
}

/** A plan's "…" and right click: the answers, "Katıl", its note, then report and delete. */
export function planMenu(input: PlanInput): Plan<PlanAction> {
  return tidy<PlanAction>([
    { action: "yes", checked: input.answer === "yes" },
    { action: "maybe", checked: input.answer === "maybe" },
    { action: "no", checked: input.answer === "no" },
    null,
    input.joinable ? { action: "join", disabled: input.here } : null,
    null,
    input.notes && input.canWrite ? { action: input.hasNote ? "editNote" : "writeNote" } : null,
    input.notes && input.hasNote && input.canClear ? { action: "clearNote" } : null,
    null,
    input.report ? { action: "report" } : null,
    input.cancel ? { action: "cancel" } : null,
  ]);
}

// --- a crew call's row --------------------------------------------------------------------------

export type CallAction = "yes" | "soon" | "no" | "join" | "open";

export interface CallInput {
  /** The caller plays on projects.gg or a shared server ("Katıl"). */
  joinable: boolean;
  /** The player's game is on that server already: "Katıl" is off. */
  here: boolean;
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
    ...only<CallAction>(input.joinable && { action: "join", disabled: input.here }, {
      action: "open",
    }),
  ]);
}
