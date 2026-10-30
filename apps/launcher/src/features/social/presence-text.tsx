import type { TFunction } from "i18next";
import type { Friend } from "@/lib/api/social-types";
import { presenceParts } from "./model";

/**
 * The line under a friend's or crewmate's name (SOC9): in game with a version, the place in bone
 * (jade where "Katıl" leads, as the line around it), a plain space, then the version in ash, so
 * colour does the separating: "Projects'te 1.21.4". Otherwise the place alone, in the line's colour.
 */
export function PresenceText({
  friend,
  t,
  joinable = false,
}: {
  friend: Friend;
  t: TFunction;
  /** The line is jade ("Katıl" leads there): the place keeps it. */
  joinable?: boolean;
}) {
  const { place, version } = presenceParts(friend, t);
  if (!version) {
    return <>{place}</>;
  }
  return (
    <>
      <span className={joinable ? undefined : "text-bone-100"}>{place}</span>{" "}
      <span className="text-ash-400">{version}</span>
    </>
  );
}
