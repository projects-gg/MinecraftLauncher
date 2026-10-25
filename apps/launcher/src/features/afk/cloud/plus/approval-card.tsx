// A deal that waits for the player's OK (afkplus.md §1.4): the item, the amount in figures and in
// words, who it is with and why it asks, how long it waits; "Onayla", "Fiyatı düşür" (a buy: a
// lower ceiling the bot haggles to) and "Reddet". Money moves on "Onayla", so the price is the
// loudest thing on the card. The countdown changes once a minute, nothing moves in between.
import { Button, cn, toast, usePressGuard } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { afkFailed } from "../../errors";
import { type PlusApproval, type PlusCatalog, plusApi } from "./api";
import { AmountField } from "./fields";
import { itemName, PlusItemIcon } from "./item-icon";
import { minutesLeft } from "./time";
import { storeApproval } from "./use-plus";
import { amountWithCase, amountWords, currentLang, formatAmount, qtyItem, withCase } from "./words";

/** Minutes left, read again at each whole minute (never every second). */
function useMinutesLeft(expiresAt: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const left = expiresAt * 1000 - now;
    if (left <= 0) return;
    const wait = (left % 60_000 || 60_000) + 50;
    const timer = window.setTimeout(() => setNow(Date.now()), wait);
    return () => window.clearTimeout(timer);
  }, [expiresAt, now]);
  return minutesLeft(expiresAt, now);
}

export function ApprovalCard({
  accountId,
  approval,
  catalog,
  canLower = true,
}: {
  accountId: string;
  approval: PlusApproval;
  catalog: PlusCatalog | null;
  /**
   * The assistant can haggle now. While it is paused (its allowance used up, a break) approvals go
   * on without the AI: "Onayla" and "Reddet" work, "Fiyatı düşür" would need it to talk.
   */
  canLower?: boolean;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const left = useMinutesLeft(approval.expiresAt);
  const [busy, setBusy] = useState<"approve" | "reject" | "lower" | null>(null);
  const [lowering, setLowering] = useState(false);
  const [ceiling, setCeiling] = useState<number | null>(null);
  const [ceilingError, setCeilingError] = useState<string | null>(null);
  const guard = usePressGuard(lowering ? "lower" : "ask");
  const { deal } = approval;
  const lang = currentLang();
  const name = itemName(catalog, deal.item);
  const buy = deal.side === "buy";
  const why = approval.why[0] ?? "other";

  const answer = async (approve: boolean, maxPrice: number | null) => {
    setBusy(approve ? (maxPrice ? "lower" : "approve") : "reject");
    try {
      const outcome = await plusApi.answer(accountId, approval.id, approve, maxPrice);
      if (outcome.kind === "done") {
        storeApproval(queryClient, accountId, outcome.approval);
        toast.success(t(approve ? "afkPlus.approval.approved" : "afkPlus.approval.rejected"));
      } else {
        storeApproval(queryClient, accountId, {
          ...approval,
          state: outcome.state === "pending" ? "closed" : outcome.state,
        });
        toast(t("afkPlus.approval.closedMeanwhile"));
      }
    } catch (error) {
      afkFailed(error, t("cloudAfk.actions.failed"));
    } finally {
      setBusy(null);
    }
  };

  const sendLower = () => {
    if (ceiling === null || ceiling < 1 || ceiling >= deal.price) {
      setCeilingError(
        t("afkPlus.approval.lowerTooHigh", { price: amountWithCase(deal.price, "abl") }),
      );
      return;
    }
    void answer(true, ceiling);
  };

  const expired = left <= 0;
  return (
    <motion.article
      layout="position"
      aria-label={`${qtyItem(deal.qty, name, lang)}, ${formatAmount(deal.price)}`}
      className="rounded-[var(--radius-panel)] bg-obsidian-800/70 p-3.5 ring-1 ring-gold-400/22 ring-inset"
    >
      <div className="flex items-start gap-3">
        <PlusItemIcon item={deal.item} size={40} />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline justify-between gap-3">
            <h4 className="min-w-0 truncate font-display text-[15px] font-semibold text-bone-50">
              {qtyItem(deal.qty, name, lang)}
            </h4>
            <span
              className={cn(
                "shrink-0 text-[12.5px] tabular-nums",
                left <= 2 ? "text-gold-200" : "text-ash-300",
              )}
            >
              {expired
                ? t("afkPlus.approval.states.expired")
                : left <= 1
                  ? t("afkPlus.approval.lessThanMinute")
                  : t("afkPlus.approval.left", { count: left })}
            </span>
          </div>
          <p className="mt-1 font-display text-[19px] leading-tight font-semibold text-gold-200 tabular-nums">
            {formatAmount(deal.price)}
          </p>
          <p className="text-[12px] text-ash-400">{amountWords(deal.price, lang)}</p>
          <p className="mt-1.5 text-[12.5px] text-bone-100">
            {/* Which way the deal goes, with whom: "Kaan_07'den alış" / "MertK'ye satış". */}
            {buy
              ? t("afkPlus.approval.metaBuy", { from: withCase(approval.counterpart, "abl", lang) })
              : t("afkPlus.approval.metaSell", { to: withCase(approval.counterpart, "dat", lang) })}
          </p>
          <p className="text-[12.5px] text-ash-300">
            {t(`afkPlus.approval.why.${why}`, {
              defaultValue: t("afkPlus.approval.why.other"),
            })}
          </p>
        </div>
      </div>

      <AnimatePresence initial={false} mode="popLayout">
        {lowering ? (
          <motion.div
            key="lower"
            className="mt-3 flex flex-wrap items-start gap-2 border-t border-white/7 pt-3"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: { duration: 0.18 } }}
            exit={{ opacity: 0, transition: { duration: 0.12 } }}
          >
            <AmountField
              label={t("afkPlus.approval.lowerLabel")}
              value={ceiling}
              onChange={(value) => {
                setCeiling(value ?? null);
                setCeilingError(null);
              }}
              placeholder={t("afkPlus.approval.lowerPlaceholder", {
                price: formatAmount(deal.price),
              })}
              error={ceilingError}
              className="w-[220px]"
              autoFocus
            />
            <div className="flex gap-2 pt-[22px]">
              <Button
                size="sm"
                variant="primary"
                disabled={busy !== null || expired}
                onClick={sendLower}
              >
                {t("afkPlus.approval.lowerSend")}
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setLowering(false)}>
                {t("afkPlus.approval.cancel")}
              </Button>
            </div>
          </motion.div>
        ) : (
          <motion.div
            key="ask"
            className="mt-3 flex flex-wrap gap-2"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: { duration: 0.18 } }}
            exit={{ opacity: 0, transition: { duration: 0.12 } }}
          >
            <Button
              size="sm"
              variant="primary"
              disabled={busy !== null || expired}
              onClick={(event) => {
                if (guard.take(event)) void answer(true, null);
              }}
              {...guard.handlers}
            >
              {t("afkPlus.approval.approve")}
            </Button>
            {buy && canLower ? (
              <Button
                size="sm"
                variant="secondary"
                disabled={busy !== null || expired}
                onClick={() => setLowering(true)}
              >
                {t("afkPlus.approval.lower")}
              </Button>
            ) : null}
            <Button
              size="sm"
              variant="ghost"
              disabled={busy !== null || expired}
              onClick={() => void answer(false, null)}
            >
              {t("afkPlus.approval.reject")}
            </Button>
          </motion.div>
        )}
      </AnimatePresence>
    </motion.article>
  );
}
