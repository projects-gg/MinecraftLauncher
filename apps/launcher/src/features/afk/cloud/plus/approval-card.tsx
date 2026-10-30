// What waits for the player (README "AFK+ trade orders" → handing over and first sight), in the
// site's words and shape (FREE-WEB): the title says the deal in its notation ("1 Elytra = 37.500"),
// the lines under it who it is with ("Satıcı: Kaan_07") and each reason on its own line; the time
// left in the card's own tones, its last two minutes a step brighter. A decision about the
// player's money or items: no gold pushes either answer (the answer that goes ahead is the plate,
// the other stays quiet).
// - a deal the bot agreed and asks the OK for: "Onayla", "Reddet" and "Fiyatı düşür" (a buy for
//   money: a lower ceiling the bot haggles to, while the assistant runs);
// - "Takası sen yap": the deal is agreed and the player makes the trade (a server without a verified
//   profile, or an order the player trades): "Takas oldu" counts it (the player's word), "Olmadı"
//   lets the order go on; "Oyuna gir" stops the cloud bot and joins its server with its account;
// - a first sight: a custom item that had no fingerprint showed up in a trade window: "Evet, bu"
//   teaches the assistant the item, "Hayır" ends the deal.
// A closed one ("Kapananlar", the last ten minutes) says how it ended in place of its buttons. The
// countdown changes once a minute, nothing moves in between.
import { CheckCircle, ClockCountdown, XCircle } from "@phosphor-icons/react";
import { Button, cn, Dialog, toast, useMotionPrefs, usePressGuard } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type MouseEvent, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAccountsUi } from "@/features/accounts/store";
import { officialOrOffer } from "@/features/library/official";
import { gameBusy } from "@/features/social/join";
import { playInstance } from "@/features/social/play-together";
import { api } from "@/lib/api";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { afkFailed } from "../../errors";
import { type CloudAccount, cloudApi } from "../api";
import { storeAccount } from "../use-cloud";
import { type PlusApproval, type PlusServer, plusApi } from "./api";
import { markAnswered } from "./approval-clock";
import { AmountField } from "./fields";
import { ItemTile, PlusItemIcon } from "./item-icon";
import { itemLabel, type Namer } from "./items";
import { LimitLine } from "./limits-link";
import { limitReason } from "./model";
import { moneyText } from "./orders";
import { minutesLeft } from "./time";
import { storeApproval, usePlusUi } from "./use-plus";
import { amountWithCase, cardWords, currentLang, dealText, qtyItem } from "./words";

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

type T = (key: string, values?: Record<string, unknown>) => string;
type Kind = "deal" | "handoff" | "confirm_item";

/** The time an approval still waits, in the words of its kind ("Takası sen yap" waits a day). */
function leftWords(kind: Kind, left: number, t: T): string {
  if (kind === "handoff") {
    // Down to the hour: a deadline never reads later than it is.
    return left >= 60
      ? t("afkPlus.handoff.hoursLeft", { count: Math.floor(left / 60) })
      : t("afkPlus.handoff.minutesLeft", { count: Math.max(1, left) });
  }
  return left <= 1
    ? t("afkPlus.approval.lessThanMinute")
    : t("afkPlus.approval.left", { count: left });
}

/** How a closed approval ended, in a few words. */
function finalWords(
  approval: PlusApproval,
  kind: Kind,
  left: number,
  maintenance: boolean,
  money: (value: number) => string,
  t: T,
): string {
  if (approval.state === "approved") {
    if (kind === "handoff") return t("afkPlus.handoff.done");
    if (kind === "confirm_item") return t("afkPlus.firstSight.yesDone");
    // A lowered ceiling says what it replaced: the card's price is the one asked.
    return approval.maxPrice
      ? t("afkPlus.approval.loweredTo", {
          price: money(approval.maxPrice),
          asked: money(approval.deal.price),
        })
      : t("afkPlus.approval.states.approved");
  }
  // "Olmadı" lets the order go on and "Hayır" turns the item away: neither is a refused deal.
  if (approval.state === "rejected") {
    if (kind === "handoff") return t("afkPlus.handoff.notDone");
    if (kind === "confirm_item") return t("afkPlus.firstSight.noDone");
    return t("afkPlus.approval.states.rejected");
  }
  if (approval.state === "expired" || (approval.state === "pending" && left <= 0)) {
    return t("afkPlus.approval.states.expired");
  }
  if (approval.closed === "ai_stopped") {
    return t(
      maintenance
        ? "afkPlus.approval.states.closedService"
        : "afkPlus.approval.states.closedStopped",
    );
  }
  if (approval.closed === "deal_gone") return t("afkPlus.approval.states.closedGone");
  return t("afkPlus.approval.states.closed");
}

/**
 * "Oyuna gir" for a trade the player makes: one name cannot be in the game twice, so the cloud bot
 * stops, then the game joins the account's server with the launcher account of that name. The bot
 * stops only once the game can start: without that account the account center opens to add it,
 * with a game open the player closes it first, without an instance to play the official pack is
 * offered; the bot goes on meanwhile.
 */
export function useJoinForTrade(account: CloudAccount) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const proceed = async () => {
    setBusy(true);
    try {
      const accounts = await api.accounts();
      const mine = accounts.find(
        (candidate) => candidate.username.toLowerCase() === account.name.toLowerCase(),
      );
      if (!mine) {
        setOpen(false);
        toast(t("afkPlus.handoff.addAccount", { name: account.name }));
        useAccountsUi.getState().openCenter({ focus: "add" });
        return;
      }
      const launch = useLaunch.getState();
      if (gameBusy(launch.phase.kind)) {
        setOpen(false);
        toast(t("afkPlus.handoff.gameOpen"));
        return;
      }
      const address = { host: account.server.host, port: account.server.port || null };
      const prefs = usePrefs.getState();
      const instance =
        playInstance(await api.instances(), prefs.selectedInstanceId) ??
        (await officialOrOffer({ kind: "joinServer", ...address }))?.id;
      setOpen(false);
      if (!instance) return;
      const stopped = await cloudApi.stop(account.id);
      storeAccount(queryClient, stopped);
      prefs.selectInstance(instance);
      await launch.play(instance, {
        autoJoin: false,
        quickPlay: { kind: "multiplayer", ...address },
        account: { id: mine.id, name: mine.username },
      });
    } catch (error) {
      afkFailed(error, t("cloudAfk.actions.failed"));
    } finally {
      setBusy(false);
    }
  };
  const dialog = (
    <Dialog
      open={open}
      onOpenChange={(next) => !busy && setOpen(next)}
      title={t("afkPlus.handoff.joinTitle")}
      description={t("afkPlus.handoff.joinBody", { name: account.name })}
      closeLabel={t("common.close")}
      width={440}
      footer={
        <>
          <Button variant="ghost" onClick={() => setOpen(false)} disabled={busy}>
            {t("afkPlus.handoff.joinCancel")}
          </Button>
          <Button variant="primary" onClick={() => void proceed()} disabled={busy}>
            {t("afkPlus.handoff.joinGo")}
          </Button>
        </>
      }
    />
  );
  return { ask: () => setOpen(true), dialog };
}

export function ApprovalCard({
  accountId,
  approval,
  namer,
  server,
  canLower = true,
  maintenance = false,
  onJoin,
  compact = false,
}: {
  accountId: string;
  approval: PlusApproval;
  namer: Namer;
  /** The server's money: its name beside an amount when the profile has one. */
  server: Pick<PlusServer, "money">;
  /**
   * The assistant can haggle now. While it is paused (its allowance used up, a break) approvals go
   * on without the AI: "Onayla" and "Reddet" work, "Fiyatı düşür" waits for it.
   */
  canLower?: boolean;
  /** AFK+ is under maintenance (it closes waiting approvals): a closed card says that. */
  maintenance?: boolean;
  /** "Oyuna gir" of a handoff (the launcher's way to make the trade now). */
  onJoin?: () => void;
  /** Under its order's row: the item is the row's, the card leaves its picture out. */
  compact?: boolean;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const left = useMinutesLeft(approval.expiresAt);
  const [busy, setBusy] = useState<"approve" | "reject" | "lower" | null>(null);
  const [lowering, setLowering] = useState(false);
  const [ceiling, setCeiling] = useState<number | null>(null);
  const [ceilingError, setCeilingError] = useState<string | null>(null);
  const guard = usePressGuard(lowering ? "lower" : "ask");
  const goToLimit = usePlusUi((s) => s.goToLimit);
  // The card glides when the rows around it change, never in Performance mode.
  const { layout } = useMotionPrefs();
  const { deal } = approval;
  const lang = currentLang();
  const kind: Kind =
    approval.kind === "handoff" || approval.kind === "confirm_item" ? approval.kind : "deal";
  const pending = approval.state === "pending" && left > 0;
  const item = itemLabel(deal.item, namer, lang);
  const buy = deal.side === "buy";
  const forMoney = deal.pay === "money";
  const money = (value: number) => moneyText(value, server, lang);
  const price = forMoney
    ? money(deal.price)
    : qtyItem(deal.price, itemLabel(deal.pay, namer, lang), lang);
  const dealWords = dealText(deal.qty, item, price, lang);
  const priceWords = pending && kind === "deal" && forMoney ? cardWords(deal.price, lang) : null;
  // The other player as a label: the name takes no ending ("Satıcı: Kaan_07").
  const party = t(buy ? "afkPlus.approval.partyBuy" : "afkPlus.approval.partySell", {
    name: approval.counterpart,
  });
  const customName = approval.custom
    ? (namer.custom.find((c) => c.k === approval.custom?.k)?.name ?? namer.unnamed)
    : namer.unnamed;
  // A closed one is named by what it was about, never by a question no longer asked.
  const title =
    kind === "handoff"
      ? pending
        ? t("afkPlus.handoff.title")
        : dealWords
      : kind === "confirm_item"
        ? pending
          ? t("afkPlus.firstSight.question", { name: customName })
          : customName
        : dealWords;
  // Each reason on its own line (short phrases in a row read as one run-on line), each once.
  const lines = [
    ...new Set(
      kind === "handoff"
        ? [t("afkPlus.handoff.body", { who: approval.counterpart, deal: dealWords })]
        : kind === "confirm_item"
          ? [party]
          : [
              party,
              ...approval.why.map((reason) =>
                t(`afkPlus.approval.why.${reason}`, {
                  defaultValue: t("afkPlus.approval.why.other"),
                }),
              ),
            ],
    ),
  ];
  const lowerable = kind === "deal" && buy && forMoney;
  // The first reason that is a limit's ("Onay sınırının üstünde") carries "Sınırlara git", while it
  // waits and once closed: the limit is one click away.
  const limit = kind === "deal" ? limitReason(approval.why) : null;
  const limitLine = limit ? t(`afkPlus.approval.why.${limit.reason}`) : null;

  const answer = async (approve: boolean, maxPrice: number | null) => {
    setBusy(approve ? (maxPrice ? "lower" : "approve") : "reject");
    try {
      const outcome = await plusApi.answer(accountId, approval.id, approve, maxPrice);
      if (outcome.kind === "done") {
        markAnswered(approval.id);
        storeApproval(queryClient, accountId, outcome.approval);
        const done =
          kind === "handoff"
            ? approve
              ? "afkPlus.handoff.done"
              : "afkPlus.handoff.notDone"
            : kind === "confirm_item"
              ? approve
                ? "afkPlus.firstSight.yesDone"
                : "afkPlus.firstSight.noDone"
              : approve
                ? "afkPlus.approval.approved"
                : "afkPlus.approval.rejected";
        toast.success(t(done));
        setLowering(false);
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
        t("afkPlus.approval.lowerTooHigh", { price: amountWithCase(deal.price, "abl", lang) }),
      );
      return;
    }
    void answer(true, ceiling);
  };

  const press = (approve: boolean) => (event: MouseEvent) => {
    if (guard.take(event)) void answer(approve, null);
  };

  const yesLabel =
    kind === "handoff"
      ? t("afkPlus.handoff.yes")
      : kind === "confirm_item"
        ? t("afkPlus.firstSight.yes")
        : t("afkPlus.approval.approve");
  const noLabel =
    kind === "handoff"
      ? t("afkPlus.handoff.no")
      : kind === "confirm_item"
        ? t("afkPlus.firstSight.no")
        : t("afkPlus.approval.reject");

  return (
    <motion.article
      layout={layout ? "position" : false}
      aria-label={kind === "confirm_item" ? t("afkPlus.firstSight.title") : title}
      className={cn(
        "rounded-[var(--radius-panel)] bg-obsidian-800/70 p-3.5 ring-1 ring-inset",
        pending ? "ring-white/12" : "ring-white/7",
      )}
    >
      <div className="flex items-start gap-3">
        {compact || kind === "confirm_item" ? null : (
          <PlusItemIcon item={deal.item} custom={namer.custom} size={40} />
        )}
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline justify-between gap-3">
            <h4
              className={cn(
                "min-w-0 font-display text-[15px] font-semibold tabular-nums [overflow-wrap:anywhere]",
                // What closed reads a step quieter than what waits.
                pending ? "text-bone-50" : "text-bone-100",
              )}
            >
              {title}
            </h4>
            {pending ? (
              <span
                className={cn(
                  "flex shrink-0 items-center gap-1 text-[12.5px] tabular-nums",
                  left <= 2 ? "text-bone-50" : "text-ash-300",
                )}
              >
                <ClockCountdown size={14} aria-hidden className="translate-y-px" />
                {leftWords(kind, left, t)}
              </span>
            ) : null}
          </div>
          {/* The amount in words only while it waits for an answer about it (from 10.000). */}
          {priceWords ? <p className="text-[12px] text-ash-400">{priceWords}</p> : null}
          {lines.map((line) => (
            <LimitLine
              key={line}
              onGo={limit && line === limitLine ? () => goToLimit(limit.field) : null}
              approval={
                limit?.field === "approvalAbove" || limit?.field === "newCounterpartApprovalAbove"
              }
              className={cn(
                "mt-0.5 text-[12.5px] tabular-nums [overflow-wrap:anywhere]",
                pending ? "text-ash-300" : "text-ash-400",
              )}
            >
              {line}
            </LimitLine>
          ))}
          {kind === "confirm_item" && approval.seen ? (
            <div className="mt-2 flex items-start gap-3">
              <ItemTile
                id={approval.seen.id}
                cmd={approval.seen.cmd}
                model={approval.seen.model}
                size={40}
              />
              {/* The item in the window as the server wrote it (the counterpart's words, text
                  only), in the game's face like a chat line, so it reads as the game's text. */}
              <div className="min-w-0 flex-1 self-center rounded-[2px] bg-black/40 px-2 py-1 font-display text-[13px] leading-snug [text-shadow:1px_1px_0_rgb(0_0_0/0.55)]">
                <p className="truncate text-bone-50">{approval.seen.name ?? namer.unnamed}</p>
                {approval.seen.lore.map((line, index) => (
                  // biome-ignore lint/suspicious/noArrayIndexKey: the lines have no ids and never move
                  <p key={index} className="truncate text-ash-300">
                    {line}
                  </p>
                ))}
              </div>
            </div>
          ) : null}
        </div>
      </div>

      {pending ? (
        <AnimatePresence initial={false} mode="popLayout">
          {lowering ? (
            <motion.div
              key="lower"
              className="mt-3 border-t border-white/7 pt-3"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: { duration: 0.18 } }}
              exit={{ opacity: 0, transition: { duration: 0.12 } }}
            >
              <div className="flex flex-wrap items-start gap-2">
                <AmountField
                  label={t("afkPlus.approval.lowerLabel")}
                  value={ceiling}
                  onChange={(value) => {
                    setCeiling(value ?? null);
                    setCeilingError(null);
                  }}
                  placeholder={t("afkPlus.approval.lowerPlaceholder", {
                    price: amountWithCase(deal.price, "abl", lang),
                  })}
                  error={ceilingError}
                  className="w-[220px]"
                  autoFocus
                />
                <div className="flex gap-2 pt-[22px]">
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={busy !== null}
                    onClick={sendLower}
                  >
                    {t("afkPlus.approval.lowerSend")}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={busy !== null}
                    onClick={() => {
                      setLowering(false);
                      setCeiling(null);
                      setCeilingError(null);
                    }}
                  >
                    {t("afkPlus.approval.cancel")}
                  </Button>
                </div>
              </div>
              {ceilingError ? null : (
                <p className="mt-1 text-[12px] text-ash-400 [overflow-wrap:anywhere]">
                  {t("afkPlus.approval.lowerHelp", { name: approval.counterpart })}
                </p>
              )}
            </motion.div>
          ) : (
            <motion.div
              key="ask"
              className="mt-3 flex flex-col gap-2"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: { duration: 0.18 } }}
              exit={{ opacity: 0, transition: { duration: 0.12 } }}
            >
              <div className="flex flex-wrap gap-2">
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={busy !== null}
                  onClick={press(true)}
                  className="min-w-[96px]"
                  {...guard.handlers}
                >
                  {yesLabel}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={busy !== null}
                  onClick={() => void answer(false, null)}
                >
                  {noLabel}
                </Button>
                {lowerable ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={busy !== null || !canLower}
                    onClick={() => setLowering(true)}
                  >
                    {t("afkPlus.approval.lower")}
                  </Button>
                ) : null}
                {kind === "handoff" && onJoin ? (
                  <Button size="sm" variant="ghost" disabled={busy !== null} onClick={onJoin}>
                    {t("afkPlus.handoff.join")}
                  </Button>
                ) : null}
              </div>
              {lowerable && !canLower ? (
                <p className="text-[12px] text-ash-400">{t("afkPlus.approval.lowerLater")}</p>
              ) : null}
            </motion.div>
          )}
        </AnimatePresence>
      ) : (
        <p
          className={cn(
            "mt-2.5 flex items-center gap-1.5 text-[12.5px]",
            approval.state === "approved" ? "text-jade-400" : "text-ash-300",
          )}
        >
          {approval.state === "approved" ? (
            <CheckCircle size={15} weight="fill" aria-hidden />
          ) : (
            <XCircle size={15} aria-hidden />
          )}
          {finalWords(approval, kind, left, maintenance, money, t)}
        </p>
      )}
    </motion.article>
  );
}
