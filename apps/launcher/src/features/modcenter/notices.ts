// Non-blocking notices about the mods that arrive during a launch (adopted changes in the mods
// folder, a Minecraft version the pack stopped offering). Details live in the "Modlar" tab; the
// toast only says what happened and links there.
import { toast } from "@pl/ui";
import type { ModsSection } from "@/features/library/instance-tab";
import type { ModsNotice } from "@/lib/api";
import i18n from "@/lib/i18n";

/**
 * Opens the instance page on its "Modlar" tab, on `section` when given (else where the player
 * left it). The router is loaded lazily: no import cycle.
 */
export function openModCenter(instanceId: string, section?: ModsSection) {
  void import("@/router").then(({ router }) =>
    router.navigate({
      to: "/library/$instanceId",
      params: { instanceId },
      search: { tab: "mods", focus: section },
    }),
  );
}

/** Label of a mod center job stage (applying, switching, moving mods). */
export function modStageLabel(stage: string | undefined): string {
  return i18n.t(`modCenter.stages.${stage ?? "resolving"}`, {
    defaultValue: i18n.t("modCenter.stages.resolving"),
  });
}

/** One sentence about one adopted change. */
export function describeChange(change: ModsNotice["changes"][number]): string {
  const name = change.title;
  if (
    change.requiredBy.length > 0 &&
    (change.change === "removed" || change.change === "disabled")
  ) {
    return i18n.t("modCenter.notice.required", { name, by: change.requiredBy.join(", ") });
  }
  return i18n.t(`modCenter.notice.${change.change}`, { name });
}

export function announceModsNotice(instanceId: string, notice: ModsNotice) {
  const only = notice.changes.length === 1 ? notice.changes[0] : undefined;
  const text = notice.reset
    ? i18n.t("modCenter.notice.reset")
    : only
      ? describeChange(only)
      : notice.changes.length > 1
        ? i18n.t("modCenter.notice.many", { count: notice.changes.length })
        : i18n.t("modCenter.notice.issues");
  toast(text, {
    duration: 9000,
    description:
      notice.issues > 0 && (notice.reset || notice.changes.length > 0)
        ? i18n.t("modCenter.notice.issuesToo")
        : undefined,
    action: {
      label: i18n.t("modCenter.notice.details"),
      // The adopted changes are told on the recommended mods' rows.
      onClick: () => openModCenter(instanceId, "recommended"),
    },
  });
}

export function announceVersionDropped(
  instanceId: string,
  minecraft: string,
  recommended: string | undefined,
) {
  toast(i18n.t("modCenter.dropped.toast", { minecraft }), {
    duration: 10_000,
    description: recommended
      ? i18n.t("modCenter.dropped.recommended", { version: recommended })
      : undefined,
    action: {
      label: i18n.t("modCenter.dropped.action"),
      // The recommended mods say which versions the pack offers and switch the version.
      onClick: () => openModCenter(instanceId, "recommended"),
    },
  });
}
