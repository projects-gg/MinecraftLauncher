// Several cloud accounts at once (CAFK-FEAT §3.4): the bar under the chosen cards with Başlat,
// Durdur, Yeniden bağlan and Ayarları kopyala. Actions go one account after another, in the list's
// order, and end in one sentence ("3 hesap başlatıldı, 1 hesap başlatılamadı: süresi dolmuş."),
// worded as on the phone and the site.
import { ArrowsClockwise, Copy, Play, Stop } from "@phosphor-icons/react";
import { Button, toast } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ChangeBar } from "@/features/library/change-bar";
import type { AfkAccount } from "@/lib/api";
import { type CloudAccount, type CloudPageData, cloudApi } from "./api";
import { appliesTo, type BulkAction, errorCode, tally } from "./bulk";
import { CLOUD_KEY, storeAccount } from "./use-cloud";

/** Desktop states that mean the account is busy on this computer. */
function activeHere(account: AfkAccount): boolean {
  return !["stopped", "error", "clientFailed"].includes(account.state.kind);
}

/** One sentence for a bulk result: all went through, some did not, or none did. */
export function useBulkToast() {
  const { t } = useTranslation();
  return (action: BulkAction, results: Array<{ ok: boolean; code?: string | null }>) => {
    const { done, failed, reason } = tally(results);
    const why = t(`cloudAfkFeat.bulk.reasons.${reason ?? "other"}`);
    if (!failed) toast.success(t(`cloudAfkFeat.bulk.done.${action}`, { count: done }));
    else if (!done) toast.error(t(`cloudAfkFeat.bulk.none.${action}`, { reason: why }));
    else toast(t(`cloudAfkFeat.bulk.partial.${action}`, { done, failed, reason: why }));
  };
}

export function BulkBar({
  open,
  chosen,
  desktop,
  onCopy,
}: {
  open: boolean;
  /** The chosen accounts, in the list's order. */
  chosen: CloudAccount[];
  desktop: AfkAccount[];
  onCopy: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const report = useBulkToast();
  const [busy, setBusy] = useState<BulkAction | null>(null);

  const run = async (action: Exclude<BulkAction, "copy">) => {
    const targets = chosen.filter((account) => appliesTo(action, account));
    if (!targets.length || busy) return;
    setBusy(action);
    const data = queryClient.getQueryData<CloudPageData>(CLOUD_KEY);
    const results: Array<{ ok: boolean; code?: string | null }> = [];
    for (const account of targets) {
      // The same account running on this computer would keep kicking itself: left out.
      const twin = data?.twins.find((one) => one.cloudId === account.id);
      const here = twin ? desktop.find((one) => one.id === twin.desktopId) : undefined;
      if (action === "start" && here && activeHere(here)) {
        results.push({ ok: false, code: "here" });
        continue;
      }
      try {
        storeAccount(queryClient, await cloudApi[action](account.id));
        results.push({ ok: true });
      } catch (error) {
        results.push({ ok: false, code: errorCode(error) });
      }
    }
    setBusy(null);
    report(action, results);
    void queryClient.invalidateQueries({ queryKey: CLOUD_KEY });
  };

  const count = (action: Exclude<BulkAction, "copy">) =>
    chosen.filter((account) => appliesTo(action, account)).length;

  return (
    <ChangeBar
      open={open && chosen.length > 0}
      label={t("cloudAfkFeat.bulk.selected", { count: chosen.length })}
    >
      <p className="min-w-0 flex-1 truncate text-[13px] text-bone-100 tabular-nums">
        {t("cloudAfkFeat.bulk.selected", { count: chosen.length })}
      </p>
      <div className="flex shrink-0 items-center gap-1.5">
        <Button
          size="sm"
          variant="ghost"
          icon={<Play size={14} weight="fill" />}
          disabled={busy !== null || !count("start")}
          aria-busy={busy === "start" || undefined}
          onClick={() => void run("start")}
        >
          {t("cloudAfkFeat.bulk.actions.start")}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          icon={<Stop size={14} weight="fill" />}
          disabled={busy !== null || !count("stop")}
          aria-busy={busy === "stop" || undefined}
          onClick={() => void run("stop")}
        >
          {t("cloudAfkFeat.bulk.actions.stop")}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          icon={<ArrowsClockwise size={14} />}
          disabled={busy !== null || !count("restart")}
          aria-busy={busy === "restart" || undefined}
          onClick={() => void run("restart")}
        >
          {t("cloudAfkFeat.bulk.actions.restart")}
        </Button>
        <Button
          size="sm"
          variant="secondary"
          icon={<Copy size={14} />}
          disabled={busy !== null || chosen.length < 2}
          onClick={onCopy}
        >
          {t("cloudAfkFeat.bulk.actions.copy")}
        </Button>
      </div>
    </ChangeBar>
  );
}
