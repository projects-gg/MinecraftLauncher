// Settings → Arkadaşlar: the same settings as the friends panel (or its opt-in screen while
// signed out), where players look for privacy and "Verilerimi sil".
import { Skeleton } from "@pl/ui";
import { ConsentPage } from "./consent-page";
import { SocialSettings } from "./settings-page";
import { useSocialUi } from "./store";

export function FriendsSettingsSection() {
  const auth = useSocialUi((s) => s.view.auth);
  const hasProfile = useSocialUi((s) => Boolean(s.view.me));
  if (auth === "signedIn" && hasProfile) return <SocialSettings />;
  if (auth === "restoring" || auth === "signedIn") {
    return (
      <div aria-hidden className="space-y-3 py-4">
        <Skeleton className="h-4 w-40" />
        <Skeleton className="h-11 w-full" />
        <Skeleton className="h-11 w-full" />
      </div>
    );
  }
  return <ConsentPage />;
}
