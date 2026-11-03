// SOC-NEXT §3.7 for code outside the friends feature (voice chat's ring, the overlay's glue): is
// someone ignored now? Off (the switch), nobody is.
import { socNextOn } from "./flag";
import { useSocNextPrefs } from "./prefs";

export function ignoringNow(id: string): boolean {
  return socNextOn() && useSocNextPrefs.getState().ignored.includes(id);
}
