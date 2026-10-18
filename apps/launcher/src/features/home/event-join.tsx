// "Katıl" on the event card: while an event that takes place on the server is live
// (`events[].join`), one click starts the Projects instance straight into the server. The same
// path as "Katıl" in the friends list (features/social/join.ts): the official instance (made
// first if the player has none), its own server whatever the auto-join setting, and the usual
// launch with its account and AFK questions. One game at a time: it waits while a game is being
// prepared or played. It sits where the reminder bell sits on an upcoming event.
import { Play } from "@phosphor-icons/react";
import { Button, cn, Tooltip } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { gameBusy, joinProjects } from "@/features/social/join";
import { useLaunch } from "@/stores/launch";

export function EventJoin({
  describedBy,
  className,
}: {
  /** The event's title, read after the button's name. */
  describedBy?: string;
  className?: string;
}) {
  const { t } = useTranslation();
  const busy = useLaunch((s) => gameBusy(s.phase.kind));
  // A disabled button takes no pointer: the tooltip hangs on its wrapper, there all the time (and
  // quiet while the button works), so the button stays the same and fades into its waiting look.
  return (
    <Tooltip content={t("eventJoin.busy")} disabled={!busy}>
      <span className={cn("inline-flex", className)}>
        <Button
          size="sm"
          variant="secondary"
          icon={<Play size={13} weight="fill" className="text-gold-300" />}
          disabled={busy}
          aria-describedby={describedBy}
          onClick={() => void joinProjects()}
        >
          {t("eventJoin.join")}
        </Button>
      </span>
    </Tooltip>
  );
}
