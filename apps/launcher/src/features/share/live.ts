// "YAYINDA" and its preview card (SHARE-LIVE; share-live.md §1): who a tag stands for, what the
// card may show and offer, and its picture. The card asks for a picture only while it is open (the
// launcher keeps an answer 20 s, the page as long), keeps the last one while a newer one comes (the
// room's `th` moved) and never asks again on its own: no polling.
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { crewRoomFull, crewRoomName } from "@/features/voice/rules";
import { useVoiceUi } from "@/features/voice/store";
import type { VoiceCrewRoom, VoiceState } from "@/lib/api/voice-types";
import { shareApi } from "@/share/api";
import type { ShareKind, ShareThumb } from "@/share/types";

/**
 * What the card offers: "İzle" (`watch`), nothing while the player watches it already (`watching`)
 * or it is their own (`own`), "Katıl ve izle" (`join`); no picture at all where the player may not
 * go (`full`: the room has no place; `private`: someone's call the player is not in).
 */
export type LiveOffer = "watch" | "watching" | "join" | "own" | "full" | "private";

/** One share a tag stands for. Outside the room only the person and the room are known. */
export interface LiveTarget {
  person: string;
  /** The room it is shared in; null when the player cannot know it (a friend's call). */
  room: string | null;
  /** The room's crew (for "Katıl ve izle"), and whether it is the crew's "Genel". */
  crew: string | null;
  general: boolean;
  /** The crew room's own name (said under the sharer's name outside the room). */
  roomName: string | null;
  n: number | null;
  kind: ShareKind | null;
  /** How many watch (the player's own share: the faces the panel lists). */
  watchers: number | null;
  /** When the room last stored its picture (`th`), to ask again while the card is open. */
  th: number | null;
  offer: LiveOffer;
}

type RoomState = Pick<VoiceState, "room" | "share">;

/** Someone's share in the player's own room (the player's too), or null when they share nothing. */
export function liveTargetIn(state: RoomState, person: string): LiveTarget | null {
  const room = state.room;
  const share = state.share;
  if (!room || !share?.on) return null;
  const base = {
    person,
    room: room.id,
    crew: room.crew,
    general: room.general,
    roomName: null,
  };
  if (person === room.me.id) {
    const mine = share.mine.find((m) => m.phase !== "starting");
    if (!mine) return null;
    return {
      ...base,
      n: mine.n,
      kind: mine.kind,
      watchers: mine.watchers.length,
      th: mine.th ?? null,
      offer: "own",
    };
  }
  const theirs = share.live.filter((l) => l.person === person);
  if (!theirs.length) return null;
  // The first one the player does not watch yet ("İzle" watches the same one).
  const unwatched = theirs.find(
    (l) => !share.watching.some((w) => w.person === l.person && w.n === l.n),
  );
  const shown = unwatched ?? theirs[0];
  if (!shown) return null;
  return {
    ...base,
    n: shown.n,
    kind: shown.kind,
    watchers: shown.watchers,
    th: shown.th ?? null,
    offer: unwatched ? "watch" : "watching",
  };
}

/** The player's own live share, as their row's tag shows it (null while none is live). */
export function ownTarget(state: RoomState): LiveTarget | null {
  return state.room ? liveTargetIn(state, state.room.me.id) : null;
}

/** A crew room seen from its row on the crew page: everyone live in it, in the room's order. */
export function crewRoomTargets(
  state: RoomState,
  crew: string,
  room: VoiceCrewRoom,
  crewMax: number,
  roomName: string,
): LiveTarget[] {
  if (state.room?.id === room.id) {
    // The player is in it: the room's own state knows more (what is shared, who watches).
    const people = [...new Set(state.share?.on ? state.share.live.map((l) => l.person) : [])];
    return people.flatMap((person) => liveTargetIn(state, person) ?? []);
  }
  const full = crewRoomFull(room, crewMax);
  return (room.live ?? []).map((person) => ({
    person,
    room: room.id,
    crew,
    general: room.general,
    roomName,
    n: null,
    kind: null,
    watchers: null,
    th: null,
    offer: full ? "full" : "join",
  }));
}

/**
 * A friend who is live (`Friend.voiceLive`): in the player's room the share itself; in a crew room
 * of the player's crews that room ("Katıl ve izle" while it has a place); else someone's call the
 * player is not in (locked).
 */
export function friendTarget(
  state: Pick<VoiceState, "room" | "share" | "crews">,
  friend: string,
  roomName: (room: VoiceCrewRoom) => string,
): LiveTarget {
  const here = liveTargetIn(state, friend);
  if (here) return here;
  for (const entry of state.crews) {
    const room = entry.rooms.find((r) => r.people.includes(friend));
    if (!room || state.room?.id === room.id) continue;
    return {
      person: friend,
      room: room.id,
      crew: entry.crew,
      general: room.general,
      roomName: roomName(room),
      n: null,
      kind: null,
      watchers: null,
      th: null,
      offer: crewRoomFull(room, entry.crewMax) ? "full" : "join",
    };
  }
  return {
    person: friend,
    room: null,
    crew: null,
    general: false,
    roomName: null,
    n: null,
    kind: null,
    watchers: null,
    th: null,
    offer: "private",
  };
}

/** Whether the card may ask for a picture at all (never where the player may not go). */
export function asksPicture(target: LiveTarget): boolean {
  return target.room !== null && target.offer !== "full" && target.offer !== "private";
}

// --- the tags' targets, from the voice state (stable slices, worked out once per change) ------------

const NONE: LiveTarget[] = [];
const NO_CREWS: VoiceState["crews"] = [];

/** Someone in the player's room who shares (the player's own row: `person` is the player). */
export function useRoomTargets(person: string): LiveTarget[] {
  const room = useVoiceUi((s) => s.state.room);
  const share = useVoiceUi((s) => s.state.share);
  return useMemo(() => {
    const target = liveTargetIn({ room, share }, person);
    return target ? [target] : NONE;
  }, [room, share, person]);
}

/** Everyone live in a crew room, for its row on the crew page. */
export function useCrewRoomTargets(
  crew: string,
  room: VoiceCrewRoom,
  crewMax: number,
): LiveTarget[] {
  const { t } = useTranslation();
  const inRoom = useVoiceUi((s) => s.state.room);
  const share = useVoiceUi((s) => s.state.share);
  return useMemo(() => {
    const targets = crewRoomTargets(
      { room: inRoom, share },
      crew,
      room,
      crewMax,
      crewRoomName(room, t),
    );
    return targets.length ? targets : NONE;
  }, [inRoom, share, crew, room, crewMax, t]);
}

/** A friend the friend list says is live (`Friend.voiceLive`). */
export function useFriendTargets(friend: string, live: boolean): LiveTarget[] {
  const { t } = useTranslation();
  // Only a live friend's row follows voice chat's changes: the list's other rows never draw again for them.
  const room = useVoiceUi((s) => (live ? s.state.room : null));
  const share = useVoiceUi((s) => (live ? s.state.share : undefined));
  const crews = useVoiceUi((s) => (live ? s.state.crews : NO_CREWS));
  return useMemo(
    () =>
      live ? [friendTarget({ room, share, crews }, friend, (each) => crewRoomName(each, t))] : NONE,
    [live, room, share, crews, friend, t],
  );
}

// --- the picture ---------------------------------------------------------------------------------

/** As long as the launcher keeps an answer (share-live.md §3). */
export const THUMB_KEEP_MS = 20_000;
const KEPT_MAX = 12;

const kept = new Map<string, { at: number; thumb: ShareThumb }>();

/** A picture's key: the share and the room's picture time (a newer `th` is a new picture). */
export function thumbKey(target: Pick<LiveTarget, "room" | "person" | "n" | "th">): string {
  return `${target.room ?? ""}|${target.person}|${target.n ?? ""}|${target.th ?? ""}`;
}

/** The same share whatever its picture's time (the card keeps showing the last one meanwhile). */
const shareKeyOf = (target: Pick<LiveTarget, "room" | "person" | "n">) =>
  `${target.room ?? ""}|${target.person}|${target.n ?? ""}`;

function keep(key: string, thumb: ShareThumb, now: number) {
  kept.delete(key);
  kept.set(key, { at: now, thumb });
  while (kept.size > KEPT_MAX) {
    const oldest = kept.keys().next().value;
    if (oldest === undefined) break;
    kept.delete(oldest);
  }
}

function keptThumb(key: string, now: number): ShareThumb | null {
  const hit = kept.get(key);
  return hit && now - hit.at < THUMB_KEEP_MS ? hit.thumb : null;
}

/** Tests start from nothing kept. */
export function forgetThumbs() {
  kept.clear();
}

/** A picture decoded before it shows (no half-drawn frame); a picture that fails shows nothing. */
async function decoded(thumb: ShareThumb): Promise<ShareThumb> {
  if (thumb.kind !== "image" || typeof Image === "undefined") return thumb;
  const image = new Image();
  if (typeof image.decode !== "function") return thumb;
  image.src = thumb.dataUrl;
  try {
    await image.decode();
    return thumb;
  } catch {
    return { kind: "none" };
  }
}

export interface ShownThumb {
  /** The answer for this share: a picture (maybe the last one while a newer comes), locked, none;
   * null while the first answer is on its way. */
  thumb: ShareThumb | null;
  /** The picture's key, so a newer picture can fade in over the last one. */
  key: string | null;
}

/**
 * The card's picture: asked when the card opens (and again when the room's `th` moves while it is
 * open), never otherwise; an answer that comes after the card closed is kept for the next opening
 * but changes nothing on screen.
 */
export function useShareThumb(target: LiveTarget | null, open: boolean): ShownThumb {
  const asking = target !== null && open && asksPicture(target);
  const key = asking && target ? thumbKey(target) : null;
  const share = asking && target ? shareKeyOf(target) : null;
  const [shown, setShown] = useState<{
    share: string;
    key: string;
    thumb: ShareThumb;
    at: number;
  } | null>(null);
  // A card that closes forgets a picture the launcher no longer keeps: the next one never opens on
  // an old frame.
  useEffect(() => {
    if (!open) setShown((last) => (last && Date.now() - last.at < THUMB_KEEP_MS ? last : null));
  }, [open]);
  // `key` holds the room, the person, `n` and `th`: the target's other fields change nothing.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the key is the request
  useEffect(() => {
    if (!key || !share || !target?.room) return;
    const hit = keptThumb(key, Date.now());
    if (hit) {
      setShown({ share, key, thumb: hit, at: Date.now() });
      return;
    }
    let gone = false;
    const { room, person, n } = target;
    void shareApi
      .thumb(room, person, n)
      .then(decoded)
      .catch((): ShareThumb => ({ kind: "none" }))
      .then((thumb) => {
        const at = Date.now();
        keep(key, thumb, at);
        if (gone) return;
        setShown((last) =>
          // A newer picture that did not come keeps the last one on screen.
          thumb.kind === "none" && last?.share === share && last.thumb.kind === "image"
            ? last
            : { share, key, thumb, at },
        );
      });
    return () => {
      gone = true;
    };
  }, [key]);
  if (!share || shown?.share !== share) return { thumb: null, key: null };
  return { thumb: shown.thumb, key: shown.key };
}
