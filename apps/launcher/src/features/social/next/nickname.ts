// SOC-NEXT §3.6, pure rules (nickname.test.ts): a friend's nickname, kept on this computer only
// (never in the settings backup: children's real names stay off the cloud). The friends service's
// snapshot is shown with it in one place (`withNicknames`): every row, header, sender name, card
// and notice that reads a friend's `name` shows the nickname, and `realName` keeps the name the
// service knows (the card and a tooltip show it; the friend search finds both). It holds while they
// are the player's friend; a crewmate who is no friend keeps their own name.
import type { SocialSnapshot } from "@/lib/api/social-types";

/** At most this many characters (as people count them: an emoji is one). */
export const NICKNAME_MAX = 24;

/** The nickname as it is kept: spaces collapsed and trimmed, no control characters; null when
 * nothing is left or it is longer than `NICKNAME_MAX`. */
export function cleanNickname(text: string): string | null {
  const clean = [...text].map((ch) => (invisible(ch.codePointAt(0) ?? 0) ? " " : ch)).join("");
  const name = clean.split(/\s+/).filter(Boolean).join(" ");
  if (!name) return null;
  return graphemes(name) <= NICKNAME_MAX ? name : null;
}

/** Control characters, zero-width spaces, direction marks and line separators (an emoji's zero
 * width joiner stays: it holds a family or a flag together). */
function invisible(cp: number): boolean {
  return (
    cp < 0x20 ||
    (cp >= 0x7f && cp <= 0x9f) ||
    cp === 0x200b ||
    cp === 0x200e ||
    cp === 0x200f ||
    (cp >= 0x2028 && cp <= 0x202e) ||
    (cp >= 0x2066 && cp <= 0x2069) ||
    cp === 0xfeff
  );
}

/** How many characters a person counts in `text` (an emoji with its tone is one). */
export function graphemes(text: string): number {
  let count = 0;
  for (const _ of new Intl.Segmenter("tr", { granularity: "grapheme" }).segment(text)) count += 1;
  return count;
}

/** Someone as shown: the nickname in `name`, the service's name in `realName`. */
export interface Nicknamed {
  name: string;
  realName?: string;
}

/** The name the service knows (the same as `name` without a nickname). */
export function realNameOf(person: Nicknamed): string {
  return person.realName ?? person.name;
}

function named<T extends { name: string }>(person: T, nick: string | undefined): T {
  if (!nick || nick === person.name) return person;
  return { ...person, name: nick, realName: person.name };
}

/**
 * The snapshot as the player sees it: their friends under their nicknames, wherever the snapshot
 * names them (the list, the crews they share, invites, calls and plans of theirs). Without any
 * nickname for a friend it is the same snapshot (nothing draws again).
 */
export function withNicknames(
  view: SocialSnapshot,
  nicknames: Readonly<Record<string, string>>,
): SocialSnapshot {
  const friends = new Map<string, string>();
  for (const friend of view.friends) {
    const nick = nicknames[friend.id];
    if (nick) friends.set(friend.id, nick);
  }
  if (!friends.size) return view;
  const nick = (id: string | undefined) => (id ? friends.get(id) : undefined);
  return {
    ...view,
    friends: view.friends.map((friend) => named(friend, nick(friend.id))),
    crews: view.crews.map((crew) =>
      crew.members.some((member) => friends.has(member.id))
        ? { ...crew, members: crew.members.map((member) => named(member, nick(member.id))) }
        : crew,
    ),
    invites: view.invites.map((invite) => {
      const name = nick(invite.from);
      return name && invite.name ? { ...invite, name } : invite;
    }),
    calls: view.calls.map((call) => {
      const name = nick(call.from);
      return name && call.name ? { ...call, name } : call;
    }),
    plans: view.plans.map((plan) => {
      const name = nick(plan.by);
      return name && plan.byName ? { ...plan, byName: name } : plan;
    }),
    teamInvites: view.teamInvites.map((invite) =>
      friends.has(invite.from.id)
        ? { ...invite, from: named(invite.from, nick(invite.from.id)) }
        : invite,
    ),
  };
}

/** A name a notice carries from the service, as the player sees that person. */
export function noticeName(
  id: string | undefined,
  name: string | undefined,
  view: Pick<SocialSnapshot, "friends">,
): string | undefined {
  if (!id) return name;
  const friend = view.friends.find((one) => one.id === id) as Nicknamed | undefined;
  return friend?.realName !== undefined ? friend.name : name;
}
