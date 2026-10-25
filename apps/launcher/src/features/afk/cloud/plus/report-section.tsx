// "Rapor" (QoL Q4): last week for this account as a plain statement (label and figure, two columns;
// no metric cards), then the trade history of the last 30 days: what each item went for (count,
// average, lowest, highest), every trade with its receipt opened under its row, and "Tabloyu indir"
// (a CSV in the Downloads folder, "Klasörde göster" in the toast). Buys and sales filter both lists.
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
import { type PlusCatalog, type PlusDeal, type PlusWeeklyAccount, plusApi } from "./api";
import { QuietEmpty } from "./feed-section";
import { GroupTitle } from "./fields";
import { itemName, PlusItemIcon } from "./item-icon";
import { clockText, dateTimeText, dayText, rangeText } from "./time";
import { usePlusFeatures } from "./use-plus";
import { amountWords, currentLang, formatAmount, qtyItem } from "./words";

type Filter = "all" | "buy" | "sell";

const DAY = 86_400;

/**
 * "alındı" only for a trade that happened; one that fell through is "alınamadı" (its status says
 * why). Lower case after the item's name; `upper` for a line of its own.
 */
function sideWord(t: TFunction, deal: PlusDeal, upper = false): string {
  const side = deal.side === "sell" ? "sell" : "buy";
  const failed = deal.status === "failed" || deal.status === "cancelled";
  const word = t(failed ? `afkPlus.report.sidesFailed.${side}` : `afkPlus.report.sides.${side}`);
  return upper ? caseFirst(word, "upper") : word;
}

/** The first letter in the interface's own case rules ("İptal" ↔ "iptal"). */
function caseFirst(text: string, to: "upper" | "lower"): string {
  const lang = currentLang() === "tr" ? "tr" : "en";
  const first = text.charAt(0);
  return (
    (to === "upper" ? first.toLocaleUpperCase(lang) : first.toLocaleLowerCase(lang)) + text.slice(1)
  );
}

function WeekStatement({ week }: { week: PlusWeeklyAccount }) {
  const { t } = useTranslation();
  const blocked = week.blocked.scam + week.blocked.rmt + week.blocked.unsafe + week.blocked.fake;
  // Row by row: buys on the left, sales on the right, then the conversations.
  const rows: Array<[string, string]> = [
    [t("afkPlus.report.rows.bought"), String(week.bought)],
    [t("afkPlus.report.rows.sold"), String(week.sold)],
    [t("afkPlus.report.rows.spent"), formatAmount(week.spent)],
    [t("afkPlus.report.rows.earned"), formatAmount(week.earned)],
    [t("afkPlus.report.rows.belowLimit"), formatAmount(week.belowLimit)],
    [t("afkPlus.report.rows.aboveFloor"), formatAmount(week.aboveFloor)],
    [t("afkPlus.report.rows.pms"), String(week.pms)],
    [t("afkPlus.report.rows.blocked"), String(blocked)],
    [t("afkPlus.report.rows.allowance"), formatPercent(week.allowancePct)],
  ];
  return (
    <dl className="grid grid-cols-2 gap-x-8 rounded-[var(--radius-panel)] bg-white/[0.03] px-4 py-2">
      {rows.map(([label, value]) => (
        <div
          key={label}
          className="flex items-baseline justify-between gap-4 border-b border-white/[0.05] py-2 last:border-b-0 [&:nth-last-child(2)]:border-b-0"
        >
          <dt className="min-w-0 text-[12.5px] text-ash-300">{label}</dt>
          <dd className="shrink-0 font-display text-[14px] font-semibold text-bone-50 tabular-nums">
            {/* Tektur's % reads like a 7 ("%38" as "738"): the sign in the text face, as OYNA does. */}
            {value.split("%").map((part, index) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: the parts of one short value, in order.
              <span key={index}>
                {index > 0 ? <span className="font-sans">%</span> : null}
                {part}
              </span>
            ))}
          </dd>
        </div>
      ))}
    </dl>
  );
}

/**
 * A trade's receipt, opened under its own row: six read-only lines need neither a dialog over the
 * panel nor the blur behind it. Which way it went is a noun under the item ("Satış"); the outcome
 * is said once, in "Durum" ("Tamamlandı", or "Tamamlanamadı" with the reason).
 */
function ReceiptDetails({
  deal,
  catalog,
  id,
}: {
  deal: PlusDeal;
  catalog: PlusCatalog | null;
  id: string;
}) {
  const { t } = useTranslation();
  const lang = currentLang();
  const failed = deal.status === "failed" || deal.status === "cancelled";
  const rows: Array<[string, string, string?]> = [
    [t("afkPlus.report.time"), dateTimeText(deal.at, lang)],
    [t("afkPlus.report.trader"), deal.counterpart],
    [
      t("afkPlus.report.item"),
      qtyItem(deal.qty, itemName(catalog, deal.item), lang),
      t(`afkPlus.report.sideNouns.${deal.side === "sell" ? "sell" : "buy"}`),
    ],
    [t("afkPlus.report.amount"), formatAmount(deal.price), amountWords(deal.price, lang)],
    [
      t("afkPlus.report.status"),
      t(`afkPlus.report.statuses.${deal.status}`, {
        defaultValue: t("afkPlus.report.statuses.failed"),
      }),
      failed
        ? t(`afkPlus.report.reasons.${deal.reason ?? "other"}`, {
            defaultValue: t("afkPlus.report.reasons.other"),
          })
        : undefined,
    ],
    [
      t("afkPlus.report.approval"),
      t(`afkPlus.report.approvalKinds.${deal.approval === "approved" ? "approved" : "auto"}`),
    ],
  ];
  return (
    // Named by the expanded button above it ("Makbuz", or "Ayrıntı" for a trade that did not happen).
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
    </div>
  );
}

function DealRow({
  deal,
  catalog,
  open,
  onToggle,
}: {
  deal: PlusDeal;
  catalog: PlusCatalog | null;
  open: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const lang = currentLang();
  const done = deal.status === "completed";
  const detailsId = `afk-plus-receipt-${deal.id}`;
  return (
    // The receipt fades in under the row and the rows below glide (design.md §5: never a height
    // animation).
    <motion.li layout={layout ? "position" : false} className="py-2">
      <div className="flex items-center gap-3">
        <PlusItemIcon item={deal.item} size={32} className={cn(!done && "opacity-55")} />
        <div className="min-w-0 flex-1">
          <p className="truncate text-[13.5px] text-bone-50">
            <span className="font-medium">
              {qtyItem(deal.qty, itemName(catalog, deal.item), lang)}
            </span>
            <span className="text-ash-300"> {sideWord(t, deal)}</span>
          </p>
          <p className="truncate text-[12px] text-ash-400 tabular-nums">
            {dayText(deal.at, lang)} {clockText(deal.at, lang)}, {deal.counterpart}
            {/* A trade that did not happen says why ("süresi doldu"), not its status again. */}
            {done
              ? ""
              : `, ${
                  deal.reason
                    ? t(`afkPlus.report.reasonsShort.${deal.reason}`, {
                        defaultValue: t("afkPlus.report.reasonsShort.other"),
                      })
                    : caseFirst(
                        t(`afkPlus.report.statuses.${deal.status}`, {
                          defaultValue: t("afkPlus.report.statuses.failed"),
                        }),
                        "lower",
                      )
                }`}
          </p>
        </div>
        {/* A trade that did not happen keeps its amount quiet; struck through it read as a discount. */}
        <span
          className={cn(
            "shrink-0 font-display text-[14px] font-semibold tabular-nums",
            done ? "text-bone-50" : "text-ash-400",
          )}
        >
          {formatAmount(deal.price)}
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
          {t(done ? "afkPlus.report.receipt" : "afkPlus.report.details")}
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
            <ReceiptDetails deal={deal} catalog={catalog} id={detailsId} />
          </motion.div>
        ) : null}
      </AnimatePresence>
    </motion.li>
  );
}

export function ReportSection({
  account,
  catalog,
}: {
  account: CloudAccount;
  catalog: PlusCatalog | null;
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
  const deals = useInfiniteQuery({
    queryKey: ["afk-plus", account.id, "deals"],
    queryFn: ({ pageParam }) => plusApi.deals(account.id, pageParam),
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
  const list = (deals.data?.pages.flatMap((page) => page.deals) ?? [])
    .filter((deal) => deal.at >= from)
    .filter((deal) => filter === "all" || deal.side === filter);

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
            <WeekStatement week={week} />
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
                  key={`${one.side}-${one.item}`}
                  className="grid grid-cols-[minmax(0,1fr)_64px_80px_80px_80px] items-center gap-3 border-b border-white/[0.05] py-2 text-[13px] tabular-nums last:border-b-0"
                >
                  <span className="flex min-w-0 items-center gap-2.5">
                    <PlusItemIcon item={one.item} size={24} />
                    <span className="min-w-0 truncate text-bone-50">
                      {itemName(catalog, one.item)}
                      <span className="text-ash-400">
                        {" "}
                        {t(`afkPlus.report.sides.${one.side === "sell" ? "sell" : "buy"}`)}
                      </span>
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

        {deals.isLoading ? (
          <div className="space-y-2.5" aria-hidden>
            <Skeleton className="h-11 w-full" />
            <Skeleton className="h-11 w-full" />
            <Skeleton className="h-11 w-full" />
          </div>
        ) : deals.isError ? (
          <QuietEmpty icon="logs">{t("afkPlus.report.failed")}</QuietEmpty>
        ) : list.length ? (
          <>
            {/* The rows re-measure together when a receipt opens or closes under one of them. */}
            <LayoutGroup id="afk-plus-deals">
              <ul className="divide-y divide-white/[0.05]">
                {list.map((deal) => (
                  <DealRow
                    key={deal.id}
                    deal={deal}
                    catalog={catalog}
                    open={openId === deal.id}
                    onToggle={() => setOpenId((last) => (last === deal.id ? null : deal.id))}
                  />
                ))}
              </ul>
            </LayoutGroup>
            {deals.hasNextPage ? (
              <Button
                size="sm"
                variant="ghost"
                className="mt-2"
                disabled={deals.isFetchingNextPage}
                onClick={() => void deals.fetchNextPage()}
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
