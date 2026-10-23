// "Dünyanı arkadaşlarına aç" (HOST1): the pure rules of its interface (tested in rules.test.ts).
// Who the invite window lists and how its choices add up, what a world row, a "Katıl" and the
// person menu show, which words a failure gets. No React and no backend here.
import { fold } from "@pl/ui";
import type {
  HostPlace,
  HostSession,
  HostView,
  InviteRefusal,
  JoinFailure,
  JoinStage,
  OpenFailure,
  OpeningStep,
  WorldGameMode,
  WorldGuest,
  WorldInvite,
} from "@/lib/api/host-types";
import type { WorldMode } from "@/lib/api/library-types";
import type { Friend, SocialTeam } from "@/lib/api/social-types";
import type { LaunchPhase } from "@/stores/launch";

const collator = new Intl.Collator("tr", { sensitivity: "base" });

// --- counts ------------------------------------------------------------------------------------

/** "2/8": who is in the world with the host counted, of how many may be (PROTOCOL §6.4). */
export function peopleOf(guestsIn: number, max: number): { count: number; max: number } {
  return { count: guestsIn + 1, max: max + 1 };
}

/** A session's "3/8": the guests in the world now. */
export function sessionPeople(session: Pick<HostSession, "guests" | "max">) {
  return peopleOf(session.guests.filter((g) => g.state === "in").length, session.max);
}

/** An invite's "2/8" (the host's own count, carried by the invite). */
export function invitePeople(invite: Pick<WorldInvite, "guests" | "max">) {
  return peopleOf(invite.guests, invite.max);
}

/** Guests in the session that hold a place: picked, invited, on their way or in. */
export function heldPlaces(session: Pick<HostSession, "guests"> | null): number {
  return session ? session.guests.length : 0;
}

const GUEST_ORDER: Record<WorldGuest["state"], number> = {
  in: 0,
  joining: 1,
  invited: 2,
  pending: 3,
};

/** The sheet's list: in the world first, then on their way, invited, waiting; else as picked. */
export function guestOrder(guests: readonly WorldGuest[]): WorldGuest[] {
  return guests
    .map((guest, index) => ({ guest, index }))
    .sort((a, b) => GUEST_ORDER[a.guest.state] - GUEST_ORDER[b.guest.state] || a.index - b.index)
    .map(({ guest }) => guest);
}

// --- the invite window ---------------------------------------------------------------------------

/** The mode a world opens in by default: its own (hardcore and spectator worlds: survival). */
export function defaultMode(world: { gameMode: WorldMode; hardcore: boolean }): WorldGameMode {
  if (world.hardcore) return "survival";
  return world.gameMode === "creative" || world.gameMode === "adventure"
    ? world.gameMode
    : "survival";
}

export interface PickerGroup {
  key: string;
  /** The crew the people are listed under; none for the friends outside the crews. */
  crew?: SocialTeam;
  people: Friend[];
}

export interface PickerInput {
  friends: readonly Friend[];
  /** The player's crews, in their order. */
  crews: readonly SocialTeam[];
  /** Blocked people never show. */
  blocked: ReadonlySet<string>;
  /** Already in the session (the window's "Davet et" while the world is open): listed, fixed. */
  taken: ReadonlySet<string>;
  query: string;
}

const PRESENCE_ORDER: Record<Friend["state"], number> = { playing: 0, launcher: 1, offline: 2 };

function byPresence(a: Friend, b: Friend): number {
  return PRESENCE_ORDER[a.state] - PRESENCE_ORDER[b.state] || collator.compare(a.name, b.name);
}

function matches(friend: Friend, query: string): boolean {
  const q = fold(query.trim());
  if (!q) return true;
  return fold(friend.name).includes(q) || fold(friend.username).includes(q);
}

/**
 * Who the window lists (host1.md §1.2): friends online now (live invites only reach them), in
 * game first; under each crew its members who are friends ("Hepsini seç"), each person once (the
 * first crew that has them), then the other friends. Only friends: a crewmate who is not one is
 * never invited (§1.7). People already in the session show too, ticked and fixed.
 */
export function pickerGroups(input: PickerInput): PickerGroup[] {
  const eligible = new Map(
    input.friends
      .filter((f) => !input.blocked.has(f.id) && (f.state !== "offline" || input.taken.has(f.id)))
      .map((f) => [f.id, f]),
  );
  const placed = new Set<string>();
  const groups: PickerGroup[] = [];
  for (const crew of input.crews) {
    const people = crew.members
      .filter((m) => !m.me && m.friend && !m.blocked && !placed.has(m.id))
      .map((m) => eligible.get(m.id))
      .filter((f): f is Friend => f !== undefined);
    for (const person of people) placed.add(person.id);
    const shown = people.filter((p) => matches(p, input.query)).sort(byPresence);
    if (shown.length) groups.push({ key: `crew:${crew.id}`, crew, people: shown });
  }
  const rest = [...eligible.values()]
    .filter((f) => !placed.has(f.id) && matches(f, input.query))
    .sort(byPresence);
  if (rest.length) groups.push({ key: "friends", people: rest });
  return groups;
}

/** Everyone the window lists, in its order (for counting and for the search field's threshold). */
export function pickerCount(groups: readonly PickerGroup[]): number {
  return groups.reduce((sum, group) => sum + group.people.length, 0);
}

/** A crew header's button: "Hepsini seç" while some of its people could still be ticked. */
export function groupChoice(
  people: readonly Friend[],
  selected: ReadonlySet<string>,
  taken: ReadonlySet<string>,
): "all" | "clear" | "none" {
  const free = people.filter((p) => !taken.has(p.id));
  if (!free.length) return "none";
  return free.every((p) => selected.has(p.id)) ? "clear" : "all";
}

/**
 * "Hepsini seç" ticks the group's people in their order while places are left (`room`: how many
 * may be ticked in all); "Seçimi kaldır" unticks the group.
 */
export function toggleGroup(
  people: readonly Friend[],
  selected: ReadonlySet<string>,
  taken: ReadonlySet<string>,
  room: number,
): Set<string> {
  const next = new Set(selected);
  const free = people.filter((p) => !taken.has(p.id));
  if (groupChoice(people, selected, taken) === "clear") {
    for (const person of free) next.delete(person.id);
    return next;
  }
  for (const person of free) {
    if (next.size >= room) break;
    next.add(person.id);
  }
  return next;
}

/** One person ticked or unticked; past the room a tick does nothing. */
export function toggleOne(selected: ReadonlySet<string>, id: string, room: number): Set<string> {
  const next = new Set(selected);
  if (next.has(id)) next.delete(id);
  else if (next.size < room) next.add(id);
  return next;
}

// --- a world's row in the library ----------------------------------------------------------------

export type HostRowAction =
  /** Nothing of the feature on this row. */
  | { kind: "hidden" }
  /** "Arkadaşlarınla oyna"; `wait` says why it waits (another world is shared, the game runs). */
  | { kind: "play"; wait?: "busy" | "locked" }
  | { kind: "opening"; step: OpeningStep; percent?: number }
  /** This world is open: "Dünyan açık" and "Yönet". */
  | { kind: "open" }
  | { kind: "failed"; reason: OpenFailure };

export interface RowInput {
  enabled: boolean;
  place: HostPlace | undefined;
  session: HostSession | null;
  instanceId: string;
  folder: string;
  /** The instance's game runs (the tab is locked). */
  locked: boolean;
  readable: boolean;
}

export function hostRowAction(input: RowInput): HostRowAction {
  const { session } = input;
  const mine =
    session !== null && session.instanceId === input.instanceId && session.folder === input.folder;
  if (mine) {
    if (session.phase === "opening") {
      return { kind: "opening", step: session.step, percent: session.percent };
    }
    return session.phase === "open" ? { kind: "open" } : { kind: "failed", reason: session.reason };
  }
  if (!input.enabled || !input.place?.shown || !input.readable) return { kind: "hidden" };
  if (session && session.phase !== "failed") return { kind: "play", wait: "busy" };
  if (input.locked && input.place.playing !== input.folder) return { kind: "play", wait: "locked" };
  return { kind: "play" };
}

// --- "Katıl" on a world invite -----------------------------------------------------------------------

export type JoinButton =
  | { kind: "join" }
  | { kind: "retry" }
  | { kind: "rejoin" }
  | { kind: "busy"; step: "preparing" | "opening" | "connecting"; percent?: number }
  | { kind: "in" };

/**
 * What a world invite's "Katıl" shows in place (host1.md §1.4: "Hazırlanıyor" → "Oyun açılıyor" →
 * "Bağlanıyor"). Once ready, the launch says where it is: a game of another instance (the "Bağlan"
 * question, or a launch the player stopped) leaves the button as "Katıl".
 */
export function joinButton(
  join: JoinStage | undefined,
  launch: { phase: LaunchPhase; instanceId: string | null },
): JoinButton {
  if (!join) return { kind: "join" };
  switch (join.kind) {
    case "preparing":
      return { kind: "busy", step: "preparing", percent: join.percent };
    case "ask":
      return { kind: "busy", step: "preparing" };
    case "ready": {
      if (launch.instanceId !== join.instanceId) return { kind: "join" };
      const phase = launch.phase;
      if (phase.kind === "working" || phase.kind === "launching") {
        return { kind: "busy", step: "opening" };
      }
      if (phase.kind === "running") {
        return { kind: "busy", step: phase.shown ? "connecting" : "opening" };
      }
      return { kind: "join" };
    }
    case "connecting":
      return { kind: "busy", step: "connecting" };
    case "in":
      return { kind: "in" };
    case "lost":
      return { kind: "rejoin" };
    case "failed":
      return { kind: "retry" };
  }
}

/** A join is under way (the invite's "Kapat" and a second "Katıl" wait). */
export function joinActive(join: JoinStage | undefined): boolean {
  return Boolean(join) && join?.kind !== "failed" && join?.kind !== "lost";
}

/** "Tekrar dene" after mods were missing goes the way that fixes it: a copy of the install. */
export function retryWithCopy(join: JoinStage | undefined): boolean {
  return join?.kind === "failed" && join.failure === "mods";
}

/** The question a join waits on, the oldest first (one window at a time). */
export function waitingAsk(
  invites: readonly WorldInvite[],
): (WorldInvite & { join: Extract<JoinStage, { kind: "ask" }> }) | null {
  for (const invite of [...invites].reverse()) {
    if (invite.join?.kind === "ask") {
      return invite as WorldInvite & { join: Extract<JoinStage, { kind: "ask" }> };
    }
  }
  return null;
}

/** The words and the one action of a join that ended (a toast; the row offers "Tekrar dene"). */
export function failureWords(failure: JoinFailure): {
  key: string;
  action: "retry" | "copy" | null;
} {
  return {
    key: `host.join.failed.${failure}`,
    action: failure === "unreachable" ? "retry" : failure === "mods" ? "copy" : null,
  };
}

export function refusalKey(why: InviteRefusal): string {
  return why === "session" ? "host.invites.session" : "host.invites.daily";
}

export function openFailureKey(reason: OpenFailure): string {
  return `host.failed.${reason}`;
}

// --- where invites show -----------------------------------------------------------------------------

/** A friend's world invite (their row says "Dünyasında" and offers "Katıl"). */
export function inviteFrom(invites: readonly WorldInvite[], friend: string): WorldInvite | null {
  return invites.find((invite) => invite.from === friend) ?? null;
}

/** Home's line: the newest invite that is not quiet and was not closed there. */
export function homeInvite(
  invites: readonly WorldInvite[],
  closed: readonly string[],
): WorldInvite | null {
  return (
    invites.find(
      (invite) => !invite.quiet && !closed.includes(invite.session) && invite.join?.kind !== "in",
    ) ?? null
  );
}

/**
 * The world invites a list shows, as one string ("session name" pairs): the list renders again only
 * when an invite comes, goes or is renamed, never on each step of a join (each row follows its own
 * invite). `inviteList` reads it back.
 */
export function inviteKeys(invites: readonly WorldInvite[]): string {
  return invites.map((invite) => `${invite.session}\u0001${invite.name}`).join("\u0002");
}

export function inviteList(keys: string): Array<{ session: string; name: string }> {
  if (!keys) return [];
  return keys.split("\u0002").map((pair) => {
    const [session = "", name = ""] = pair.split("\u0001");
    return { session, name };
  });
}

// --- the person menu ----------------------------------------------------------------------------------

export interface PersonWorld {
  /** Their world invite: "Katıl" (`here` while the player is on the way there or in it). */
  join?: "ready" | "here";
  /** The player's world is open: "Dünyana çağır" and, for a guest, "Dünyadan çıkar". */
  host?: { invite: "ready" | "invited" | "full"; guest: boolean };
}

/**
 * What the person menu reads of world sharing, as one string: menus render again only when it
 * changes (a guest comes or goes, an invite arrives or its join starts or ends), not on every step
 * of a join's download.
 */
export function hostMenuKey(view: HostView): string {
  if (!view.enabled) return "";
  const session =
    view.session?.phase === "open"
      ? `${view.session.max}:${view.session.guests.map((g) => g.friend).join(",")}`
      : "-";
  const invites = view.invites
    .map((invite) => `${invite.from}:${joinActive(invite.join) ? 1 : 0}`)
    .join(",");
  return `${session}|${invites}`;
}

export function personWorld(view: HostView, id: string, friend: boolean): PersonWorld {
  if (!view.enabled) return {};
  const out: PersonWorld = {};
  const invite = inviteFrom(view.invites, id);
  if (invite) out.join = joinActive(invite.join) ? "here" : "ready";
  const session = view.session;
  if (session?.phase === "open") {
    const guest = session.guests.some((g) => g.friend === id);
    if (friend || guest) {
      out.host = {
        guest,
        invite: guest ? "invited" : session.guests.length >= session.max ? "full" : "ready",
      };
    }
  }
  return out;
}
