// Friend categories ("Okul", "PvP"): the player's own, in their order, and the implicit
// "Arkadaşlar" for everyone else. Pure rules (no React), each with a unit test (groups.test.ts):
// changing the document, the sections the list shows, and where a dragged row lands.
import type { Friend, FriendGroup, FriendGroups } from "@/lib/api/social-types";
import { groupFriends } from "./model";

/** The implicit category of friends without one. Never stored. */
export const REST = "rest";
/** As many categories as the service keeps. */
export const MAX_GROUPS = 20;
export const GROUP_NAME_MAX = 24;

export const EMPTY_GROUPS: FriendGroups = { version: 0, groups: [], members: {} };

/** Spaces collapsed and trimmed, at most 24 characters (what the service stores). */
export function cleanGroupName(input: string): string {
  return [...input.replace(/\s+/g, " ").trim()].slice(0, GROUP_NAME_MAX).join("");
}

/** A new id: 8 lower case letters and digits, not used yet. */
export function newGroupId(groups: readonly FriendGroup[], random = Math.random): string {
  const alphabet = "abcdefghijklmnopqrstuvwxyz0123456789";
  for (;;) {
    let id = "";
    for (let i = 0; i < 8; i++) id += alphabet[Math.floor(random() * alphabet.length)];
    if (id !== REST && !groups.some((g) => g.id === id)) return id;
  }
}

/** The category of a friend: a group id, or REST. */
export function groupOf(doc: FriendGroups, friendId: string): string {
  const group = doc.members[friendId];
  return group && doc.groups.some((g) => g.id === group) ? group : REST;
}

function withMembers(doc: FriendGroups, members: Record<string, string>): FriendGroups {
  return { ...doc, members };
}

/** A new category at the end (before "Arkadaşlar"), optionally with a first friend in it. */
export function addGroup(
  doc: FriendGroups,
  id: string,
  name: string,
  friendId?: string,
): FriendGroups {
  const clean = cleanGroupName(name);
  if (!clean || doc.groups.length >= MAX_GROUPS || doc.groups.some((g) => g.id === id)) return doc;
  const next = { ...doc, groups: [...doc.groups, { id, name: clean }] };
  return friendId ? moveFriend(next, friendId, id) : next;
}

export function renameGroup(doc: FriendGroups, id: string, name: string): FriendGroups {
  const clean = cleanGroupName(name);
  if (!clean) return doc;
  return { ...doc, groups: doc.groups.map((g) => (g.id === id ? { ...g, name: clean } : g)) };
}

/** Deletes a category; its friends go back to "Arkadaşlar". */
export function removeGroup(doc: FriendGroups, id: string): FriendGroups {
  const members = Object.fromEntries(Object.entries(doc.members).filter(([, g]) => g !== id));
  return { ...doc, groups: doc.groups.filter((g) => g.id !== id), members };
}

/** Puts a friend into a category (REST: back to "Arkadaşlar"). */
export function moveFriend(doc: FriendGroups, friendId: string, group: string): FriendGroups {
  const members = { ...doc.members };
  if (group === REST || !doc.groups.some((g) => g.id === group)) delete members[friendId];
  else members[friendId] = group;
  return withMembers(doc, members);
}

/** Categories in this order (unknown ids ignored, missing ones kept at the end). */
export function reorderGroups(doc: FriendGroups, order: readonly string[]): FriendGroups {
  const byId = new Map(doc.groups.map((g) => [g.id, g]));
  const sorted = order.flatMap((id) => {
    const group = byId.get(id);
    byId.delete(id);
    return group ? [group] : [];
  });
  return { ...doc, groups: [...sorted, ...byId.values()] };
}

/** Moves a category one place up or down; null when it is already at that end. */
export function stepGroup(doc: FriendGroups, id: string, step: -1 | 1): FriendGroups | null {
  const at = doc.groups.findIndex((g) => g.id === id);
  const to = at + step;
  if (at < 0 || to < 0 || to >= doc.groups.length) return null;
  const order = doc.groups.map((g) => g.id);
  [order[at], order[to]] = [order[to] as string, order[at] as string];
  return reorderGroups(doc, order);
}

/**
 * Moves a friend to the category above or below theirs (Alt+↑/↓), "Arkadaşlar" last; null at
 * either end.
 */
export function stepFriend(doc: FriendGroups, friendId: string, step: -1 | 1): FriendGroups | null {
  const order = [...doc.groups.map((g) => g.id), REST];
  const at = order.indexOf(groupOf(doc, friendId));
  const to = order[at + step];
  return to === undefined ? null : moveFriend(doc, friendId, to);
}

/** Entries of people who are no longer friends dropped (the next save sends this). */
export function pruneMembers(
  doc: FriendGroups,
  friends: readonly Pick<Friend, "id">[],
): FriendGroups {
  const ids = new Set(friends.map((f) => f.id));
  const groups = new Set(doc.groups.map((g) => g.id));
  const members = Object.fromEntries(
    Object.entries(doc.members).filter(([friend, group]) => ids.has(friend) && groups.has(group)),
  );
  return withMembers(doc, members);
}

/** The same categories, names and members (versions aside). */
export function sameGroups(a: FriendGroups, b: FriendGroups): boolean {
  const members = (doc: FriendGroups) =>
    Object.entries(doc.members)
      .sort(([x], [y]) => (x < y ? -1 : x > y ? 1 : 0))
      .join("|");
  return (
    a.groups.length === b.groups.length &&
    a.groups.every((g, i) => g.id === b.groups[i]?.id && g.name === b.groups[i]?.name) &&
    members(a) === members(b)
  );
}

export interface GroupSection {
  /** A category id, or REST. */
  id: string;
  /** The player's name for it; null for "Arkadaşlar". */
  name: string | null;
  /** In game first (projects.gg first), then in the launcher, then offline. */
  friends: Friend[];
  online: number;
}

/**
 * The friend list by category: the player's categories in their order (empty ones too, they are
 * drop targets), then "Arkadaşlar". Inside each, the presence order of the plain list, favourites
 * first.
 */
export function groupSections(
  friends: readonly Friend[],
  doc: FriendGroups,
  favorites: readonly string[],
): GroupSection[] {
  const buckets = new Map<string, Friend[]>([
    ...doc.groups.map((g) => [g.id, []] as [string, Friend[]]),
    [REST, []],
  ]);
  for (const friend of friends) buckets.get(groupOf(doc, friend.id))?.push(friend);
  const section = (id: string, name: string | null): GroupSection => {
    const { playing, online, offline } = groupFriends(buckets.get(id) ?? [], favorites);
    return {
      id,
      name,
      friends: [...playing, ...online, ...offline],
      online: playing.length + online.length,
    };
  };
  return [...doc.groups.map((g) => section(g.id, g.name)), section(REST, null)];
}

/**
 * Where `element` sits in `list`'s layout: px from the list's top, leaving out the transforms of
 * everything in between (rows gliding aside). The offsets are added up the offset parents to the
 * list, because a gliding row is the offset parent of what it holds (a transform makes one): its
 * header's own `offsetTop` reads 0 then, and every drop fell back into the friend's own category
 * (FIX2). `null` when `element` is not laid out inside `list` (the caller measures on screen).
 */
export function layoutTop(element: HTMLElement, list: HTMLElement): number | null {
  let top = 0;
  let node: HTMLElement | null = element;
  while (node !== list) {
    if (!node) return null;
    top += node.offsetTop;
    const parent = node.offsetParent as HTMLElement | null;
    // An offset is measured to the parent's padding edge; its border sits above that.
    if (parent && parent !== list) top += parent.clientTop;
    node = parent;
  }
  return top;
}

/**
 * Where a dragged friend lands: the category whose block holds `y`. `headers` are the categories'
 * header tops in list order (layout positions, so rows gliding by do not move the target).
 */
export function dropTarget(
  y: number,
  headers: ReadonlyArray<{ id: string; top: number }>,
): string | null {
  let target: string | null = null;
  for (const header of headers) {
    if (header.top <= y) target = header.id;
    else break;
  }
  return target;
}

/**
 * The next order of the categories while one is dragged, one step at a time like a reorder list:
 * it passes a neighbour once its centre `y` goes beyond that neighbour's middle in the direction
 * it moves. `blocks` are the other categories' blocks (layout positions) in the current order.
 */
export function dragOrder(
  order: readonly string[],
  dragged: string,
  y: number,
  direction: 1 | -1,
  blocks: ReadonlyMap<string, { top: number; bottom: number }>,
): string[] {
  const at = order.indexOf(dragged);
  const neighbour = order[at + direction];
  if (at < 0 || neighbour === undefined) return [...order];
  const block = blocks.get(neighbour);
  if (!block) return [...order];
  const middle = (block.top + block.bottom) / 2;
  const passed = direction === 1 ? y > middle : y < middle;
  if (!passed) return [...order];
  const next = [...order];
  next[at] = neighbour;
  next[at + direction] = dragged;
  return next;
}
