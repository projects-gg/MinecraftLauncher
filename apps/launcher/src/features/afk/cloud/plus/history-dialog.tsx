// "Ayar geçmişi" (QoL Q7): the last saved versions of the AFK+ settings, newest first, each with
// when, where it was made and what it changed in words ("Elytra en fazla 40.000 yerine 45.000").
// "Bu hale dön" asks once in one sentence; the version comes back as a new one, so nothing is lost.
import { Button, cn, Dialog, enter, leave, Skeleton, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { afkFailed } from "../../errors";
import type { CloudAccount } from "../api";
import { type PlusHistoryEntry, type PlusPanel, plusApi } from "./api";
import { itemName } from "./item-icon";
import { type ConfigChange, configChanges, type PlusLimits } from "./model";
import { clockText, dayGroup, dayText } from "./time";
import { plusKey } from "./use-plus";
import { currentLang, formatAmount } from "./words";

const LIMIT_LABEL: Record<keyof PlusLimits, string> = {
  perDealMax: "perDealMax",
  dailySpendMax: "dailySpendMax",
  totalBudget: "totalBudget",
  balanceFloor: "balanceFloor",
  approvalAbove: "approvalAbove",
  newCounterpartApprovalAbove: "newCounterpart",
  approvalTimeoutMin: "timeoutShort",
};

/** One change in words. */
function useChangeText(panel: PlusPanel) {
  const { t } = useTranslation();
  return (change: ConfigChange): string => {
    const item = "item" in change ? itemName(panel.catalog, change.item) : "";
    switch (change.kind) {
      case "row_added":
        return t(
          change.side === "buy" ? "afkPlus.history.rowAdded" : "afkPlus.history.rowAddedSell",
          { item },
        );
      case "row_removed":
        return t(
          change.side === "buy" ? "afkPlus.history.rowRemoved" : "afkPlus.history.rowRemovedSell",
          {
            item,
          },
        );
      case "row_amount":
        return t("afkPlus.history.rowAmount", {
          item,
          field: t(`afkPlus.history.fields.${change.field}`),
          from: formatAmount(change.from),
          to: formatAmount(change.to),
        });
      case "row_toggled":
        return t(change.enabled ? "afkPlus.history.rowOn" : "afkPlus.history.rowOff", { item });
      case "row_other":
        return t("afkPlus.history.rowOther", { item });
      case "limit": {
        const value = (amount: number | null) =>
          amount === null
            ? t("afkPlus.history.noLimit")
            : change.field === "approvalTimeoutMin"
              ? t("afkPlus.units.minutes", { count: amount })
              : formatAmount(amount);
        return t("afkPlus.history.limit", {
          field: t(`afkPlus.settings.${LIMIT_LABEL[change.field]}`),
          from: value(change.from),
          to: value(change.to),
        });
      }
      default:
        return t(`afkPlus.history.${change.kind}`);
    }
  };
}

/** "Bugün 14:05", "Dün 09:12", "21 Ekim 18:40". */
function useWhen() {
  const { t } = useTranslation();
  const lang = currentLang();
  return (at: number) => {
    const group = dayGroup(at, Date.now());
    const day =
      group === "today"
        ? t("afkPlus.feed.today")
        : group === "yesterday"
          ? t("afkPlus.feed.yesterday")
          : dayText(at, lang);
    return `${day} ${clockText(at, lang)}`;
  };
}

function Entry({
  entry,
  older,
  current,
  panel,
  asking,
  busy,
  onAsk,
  onCancel,
  onRestore,
}: {
  entry: PlusHistoryEntry;
  older: PlusHistoryEntry | undefined;
  current: boolean;
  panel: PlusPanel;
  asking: boolean;
  busy: boolean;
  onAsk: () => void;
  onCancel: () => void;
  onRestore: () => void;
}) {
  const { t } = useTranslation();
  const when = useWhen();
  const text = useChangeText(panel);
  const changes = older ? configChanges(older.config, entry.config) : [];
  const shown = changes.slice(0, 3);
  const rest = changes.length - shown.length;
  return (
    <li
      className={cn(
        "rounded-[var(--radius-panel)] px-3.5 py-3",
        current ? "bg-white/[0.05] ring-1 ring-white/8 ring-inset" : "bg-white/[0.02]",
      )}
    >
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <p className="flex flex-wrap items-baseline gap-x-2 text-[13px]">
            <span className="font-medium text-bone-50 tabular-nums">{when(entry.at)}</span>
            <span className="text-ash-400">
              {t(`afkPlus.history.by.${entry.by}`, {
                defaultValue: t("afkPlus.history.by.player"),
              })}
            </span>
          </p>
          <ul className="mt-1 space-y-0.5 text-[12.5px] text-bone-100">
            {older ? (
              shown.length ? (
                shown.map((change, index) => (
                  // biome-ignore lint/suspicious/noArrayIndexKey: a fixed list of words for one version
                  <li key={index}>{text(change)}</li>
                ))
              ) : (
                <li className="text-ash-300">{t("afkPlus.history.same")}</li>
              )
            ) : (
              <li className="text-ash-300">{t("afkPlus.history.oldest")}</li>
            )}
            {rest > 0 ? (
              <li className="text-ash-400">{t("afkPlus.history.more", { count: rest })}</li>
            ) : null}
          </ul>
        </div>
        {current ? (
          <span className="shrink-0 pt-px text-[12.5px] text-ash-300">
            {t("afkPlus.history.current")}
          </span>
        ) : !asking ? (
          <Button size="sm" variant="ghost" className="shrink-0" onClick={onAsk}>
            {t("afkPlus.history.restore")}
          </Button>
        ) : null}
      </div>
      <AnimatePresence initial={false}>
        {asking ? (
          <motion.div
            key="ask"
            className="mt-2.5 flex flex-wrap items-center justify-between gap-3 border-t border-white/7 pt-2.5"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.18) }}
            exit={{ opacity: 0, transition: leave(0.12) }}
          >
            <p className="min-w-0 flex-1 text-[12.5px] text-bone-100">
              {t("afkPlus.history.restoreBody")}
            </p>
            <div className="flex gap-2">
              <Button size="sm" variant="ghost" onClick={onCancel} disabled={busy}>
                {t("afkPlus.settings.discard")}
              </Button>
              <Button size="sm" variant="primary" onClick={onRestore} disabled={busy}>
                {t("afkPlus.history.restore")}
              </Button>
            </div>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </li>
  );
}

export function HistoryDialog({
  open,
  onClose,
  account,
  panel,
}: {
  open: boolean;
  onClose: () => void;
  account: CloudAccount;
  panel: PlusPanel;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [asking, setAsking] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [conflict, setConflict] = useState(false);
  const query = useQuery({
    queryKey: ["afk-plus", account.id, "history"],
    queryFn: () => plusApi.history(account.id),
    enabled: open,
    staleTime: 0,
    refetchOnWindowFocus: false,
    retry: false,
  });
  const entries = [...(query.data?.entries ?? [])].sort((a, b) => b.aiRev - a.aiRev);
  const currentRev = panel.settings.aiRev;

  const restore = async (rev: number) => {
    setBusy(true);
    setConflict(false);
    try {
      const outcome = await plusApi.restore(account.id, rev, currentRev);
      if (outcome.kind === "saved") {
        queryClient.setQueryData<PlusPanel>(plusKey(account.id), (current) =>
          current ? { ...current, settings: outcome.settings } : current,
        );
        toast.success(t("afkPlus.history.restored"));
        setAsking(null);
        onClose();
      } else if (outcome.kind === "conflict") {
        queryClient.setQueryData<PlusPanel>(plusKey(account.id), (current) =>
          current ? { ...current, settings: outcome.settings } : current,
        );
        setConflict(true);
        setAsking(null);
        void query.refetch();
      } else {
        // The old version breaks a rule of today (a lower system cap): it cannot come back as it was.
        toast.error(t("afkPlus.history.invalidRestore"));
      }
    } catch (error) {
      afkFailed(error, t("afkPlus.history.failedRestore"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          setAsking(null);
          setConflict(false);
          onClose();
        }
      }}
      title={t("afkPlus.history.title")}
      closeLabel={t("common.close")}
      width={560}
    >
      {/* As tall as its versions (a short history is a short dialog), scrolling past 440 px. */}
      <div className="max-h-[min(440px,64vh)] overflow-y-auto pr-1 [scrollbar-width:thin]">
        {conflict ? (
          <p role="alert" className="mb-2 text-[12.5px] text-flare-300">
            {t("afkPlus.history.conflict")}
          </p>
        ) : null}
        {query.isLoading ? (
          <div className="space-y-2" aria-hidden>
            <Skeleton className="h-[62px] w-full" />
            <Skeleton className="h-[62px] w-full" />
            <Skeleton className="h-[62px] w-full" />
          </div>
        ) : query.isError ? (
          <div className="flex flex-col items-start gap-2 py-2">
            <p className="text-[13px] text-ash-300">{t("afkPlus.history.failed")}</p>
            <Button size="sm" variant="secondary" onClick={() => void query.refetch()}>
              {t("common.retry")}
            </Button>
          </div>
        ) : entries.length <= 1 ? (
          <p className="py-2 text-[13px] text-ash-300">{t("afkPlus.history.empty")}</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {entries.map((entry, index) => (
              <Entry
                key={entry.aiRev}
                entry={entry}
                older={entries[index + 1]}
                current={entry.aiRev === currentRev}
                panel={panel}
                asking={asking === entry.aiRev}
                busy={busy}
                onAsk={() => setAsking(entry.aiRev)}
                onCancel={() => setAsking(null)}
                onRestore={() => void restore(entry.aiRev)}
              />
            ))}
          </ul>
        )}
      </div>
    </Dialog>
  );
}
