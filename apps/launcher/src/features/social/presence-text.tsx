import { Clock } from "@phosphor-icons/react";
import type { TFunction } from "i18next";
import type { Friend } from "@/lib/api/social-types";
import { presenceParts } from "./model";
import { friendPlayed, playedText, usePlayedClock } from "./played";

/**
 * The line under a friend's or crewmate's name (SOC9): in game with a version, the place in bone
 * (jade where "Katıl" leads, as the line around it), a plain space, then the version in ash, so
 * colour does the separating: "Projects'te 1.21.4". Otherwise the place alone, in the line's colour.
 * With `played` (CREW-PLUS; the friends panel, whose clock ticks), how long they have been playing
 * follows in ash after a small clock, as the headset marks voice chat in the same line: the clock
 * says "for", where a bare "25 dk" could read as "25 minutes ago".
 */
export function PresenceText({
  friend,
  t,
  joinable = false,
  played = false,
}: {
  friend: Friend;
  t: TFunction;
  /** The line is jade ("Katıl" leads there): the place keeps it. */
  joinable?: boolean;
  /** Say how long they have been playing (only where `usePlayedTicker` keeps the time). */
  played?: boolean;
}) {
  const { place, version } = presenceParts(friend, t);
  // Only a friend in a session listens to the clock: the others never redraw on its ticks.
  const now = usePlayedClock((s) => (played && friend.playingSince !== undefined ? s.now : 0));
  const time = now ? friendPlayed(friend, now) : null;
  const tail = time ? (
    <span className="text-ash-400">
      <Clock size={12} weight="bold" aria-hidden className="mr-0.5 ml-1.5 inline align-[-1.5px]" />
      {playedText(time, t)}
    </span>
  ) : null;
  if (!version) {
    return (
      <>
        {place}
        {tail}
      </>
    );
  }
  return (
    <>
      <span className={joinable ? undefined : "text-bone-100"}>{place}</span>{" "}
      <span className="text-ash-400">{version}</span>
      {tail}
    </>
  );
}
