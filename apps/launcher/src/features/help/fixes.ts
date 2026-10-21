// The troubleshooter's one-tap fixes (HELP1). Each goes where the launcher already fixes that thing
// (the cleanup, the Java prompt's "Kur ve oyna", the mod repair, the crash dialog, the instance
// settings) or does it at once (memory back on "Otomatik"); what a fix changes is checked again,
// right away for the ones done in place, at the next visit for the ones done elsewhere.
import { toast } from "@pl/ui";
import type { QueryClient } from "@tanstack/react-query";
import { showError } from "@/features/content/jobs";
import { openMakeSpace } from "@/features/disk/make-space";
import { openModCenter } from "@/features/modcenter/notices";
import { useModFlow } from "@/features/modhealth/flow";
import { api, type HelpFix } from "@/lib/api";
import i18n from "@/lib/i18n";
import { openLink } from "@/lib/links";
import { useLaunch } from "@/stores/launch";
import { useHelp } from "./store";

export interface FixContext {
  /** The instance the game rows are about. */
  instanceId: string | null;
  /** The announcement's status page. */
  url?: string;
  queryClient: QueryClient;
}

function openInstanceSettings(instanceId: string) {
  void import("@/router").then(({ router }) =>
    router.navigate({
      to: "/library/$instanceId",
      params: { instanceId },
      search: { tab: "settings" },
    }),
  );
}

/** Runs a row's fix. Never throws: a fix that fails says so in a toast. */
export async function runFix(fix: HelpFix, ctx: FixContext): Promise<void> {
  const help = useHelp.getState();
  const id = ctx.instanceId;
  try {
    switch (fix) {
      case "makeSpace":
        help.invalidate();
        openMakeSpace();
        return;
      case "installJava":
        if (!id) return;
        help.invalidate();
        // The Java prompt's own way: switch to a Java that fits (downloaded if needed) and play;
        // the command bar shows the launch from there.
        void useLaunch.getState().play(id, { java: "install" });
        return;
      case "instanceSettings":
        if (!id) return;
        help.invalidate();
        openInstanceSettings(id);
        return;
      case "repair":
        // The repair's dialog opens over the panel; the panel checks again once it closes.
        if (id)
          useModFlow.getState().open(id, "center", { kind: "repair", scope: { kind: "all" } });
        return;
      case "showMods":
        if (!id) return;
        help.invalidate();
        openModCenter(id);
        return;
      case "crashDetails": {
        if (!id) return;
        const crash = await api.help.crash(id);
        if (!crash) {
          // Resolved meanwhile: the row says so after a new check.
          help.start(id);
          return;
        }
        useLaunch.setState({ crash });
        useModFlow.getState().open(id, "crash", { kind: "crash" });
        return;
      }
      case "memoryAuto": {
        if (!id) return;
        const detail = await api.instance(id);
        await api.updateInstanceSettings(id, { ...detail.settings, memoryAuto: true });
        void ctx.queryClient.invalidateQueries({ queryKey: ["instance", id] });
        toast.success(i18n.t("help.memorySet"));
        help.start(id);
        return;
      }
      case "clockSettings":
        help.invalidate();
        await api.help.openClock();
        return;
      case "statusPage":
        if (ctx.url) openLink(ctx.url);
        return;
    }
  } catch (error) {
    showError(
      error instanceof Error && error.message ? error : new Error(i18n.t("help.fixFailed")),
    );
  }
}
