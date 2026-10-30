// "Ayar geçmişi" (QoL Q7): the last saved versions of the AFK+ settings, newest first, each with
// when, where it was made and what it changed in words, every amount with its unit and no ending
// after a money's name ("Elitra: en fazla 45.000 ripple yerine 40.000 ripple"; the site's and the
// phone's same form). "Bu hâle dön" asks once in one sentence; the version comes back as a new one,
// so nothing is lost.
import { Button, cn, Dialog, enter, leave, Skeleton, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { afkFailed } from "../../errors";
import type { CloudAccount } from "../api";
import { type PlusHistoryEntry, type PlusPanel, plusApi } from "./api";
import { itemLabel, type Namer, usePlusItemData } from "./items";
import { type ConfigChange, configChanges, emptyLimit, type PlusLimits } from "./model";
import { moneyText } from "./orders";
import { clockText, dayGroup, dayText } from "./time";
import { plusKey } from "./use-plus";
import { currentLang, formatAmount, qtyItem } from "./words";

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
  const items = usePlusItemData(panel.settings.server.mc);
  const namer: Namer = {
    data: items.data,
    custom: panel.settings.config.customItems,
    unnamed: t("afkPlus.items.unnamed"),
  };
  const lang = currentLang();
  const server = panel.settings.server;
  return (change: ConfigChange): string => {
    const item = "item" in change ? itemLabel(change.item, namer) : "";
    switch (change.kind) {
      case "order_added":
        return t(
          change.side === "buy" ? "afkPlus.history.orderAdded" : "afkPlus.history.orderAddedSell",
          { item },
        );
      case "order_removed":
        return t(
          change.side === "buy"
            ? "afkPlus.history.orderRemoved"
            : "afkPlus.history.orderRemovedSell",
          { item },
        );
      case "order_amount": {
        // Every amount with its unit: the server's money, or the paying item's count.
        const unit = (value: number) =>
          change.field === "qty"
            ? formatAmount(value, lang)
            : change.pay && change.pay !== "money"
              ? qtyItem(formatAmount(value, lang), itemLabel(change.pay, namer), lang)
              : moneyText(value, server, lang);
        // A sell order's limit is its floor ("en az"), a buy order's its most ("en fazla").
        const field = change.field === "limit" && change.side === "sell" ? "floor" : change.field;
        return t("afkPlus.history.orderAmount", {
          item,
          field: t(`afkPlus.history.fields.${field}`),
          from: unit(change.from),
          to: unit(change.to),
        });
      }
      case "order_toggled":
        return t(change.on ? "afkPlus.history.orderOn" : "afkPlus.history.orderOff", { item });
      case "order_other":
        return t("afkPlus.history.orderOther", { item });
      case "authority":
        return t("afkPlus.history.authority", {
          to: t(`afkPlus.settings.authority.${change.to}`, {
            defaultValue: t("afkPlus.settings.authority.ask"),
          }),
        });
      case "limit": {
        const field = t(`afkPlus.settings.${LIMIT_LABEL[change.field]}`);
        // Every amount with its unit: the server's money, the minutes.
        const value = (amount: number) =>
          change.field === "approvalTimeoutMin"
            ? t("afkPlus.units.minutes", { count: amount })
            : moneyText(amount, server, lang);
        // An empty limit is never an amount, nor "sınırsız" where it means something else: it
        // says what it means (the site's and the phone's final lines): no buying without the
        // per-deal or the total limit, no question without an approval limit.
        if (change.to === null) {
          const empty = emptyLimit(change.field);
          const means =
            empty === "buysNothing"
              ? "NoBuying"
              : change.field === "approvalAbove"
                ? "NoAsk"
                : change.field === "newCounterpartApprovalAbove"
                  ? "NoAskNew"
                  : "";
          return t(`afkPlus.history.cleared${means}`, { field });
        }
        if (change.from === null) {
          return t("afkPlus.history.limitSet", { field, to: value(change.to) });
        }
        return t("afkPlus.history.limit", {
          field,
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
        ? t("afkPlus.audit.today")
        : group === "yesterday"
          ? t("afkPlus.audit.yesterday")
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
