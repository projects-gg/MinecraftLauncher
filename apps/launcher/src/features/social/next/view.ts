// The friends service's snapshot as the player sees it (SOC-NEXT §3.6): with SOC-NEXT on, the
// friends they gave a nickname show under it everywhere the snapshot names them. Off, or without
// any nickname, it is the very snapshot the backend sent.
import type { SocialSnapshot } from "@/lib/api/social-types";
import { socNextOn } from "./flag";
import { withNicknames } from "./nickname";
import { useSocNextPrefs } from "./prefs";

export function shownView(view: SocialSnapshot): SocialSnapshot {
  if (!socNextOn()) return view;
  return withNicknames(view, useSocNextPrefs.getState().nicknames);
}
