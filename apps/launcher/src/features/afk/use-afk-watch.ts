// Tells the player when an AFK account stops for good (watch.ts decides which): a system
// notification while the launcher is not in front (in the tray, minimized, behind the game), a
// toast with "Göster" while it is, and nothing on the AFK page, whose cards already say it. The
// system notification follows Settings → Genel → Masaüstü bildirimleri (`notify`).
import { toast } from "@pl/ui";
import { useRouter } from "@tanstack/react-router";
import { useEffect } from "react";
import { api } from "@/lib/api";
import { notify, windowAway } from "@/lib/attention";
import i18n from "@/lib/i18n";
import { type AfkSeen, type AfkStop, findStops, seenOf, stopNotice } from "./watch";

/** Every console line is an AFK change too: the list is read at most this often. */
const READ_GAP_MS = 500;
/** Accounts that stop this close together (a ban of every account, a whitelist) share a notice. */
const GATHER_MS = 1500;

/**
 * Follows the AFK accounts for the whole session. Holds no React state, so the shell that calls
 * it never renders again for an AFK change.
 */
export function useAfkWatch() {
  const router = useRouter();
  useEffect(() => {
    let seen: AfkSeen | null = null;
    let closed = false;
    let reading = false;
    let readAgain = false;
    let readTimer: ReturnType<typeof setTimeout> | undefined;
    let gatherTimer: ReturnType<typeof setTimeout> | undefined;
    let gathered: AfkStop[] = [];

    const announce = () => {
      gatherTimer = undefined;
      const [first, ...rest] = gathered;
      gathered = [];
      if (!first || closed) return;
      const { title, body } = stopNotice([first, ...rest], i18n.t);
      if (windowAway()) {
        void notify(title, body);
      } else if (router.state.location.pathname !== "/afk") {
        toast.error(title, {
          description: body,
          duration: 10_000,
          action: {
            label: i18n.t("afkAlerts.show"),
            onClick: () => void router.navigate({ to: "/afk" }),
          },
        });
      }
    };

    const schedule = () => {
      if (readTimer !== undefined || closed) return;
      readTimer = setTimeout(() => {
        readTimer = undefined;
        void read();
      }, READ_GAP_MS);
    };

    const read = async () => {
      if (reading) {
        readAgain = true;
        return;
      }
      reading = true;
      try {
        const accounts = await api.afkAccounts();
        if (closed) return;
        const stops = findStops(seen, accounts);
        seen = seenOf(accounts);
        if (stops.length > 0) {
          gathered.push(...stops);
          gatherTimer ??= setTimeout(announce, GATHER_MS);
        }
      } catch {
        // The next change reads again.
      } finally {
        reading = false;
        if (readAgain) {
          readAgain = false;
          schedule();
        }
      }
    };

    // The first read is the starting point: accounts already stopped are not news.
    void read();
    const stop = api.onAfkChange(schedule);
    return () => {
      closed = true;
      stop();
      clearTimeout(readTimer);
      clearTimeout(gatherTimer);
    };
  }, [router]);
}
