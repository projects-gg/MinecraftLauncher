// "Rapor" (QoL Q4): last week for this account as a plain statement in three groups side by side
// (what was bought, what was sold, what the assistant did; label and figure, no metric cards), then
// the trades of the last 30 days from their receipts (afkplus-free.md §5.6:
// only completed deals and those under review are kept): what each item went for (count, average,
// lowest, highest; in money, or in the item that paid), every trade with its receipt opened under
// its row, and "Tabloyu indir" (a CSV in the Downloads folder, "Klasörde göster" in the toast).
// Buys and sales filter both lists.
import { CaretDown, DownloadSimple } from "@phosphor-icons/react";
import { Button, cn, enter, leave, Segmented, Skeleton, toast, useMotionPrefs } from "@pl/ui";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { formatPercent } from "@/lib/format";
import { afkFailed } from "../../errors";
import type { CloudAccount } from "../api";
import { type PlusReceipt, type PlusServer, type PlusWeeklyAccount, plusApi } from "./api";
import { QuietEmpty } from "./audit-section";
import { GroupTitle } from "./fields";
import { PlusItemIcon } from "./item-icon";
import { itemLabel, type Namer } from "./items";
import { moneyText } from "./orders";
import { clockText, dateTimeText, dayText, rangeText } from "./time";
import { usePlusFeatures } from "./use-plus";
import { cardWords, currentLang, formatAmount, goodsText, type Lang, qtyItem } from "./words";

type Filter = "all" | "buy" | "sell";

const DAY = 86_400;

/** A price as the receipt counts it: the server's money (its name when it is the account's), or an item. */
function priceText(
  price: number,
  pay: string,
  currency: string | null,
  namer: Namer,
  server: Pick<PlusServer, "money">,
  lang: Lang,
): string {
  if (pay !== "money") return qtyItem(price, itemLabel(pay, namer, lang), lang);
  return currency && currency === server.money.currency
    ? moneyText(price, server, lang)
    : formatAmount(price, lang);
}

/** "alındı" / "satıldı" after the item's name. */
function sideWord(t: TFunction, side: string): string {
  return t(`afkPlus.report.sides.${side === "sell" ? "sell" : "buy"}`);
}

function WeekStatement({
  week,
  server,
}: {
  week: PlusWeeklyAccount;
  server: Pick<PlusServer, "money">;
}) {
  const { t } = useTranslation();
  const lang = currentLang();
  const money = (value: number) => moneyText(value, server, lang);
  const blocked = week.blocked.scam + week.blocked.rmt + week.blocked.unsafe + week.blocked.fake;
  // Three groups: one long list paired a buy's figure with a sale's ("Takas 2 | Takas 1").
  const groups: Array<{ key: string; rows: Array<[string, string] | null> }> = [
    {
      key: "buy",
      rows: [
        [t("afkPlus.report.rows.bought"), String(week.bought)],
        [t("afkPlus.report.rows.spent"), money(week.spent)],
        week.belowLimit > 0 ? [t("afkPlus.report.rows.belowLimit"), money(week.belowLimit)] : null,
      ],
    },
    {
      key: "sell",
      rows: [
        [t("afkPlus.report.rows.sold"), String(week.sold)],
        [t("afkPlus.report.rows.earned"), money(week.earned)],
        week.aboveFloor > 0 ? [t("afkPlus.report.rows.aboveFloor"), money(week.aboveFloor)] : null,
      ],
    },
    {
      key: "assistant",
      rows: [
        [t("afkPlus.report.rows.pms"), String(week.pms)],
        [t("afkPlus.report.rows.blocked"), String(blocked)],
        [t("afkPlus.report.rows.allowance"), formatPercent(week.allowancePct)],
      ],
    },
  ];
  return (
    <div className="grid grid-cols-3 gap-x-6 rounded-[var(--radius-panel)] bg-white/[0.03] px-4 py-3">
      {groups.map((group) => (
        <section key={group.key} aria-label={t(`afkPlus.report.weekGroups.${group.key}`)}>
          <h4 className="mb-0.5 text-[12px] font-semibold text-ash-300">
            {t(`afkPlus.report.weekGroups.${group.key}`)}
          </h4>
          <dl>
            {group.rows.map((row) =>
              row ? (
                <div
                  key={row[0]}
                  className="flex items-baseline justify-between gap-3 border-b border-white/[0.05] py-1.5 last:border-b-0"
                >
                  <dt className="min-w-0 text-[12.5px] text-pretty text-ash-300">{row[0]}</dt>
                  <dd className="shrink-0 font-display text-[14px] font-semibold text-bone-50 tabular-nums">
                    {/* Tektur's % reads like a 7 ("%38" as "738"): the sign in the text face, as
                        OYNA does. */}
                    {row[1].split("%").map((part, index) => (
                      // biome-ignore lint/suspicious/noArrayIndexKey: the parts of one short value, in order.
                      <span key={index}>
                        {index > 0 ? <span className="font-sans">%</span> : null}
                        {part}
                      </span>
                    ))}
                  </dd>
                </div>
              ) : null,
            )}
          </dl>
        </section>
      ))}
    </div>
  );
}

/**
 * A trade's receipt, opened under its own row: six read-only lines need neither a dialog over the
 * panel nor the blur behind it. Which way it went is a noun under the item ("Satış").
 */
function ReceiptDetails({
  receipt,
  namer,
  server,
  id,
}: {
  receipt: PlusReceipt;
  namer: Namer;
  server: Pick<PlusServer, "money">;
  id: string;
}) {
  const { t } = useTranslation();
  const lang = currentLang();
  const at = Math.floor(receipt.at / 1000);
  // The amount in words from 10.000, as on the cards (a bare "bin" under "1.000" read as a million).
  const priceWords = receipt.pay === "money" ? cardWords(receipt.price, lang) : null;
  const rows: Array<[string, string, string?]> = [
    [t("afkPlus.report.time"), dateTimeText(at, lang)],
    [t("afkPlus.report.trader"), receipt.counterpart],
    [
      t("afkPlus.report.item"),
      goodsText(receipt.qty, itemLabel(receipt.item, namer, lang), lang),
      t(`afkPlus.report.sideNouns.${receipt.side === "sell" ? "sell" : "buy"}`),
    ],
    [
      t("afkPlus.report.amount"),
      priceText(receipt.price, receipt.pay, receipt.currency, namer, server, lang),
      priceWords ?? undefined,
    ],
    [
      t("afkPlus.report.status"),
      t(receipt.review ? "afkPlus.report.statuses.review" : "afkPlus.report.statuses.completed"),
      receipt.review ? t("afkPlus.report.reviewNote") : undefined,
    ],
    [
      t("afkPlus.report.approval"),
      t(
        `afkPlus.report.approvalKinds.${
          receipt.approval === "approved"
            ? "approved"
            : receipt.approval === "owner"
              ? "owner"
              : "auto"
        }`,
      ),
    ],
  ];
  return (
    // Named by the expanded button above it ("Makbuz").
    <div
      id={id}
      className="mt-2 mb-1 ml-[44px] rounded-[var(--radius-panel)] bg-white/[0.03] px-4 py-3"
    >
      <dl className="grid grid-cols-2 gap-x-6 gap-y-3">
        {rows.map(([label, value, note]) => (
          <div key={label} className="min-w-0">
            <dt className="text-[12px] text-ash-400">{label}</dt>
            <dd className="min-w-0">
              <span className="block text-[13.5px] text-bone-50 tabular-nums" data-selectable>
                {value}
              </span>
              {note ? <span className="block text-[12px] text-ash-400">{note}</span> : null}
            </dd>
          </div>
        ))}
      </dl>
      {/* A "makbuz" with a bare figure could read as real money. */}
      {receipt.pay === "money" ? (
        <p className="mt-3 text-[12px] text-ash-400">{t("afkPlus.report.receiptMoney")}</p>
      ) : null}
    </div>
  );
}

function ReceiptRow({
  receipt,
  namer,
  server,
  open,
  onToggle,
}: {
  receipt: PlusReceipt;
  namer: Namer;
  server: Pick<PlusServer, "money">;
  open: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const lang = currentLang();
  const at = Math.floor(receipt.at / 1000);
  const detailsId = `afk-plus-receipt-${receipt.at}-${receipt.counterpart}`;
  return (
    // The receipt fades in under the row and the rows below glide (design.md §5: never a height
    // animation).
    <motion.li layout={layout ? "position" : false} className="py-2">
      <div className="flex items-center gap-3">
        <PlusItemIcon item={receipt.item} custom={namer.custom} size={32} />
        <div className="min-w-0 flex-1">
          <p className="truncate text-[13.5px] text-bone-50">
            <span className="font-medium">
              {goodsText(receipt.qty, itemLabel(receipt.item, namer, lang), lang)}
            </span>
            <span className="text-ash-300"> {sideWord(t, receipt.side)}</span>
          </p>
          <p className="truncate text-[12px] text-ash-400 tabular-nums">
            {dayText(at, lang)} {clockText(at, lang)}, {receipt.counterpart}
            {receipt.review ? `, ${t("afkPlus.report.reviewShort")}` : ""}
          </p>
        </div>
        <span
          className={cn(
            "shrink-0 font-display text-[14px] font-semibold tabular-nums",
            receipt.review ? "text-ash-300" : "text-bone-50",
          )}
        >
          {priceText(receipt.price, receipt.pay, receipt.currency, namer, server, lang)}
        </span>
        <Button
          size="sm"
          variant="ghost"
          aria-expanded={open}
          aria-controls={open ? detailsId : undefined}
          iconRight={
            <CaretDown
              size={12}
              weight="bold"
              className={cn(
                "transition-transform duration-200 ease-out motion-reduce:transition-none",
                open && "rotate-180",
              )}
            />
          }
          onClick={onToggle}
        >
          {t("afkPlus.report.receipt")}
        </Button>
      </div>
      <AnimatePresence initial={false}>
        {open ? (
          <motion.div
            key="receipt"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.22) }}
            exit={{ opacity: 0, transition: leave(0.15) }}
          >
            <ReceiptDetails receipt={receipt} namer={namer} server={server} id={detailsId} />
          </motion.div>
        ) : null}
      </AnimatePresence>
    </motion.li>
  );
}

export function ReportSection({
  account,
  namer,
  server,
}: {
  account: CloudAccount;
  namer: Namer;
  server: Pick<PlusServer, "money">;
}) {
  const { t } = useTranslation();
  const features = usePlusFeatures();
  const lang = currentLang();
  const [filter, setFilter] = useState<Filter>("all");
  // One receipt open at a time: opening another closes the last.
  const [openId, setOpenId] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  // The last 30 days up to now, fixed while the section is on screen.
  const [from] = useState(() => Math.floor(Date.now() / 1000) - 30 * DAY);

  const report = useQuery({
    queryKey: ["afk-plus-report"],
    queryFn: () => plusApi.report(null),
    staleTime: 5 * 60_000,
    refetchOnWindowFocus: false,
    retry: false,
    enabled: features.report,
  });
  const summary = useQuery({
    queryKey: ["afk-plus", account.id, "summary", from],
    queryFn: () => plusApi.summary(account.id, from, null),
    staleTime: 60_000,
    refetchOnWindowFocus: false,
    retry: false,
  });
  const receipts = useInfiniteQuery({
    queryKey: ["afk-plus", account.id, "receipts"],
    queryFn: ({ pageParam }) => plusApi.receipts(account.id, pageParam),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next,
    staleTime: 60_000,
    refetchOnWindowFocus: false,
    retry: false,
  });

  const week = report.data?.accounts.find((one) => one.id === account.id) ?? null;
  const items = (summary.data?.items ?? []).filter(
    (one) => filter === "all" || one.side === filter,
  );
  const list = (receipts.data?.pages.flatMap((page) => page.receipts) ?? [])
    .filter((receipt) => receipt.at >= from * 1000)
    .filter((receipt) => filter === "all" || receipt.side === filter);
  const keyOf = (receipt: PlusReceipt) => `${receipt.at}-${receipt.counterpart}-${receipt.item}`;

  const download = async () => {
    setExporting(true);
    try {
      const file = await plusApi.exportTable(account.id, account.name, from, null);
      toast.success(t("afkPlus.report.downloaded"), {
        action: {
          label: t("afkPlus.report.showFile"),
          onClick: () =>
            void plusApi
              .showExport(file.fileName)
              .catch((error: unknown) => afkFailed(error, t("afkPlus.report.showFailed"))),
        },
      });
    } catch (error) {
      afkFailed(error, t("afkPlus.report.downloadFailed"));
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="flex flex-col gap-8">
      {features.report ? (
        <section aria-labelledby="afk-plus-week">
          <GroupTitle
            id="afk-plus-week"
            aside={
              report.data ? (
                <span className="text-[12.5px] text-ash-400 tabular-nums">
                  {/* AFK+ weeks run Monday 04:00 to Monday 04:00 (Turkey): the last day shown is
                      the Sunday its final hours belong to ("12–18 Ekim"). */}
                  {rangeText(report.data.from, report.data.to - 4 * 3600 - 1, lang)}
                </span>
              ) : null
            }
          >
            {t("afkPlus.report.weekTitle")}
          </GroupTitle>
          {report.isLoading ? (
            <Skeleton className="h-[186px] w-full" />
          ) : report.isError ? (
            <QuietEmpty icon="logs">{t("afkPlus.report.failed")}</QuietEmpty>
          ) : week && (week.pms || week.bought || week.sold) ? (
            <>
              <WeekStatement week={week} server={server} />
              {/* The week is what the assistant did (27 Oct 2026: it can make mistakes). */}
              <p className="mt-2 text-[12px] text-ash-400">{t("afkPlus.aiNote")}</p>
            </>
          ) : (
            <QuietEmpty icon="logs">{t("afkPlus.report.weekEmpty")}</QuietEmpty>
          )}
        </section>
      ) : null}

      <section aria-labelledby="afk-plus-trades">
        <GroupTitle
          id="afk-plus-trades"
          aside={
            <Button
              size="sm"
              variant="ghost"
              icon={<DownloadSimple size={15} />}
              disabled={exporting || !list.length}
              onClick={() => void download()}
            >
              {t("afkPlus.report.download")}
            </Button>
          }
        >
          {t("afkPlus.report.historyTitle")}
        </GroupTitle>
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <Segmented
            label={t("afkPlus.report.filterLabel")}
            value={filter}
            onValueChange={setFilter}
            options={(["all", "buy", "sell"] as const).map((value) => ({
              value,
              label: t(`afkPlus.report.filters.${value}`),
            }))}
          />
          <span className="text-[12.5px] text-ash-400">{t("afkPlus.report.period")}</span>
        </div>

        {items.length ? (
          <div className="mb-4 rounded-[var(--radius-panel)] bg-white/[0.03] px-4 py-1.5">
            {/* The three prices are each one item's, not the row's total: said once over them. */}
            <div className="grid grid-cols-[minmax(0,1fr)_64px_80px_80px_80px] gap-3 pt-1 text-[11.5px] text-ash-400">
              <span className="col-span-3 col-start-3 border-b border-white/[0.08] pb-1 text-center">
                {t("afkPlus.report.unitPrice")}
              </span>
            </div>
            <div className="grid grid-cols-[minmax(0,1fr)_64px_80px_80px_80px] gap-3 border-b border-white/[0.06] py-1.5 text-[11.5px] text-ash-400">
              <span>{t("afkPlus.report.item")}</span>
              <span className="text-right">{t("afkPlus.report.qty")}</span>
              <span className="text-right">{t("afkPlus.report.avg")}</span>
              <span className="text-right">{t("afkPlus.report.min")}</span>
              <span className="text-right">{t("afkPlus.report.max")}</span>
            </div>
            <ul>
              {items.map((one) => (
                <li
                  key={`${one.side}-${one.item}-${one.pay}`}
                  className="grid grid-cols-[minmax(0,1fr)_64px_80px_80px_80px] items-center gap-3 border-b border-white/[0.05] py-2 text-[13px] tabular-nums last:border-b-0"
                >
                  <span className="flex min-w-0 items-center gap-2.5">
                    <PlusItemIcon item={one.item} custom={namer.custom} size={24} />
                    <span className="min-w-0">
                      <span className="block truncate text-bone-50">
                        {itemLabel(one.item, namer, lang)}
                        <span className="text-ash-400"> {sideWord(t, one.side)}</span>
                      </span>
                      {one.pay !== "money" ? (
                        <span className="block truncate text-[11.5px] text-ash-400">
                          {t("afkPlus.report.paidWith", { item: itemLabel(one.pay, namer, lang) })}
                        </span>
                      ) : null}
                    </span>
                  </span>
                  <span className="text-right text-bone-100">{one.qty}</span>
                  <span className="text-right text-bone-50">{formatAmount(one.avgUnit)}</span>
                  <span className="text-right text-bone-100">{formatAmount(one.minUnit)}</span>
                  <span className="text-right text-bone-100">{formatAmount(one.maxUnit)}</span>
                </li>
              ))}
            </ul>
          </div>
        ) : null}

        {receipts.isLoading ? (
          <div className="space-y-2.5" aria-hidden>
            <Skeleton className="h-11 w-full" />
            <Skeleton className="h-11 w-full" />
            <Skeleton className="h-11 w-full" />
          </div>
        ) : receipts.isError ? (
          <QuietEmpty icon="logs">{t("afkPlus.report.failed")}</QuietEmpty>
        ) : list.length ? (
          <>
            {/* The rows re-measure together when a receipt opens or closes under one of them. */}
            <LayoutGroup id="afk-plus-receipts">
              <ul className="divide-y divide-white/[0.05]">
                {list.map((receipt) => (
                  <ReceiptRow
                    key={keyOf(receipt)}
                    receipt={receipt}
                    namer={namer}
                    server={server}
                    open={openId === keyOf(receipt)}
                    onToggle={() =>
                      setOpenId((last) => (last === keyOf(receipt) ? null : keyOf(receipt)))
                    }
                  />
                ))}
              </ul>
            </LayoutGroup>
            {receipts.hasNextPage ? (
              <Button
                size="sm"
                variant="ghost"
                className="mt-2"
                disabled={receipts.isFetchingNextPage}
                onClick={() => void receipts.fetchNextPage()}
              >
                {t("afkPlus.report.more")}
              </Button>
            ) : null}
          </>
        ) : (
          <QuietEmpty icon="pack">{t("afkPlus.report.empty")}</QuietEmpty>
        )}
      </section>
    </div>
  );
}
