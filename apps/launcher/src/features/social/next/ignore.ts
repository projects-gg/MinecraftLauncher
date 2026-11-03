// SOC-NEXT §3.7, pure rules (ignore.test.ts): "Yok say" keeps someone at a distance without
// blocking them, and they are never told. Only the player's own screen changes: in crew chat their
// messages fold into one quiet line each run ("Yok saydığın birinin 3 mesajı", "Göster"); nothing
// they do is told (a message, a call, a plan, an invite, their coming online); a direct
// conversation with them stays readable, faded, and waits for no one (the friends entry does not
// count it). The list travels with the settings backup (ids only).
import type { SocialNotice } from "@/lib/api/social-types";

/** Who a notice is from (or about), when it is a person's doing. */
export function noticeFrom(notice: SocialNotice): string | null {
  switch (notice.kind) {
    case "invite":
      return notice.invite.from;
    case "request":
      return notice.request.id;
    case "friendAdded":
      return null;
    case "friendOnline":
    case "friendOnProjects":
    case "crewmateOnProjects":
      return notice.id;
    case "teamInvite":
      return notice.invite.from.id;
    case "crewCall":
    case "crewCallJoin":
      return notice.call.from;
    case "joinRequest":
      return notice.request.id;
    case "crewPlan":
      return notice.plan.by;
    case "chatMessage":
      return notice.message.from;
    default:
      return null;
  }
}

/** The notice is about someone the player ignores: nothing says it (the list still shows it). */
export function fromIgnored(notice: SocialNotice, ignored: ReadonlySet<string>): boolean {
  const from = noticeFrom(notice);
  return from !== null && ignored.has(from);
}

/** One item of a conversation, as folding reads it. */
export interface FoldItem {
  key: string;
  /** A message's sender (a tombstone's too); none for a local line (a plan, a newcomer), which
   * breaks a run. */
  from?: string;
}

/** A message inside a folded run: the run's first one draws the line, the others nothing. */
export interface Folded {
  /** The run's first message: what "Göster" shows again. */
  run: string;
  lead: boolean;
  /** Messages in the run. */
  count: number;
}

/**
 * Runs of an ignored person's messages, one after another with nothing else between, fold into
 * one line at their first message; a run the player opened (`shown`, by its first key) stays
 * open. Only folded messages are in the map.
 */
export function foldRuns(
  items: readonly FoldItem[],
  ignored: ReadonlySet<string>,
  shown: ReadonlySet<string>,
): Map<string, Folded> {
  const out = new Map<string, Folded>();
  const runs: Array<{ from: string; keys: string[] }> = [];
  let current: { from: string; keys: string[] } | undefined;
  for (const item of items) {
    const from = item.from;
    if (from && ignored.has(from)) {
      if (current && current.from === from) current.keys.push(item.key);
      else {
        current = { from, keys: [item.key] };
        runs.push(current);
      }
    } else {
      current = undefined;
    }
  }
  for (const run of runs) {
    const first = run.keys[0];
    if (first === undefined || shown.has(first)) continue;
    run.keys.forEach((key, index) => {
      out.set(key, { run: first, lead: index === 0, count: run.keys.length });
    });
  }
  return out;
}

/** Direct conversations with ignored friends, as "Sessize al" holds them (`ChatMutes`): the
 * friends entry and the in-game overlay say nothing of them. */
export function ignoredDirectMutes(
  me: string | undefined,
  ignored: readonly string[],
  direct: (a: string, b: string) => string,
): Record<string, null> {
  const out: Record<string, null> = {};
  if (!me) return out;
  for (const id of ignored) out[direct(me, id)] = null;
  return out;
}
