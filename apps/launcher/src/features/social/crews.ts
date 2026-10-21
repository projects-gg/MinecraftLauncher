// Several crews (TEAM4) and calls from a server (CALL1), pure rules (crews.test.ts): which crew
// the crew page shows, whether one more may be founded or joined, the order and its moves, which
// crew Home names, where a call takes the player, how a call is said, which crews may be called
// or a friend invited to, and when Home offers a call. With one crew per player (an older
// service) all of it reads as before: one crew, no order.
import type { TFunction } from "i18next";
import type {
  CallPlace,
  CrewCall,
  CrewFeatures,
  SocialActivity,
  SocialTeam,
} from "@/lib/api/social-types";
import { callWaitUntil, crewsOfCall, myCall } from "./calls";
import { crewmatesOnProjects, isProjectsHost } from "./model";
import { crewRights } from "./teams";

/** The crew page's add view ("Ekip kur ya da katıl"), in place of a crew. */
export const NEW_CREW_TAB = "new";

/**
 * What the crew page shows: the crew the player picked, else the first (the primary crew); the
 * add view when picked (several crews) or when there is no crew at all.
 */
export function shownCrew(
  crews: readonly SocialTeam[],
  tab: string | null,
): SocialTeam | typeof NEW_CREW_TAB {
  if (tab === NEW_CREW_TAB) return NEW_CREW_TAB;
  return crews.find((crew) => crew.id === tab) ?? crews[0] ?? NEW_CREW_TAB;
}

/** One more crew may be founded or joined: below the limit (several crews), else with none. */
export function canAddCrew(crews: readonly unknown[], limit: number, multi: boolean): boolean {
  return multi ? crews.length < Math.max(1, limit) : crews.length === 0;
}

/** The order with `id` moved one place (`step`), or null when it cannot go further. */
export function moveCrew(order: readonly string[], id: string, step: -1 | 1): string[] | null {
  const at = order.indexOf(id);
  const to = at + step;
  if (at < 0 || to < 0 || to >= order.length) return null;
  const next = [...order];
  next.splice(at, 1);
  next.splice(to, 0, id);
  return next;
}

/** The order with `id` first (the primary crew), or null when it is first already. */
export function crewFirst(order: readonly string[], id: string): string[] | null {
  const at = order.indexOf(id);
  if (at <= 0) return null;
  return [id, ...order.filter((other) => other !== id)];
}

/** The crew Home's crew line shows: the one with the most crewmates on Projects (the player's
 * order breaks ties). */
export function busiestCrew(crews: readonly SocialTeam[]): SocialTeam | undefined {
  let best: SocialTeam | undefined;
  let most = 0;
  for (const crew of crews) {
    const here = crewmatesOnProjects([crew]).length;
    if (here > most) {
      best = crew;
      most = here;
    }
  }
  return best;
}

/** Crews the player may call (the service offers calls and the crew lets the player's role). */
export function callableCrews(
  crews: readonly SocialTeam[],
  features: Pick<CrewFeatures, "calls" | "roles">,
): SocialTeam[] {
  return crews.filter((crew) => crewRights(crew, features).call);
}

/** Crews where someone else's call is on now: no other call goes to them until it ends. */
export function crewsWithCall(
  calls: readonly CrewCall[],
  crews: readonly SocialTeam[],
  nowMs: number,
): Set<string> {
  const busy = new Set<string>();
  for (const call of calls) {
    if (call.mine || call.endsAt * 1000 <= nowMs) continue;
    for (const crew of crewsOfCall(call, crews)) busy.add(crew.id);
  }
  return busy;
}

/** The caller as the crews show them: found in any crew, else what the call says. */
export function callerIn(call: CrewCall, crews: readonly Pick<SocialTeam, "members">[]) {
  for (const crew of crews) {
    const member = crew.members.find((m) => m.id === call.from);
    if (member) {
      return {
        id: call.from,
        name: member.name,
        avatar: member.avatar,
        avatarUrl: member.avatarUrl,
      };
    }
  }
  return { id: call.from, name: call.name ?? "", avatar: call.avatar, avatarUrl: call.avatarUrl };
}

/**
 * "Ayşe ekibi çağırıyor"; with several crews it names them: "Ayşe, Gece Kartalları ekibini
 * çağırıyor", "…, Gece Kartalları ve PvP ekiplerini çağırıyor", "Ayşe 3 ekibi çağırıyor". Inside
 * a crew (its page) the crew is clear: `inCrew` keeps the short sentence.
 */
export function callSentenceOf(
  call: CrewCall,
  crews: readonly SocialTeam[],
  t: TFunction,
  inCrew = false,
): string {
  const name = callerIn(call, crews).name || t("teams.call.someone");
  const named = crewsOfCall(call, crews);
  const [first, second] = named;
  if (inCrew || crews.length < 2 || !first) return t("teams.call.theirs", { name });
  if (!second) return t("crews.call.theirsOne", { name, crew: first.name });
  if (named.length === 2) {
    return t("crews.call.theirsTwo", { name, first: first.name, second: second.name });
  }
  return t("crews.call.theirsMany", { name, count: named.length });
}

/** Where a call takes the player: projects.gg, another server (with its address), or nowhere
 * ("Hazır mısın?", a call from before CALL1). */
export type CallWhere = { kind: "projects" } | { kind: "server"; host: string } | { kind: "ready" };

export function callWhere(call: Pick<CrewCall, "kind" | "server">): CallWhere {
  if (call.kind !== "join") return { kind: "ready" };
  const server = call.server;
  if (!server || server.where === "projects" || isProjectsHost(server.host)) {
    return { kind: "projects" };
  }
  return server.host ? { kind: "server", host: server.host } : { kind: "ready" };
}

/** The where line under a call: "Projects'te", "mc.example.com sunucusunda", "Hazır mısın?". */
export function callWhereLine(where: CallWhere, t: TFunction): string {
  switch (where.kind) {
    case "projects":
      return t("social.presence.projects");
    case "server":
      return t("social.presence.otherHost", { host: where.host });
    case "ready":
      return t("teams.call.ready");
  }
}

/** Why "Ekibi çağır" cannot go from here (CALL1). */
export type CallRefusal = Exclude<CallPlace, "ok">;

/**
 * The reason line under "Ekibi çağır" after a press: the place now, while it still says no; the
 * service's own refusal while the place has not changed since (a moment's disagreement).
 */
export function refusalNow(
  asked: { reason: CallRefusal; place: CallPlace } | null,
  place: CallPlace,
): CallRefusal | null {
  if (!asked) return null;
  if (place !== "ok") return place;
  return asked.place === "ok" ? asked.reason : null;
}

/** A friend and one of the player's crews: invite them, invited already, in it, or no room. */
export type InviteState = "invitable" | "invited" | "member" | "full";

/** The crews a friend may be invited to from here (the crew lets the player invite), with how. */
export function inviteTargets(
  crews: readonly SocialTeam[],
  friendId: string,
  features: Pick<CrewFeatures, "calls" | "roles">,
): Array<{ crew: SocialTeam; state: InviteState }> {
  return crews
    .filter((crew) => crewRights(crew, features).invite)
    .map((crew) => ({
      crew,
      state: crew.members.some((m) => m.id === friendId)
        ? "member"
        : crew.invited.includes(friendId)
          ? "invited"
          : crew.members.length >= crew.limit
            ? "full"
            : "invitable",
    }));
}

/** The server Home's call line was closed on: projects.gg or another one's address. */
export function callLineKey(activity: SocialActivity): string | null {
  if (activity.state !== "playing") return null;
  return activity.host ?? "projects";
}

/**
 * Home's "Ekibini yanına çağır" (CALL1): in game where a call may start, with a crew to call that
 * has no call on, no call of the player's own on and the five minutes since the last one gone;
 * closed with × it stays away on that server.
 */
export function callLineShown(input: {
  place: CallPlace;
  activity: SocialActivity;
  /** Where the line was closed (`callLineKey`), if it was. */
  closedOn: string | null;
  crews: readonly SocialTeam[];
  features: Pick<CrewFeatures, "calls" | "roles">;
  calls: readonly CrewCall[];
  refusedUntil: number;
  lastCallAt: number | null;
  now: number;
}): boolean {
  const key = callLineKey(input.activity);
  if (input.place !== "ok" || !key || key === input.closedOn) return false;
  if (myCall(input.calls, input.now)) return false;
  if (callWaitUntil(input.calls, input.refusedUntil, input.lastCallAt) > input.now) return false;
  const busy = crewsWithCall(input.calls, input.crews, input.now);
  return callableCrews(input.crews, input.features).some((crew) => !busy.has(crew.id));
}
