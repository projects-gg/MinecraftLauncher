import { toast } from "@pl/ui";
import { useNavigate } from "@tanstack/react-router";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useSocialUi } from "@/features/social/store";
import { api } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { mayCheckReminder, worthReminding } from "./model";

/** The check waits this long after start (the launcher is busy then, and so is the player). */
const FIRST_CHECK = 3 * 60 * 1000;
const TOAST_FOR = 15_000;

/**
 * Now and then, when a cleanup would free a real amount of space, one quiet toast offers to show
 * it. At most every two weeks, never for the same gain twice in a row, never in do-not-disturb,
 * during a game or before the welcome flow is done; closing it is enough.
 */
export function useCleanupReminder() {
  const { t } = useTranslation();
  const navigate = useNavigate();

  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(async () => {
      const prefs = usePrefs.getState();
      const phase = useLaunch.getState().phase.kind;
      const quiet =
        useSocialUi.getState().view.me?.status === "dnd" ||
        phase === "working" ||
        phase === "launching" ||
        phase === "running";
      const now = Date.now();
      if (
        document.hidden ||
        !mayCheckReminder({
          now,
          lastShownAt: prefs.cleanupRemindedAt,
          onboarded: prefs.onboarded,
          quiet,
        })
      ) {
        return;
      }
      const summary = await api.cleanup.summary(prefs.cleanupSetDays).catch(() => null);
      if (cancelled || !summary) return;
      const worth = worthReminding({
        now,
        lastShownAt: prefs.cleanupRemindedAt,
        lastBytes: prefs.cleanupRemindedBytes,
        reclaimable: summary.reclaimableBytes,
        freeBytes: summary.freeBytes,
      });
      if (!worth) return;
      prefs.set("cleanupRemindedAt", now);
      prefs.set("cleanupRemindedBytes", summary.reclaimableBytes);
      toast(t("dataSettings.cleanup.reminder", { size: formatBytes(summary.reclaimableBytes) }), {
        duration: TOAST_FOR,
        action: {
          label: t("dataSettings.cleanup.reminderShow"),
          onClick: () => void navigate({ to: "/settings", search: { section: "data" } }),
        },
      });
    }, FIRST_CHECK);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [navigate, t]);
}
