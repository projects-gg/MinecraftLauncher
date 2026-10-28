// "Akış" (afkplus-free.md §5): what waits for the player first (approvals), then the audit log,
// every step the assistant took and how it ended, newest first, by day. The machine keeps it 7 days
// and this computer keeps what it saw (pl-cloud-afk `plus_audit.rs`): it shows at once, the newer
// steps follow, the live stream adds the rest while the panel is on screen. Filters: Tümü,
// Anlaşmalar, Mesajlar, Uyarılar. Colour only in a line's icon: gold where the player is waited
// for, jade a trade done, flare a step a safety rule stopped; the words are quiet text. A line the
// service found unsuitable comes folded ("Göster"). Above it what waits for the player
// ("Seni bekleyenler") and, apart, what closed in the last ten minutes ("Kapananlar").
import {
  CheckCircle,
  Handshake,
  MagnifyingGlass,
  Pause,
  Play,
  ShieldWarning,
  Timer,
  XCircle,
} from "@phosphor-icons/react";
import {
  Button,
  ContentIcon,
  type ContentIconKind,
  cn,
  Segmented,
  Skeleton,
  useMotionPrefs,
} from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import type { PlusApproval, PlusAuditEntry, PlusServer } from "./api";
import { ApprovalCard } from "./approval-card";
import { closedLately, useApprovalNow, waitsNow } from "./approval-clock";
import { GroupTitle } from "./fields";
import { itemLabel, type Namer } from "./items";
import { moneyText } from "./orders";
import { clockText, dayGroup, dayText } from "./time";
import { type LiveLink, plusKey, usePlusAudit } from "./use-plus";
import { currentLang, dealText, type Lang, qtyItem } from "./words";

export type AuditFilter = "all" | "deals" | "messages" | "warnings";

const DEALS = new Set([
  "offer",
  "counter",
  "agree",
  "ask_owner",
  "owner_ok",
  "owner_no",
  "owner_timeout",
  "trade_start",
  "trade_done",
  "trade_fail",
  "handoff",
  "handoff_done",
  "handoff_no",
  "cancel",
  "found",
  "confirm_item",
  "order_done",
  "order_expired",
]);
const MESSAGES = new Set(["pm_in", "reply", "owner_msg"]);
// The contract's groups (README "AFK+ player API", AuditEntry): the site and the phone filter the
// same way; an entry may be in two.
const WARNINGS = new Set(["refuse", "gate", "quota", "paused", "resumed", "cancel", "trade_fail"]);

/** Whether a step shows under a filter. */
export function inFilter(kind: string, filter: AuditFilter): boolean {
  if (filter === "deals") return DEALS.has(kind);
  if (filter === "messages") return MESSAGES.has(kind);
  if (filter === "warnings") return WARNINGS.has(kind);
  return true;
}

/** The kinds the log knows; a newer one reads as a plain step. */
const KINDS = new Set([...DEALS, ...MESSAGES, ...WARNINGS, "resumed"]);

const REASONS = new Set([
  "blocked",
  "friends_only",
  "side_off",
  "pay_kind",
  "rules",
  "unsafe",
  "injection",
  "rate",
  "hours",
  "break",
  "quota",
  "limit",
  "budget",
  "price",
  "mismatch",
  "timeout",
  "far",
  "busy",
  "owner",
  "service",
  "gate",
  "not_looking",
]);

const MARK: Record<string, { icon: ReactNode; tone: string } | undefined> = {
  ask_owner: { icon: <Timer size={15} weight="bold" />, tone: "text-gold-300" },
  confirm_item: { icon: <Timer size={15} weight="bold" />, tone: "text-gold-300" },
  handoff: { icon: <Timer size={15} weight="bold" />, tone: "text-gold-300" },
  agree: { icon: <Handshake size={15} />, tone: "text-bone-100" },
  trade_done: { icon: <CheckCircle size={15} weight="fill" />, tone: "text-jade-400" },
  handoff_done: { icon: <CheckCircle size={15} weight="fill" />, tone: "text-jade-400" },
  order_done: { icon: <CheckCircle size={15} weight="fill" />, tone: "text-jade-400" },
  trade_fail: { icon: <XCircle size={15} />, tone: "text-ash-400" },
  handoff_no: { icon: <XCircle size={15} />, tone: "text-ash-400" },
  cancel: { icon: <XCircle size={15} />, tone: "text-ash-400" },
  gate: { icon: <ShieldWarning size={15} weight="fill" />, tone: "text-flare-400" },
  found: { icon: <MagnifyingGlass size={15} weight="bold" />, tone: "text-gold-300" },
  paused: { icon: <Pause size={15} />, tone: "text-ash-400" },
  resumed: { icon: <Play size={15} />, tone: "text-ash-400" },
};

/** A refusal for an unsafe message or an instruction attempt is a safety rule's work. */
function markOf(entry: PlusAuditEntry) {
  if (entry.k === "refuse" && (entry.r === "unsafe" || entry.r === "injection")) {
    return { icon: <ShieldWarning size={15} weight="fill" />, tone: "text-flare-400" };
  }
  return MARK[entry.k];
}

type T = (key: string, values?: Record<string, unknown>) => string;

/** The words of a step (afkplus-free.md §5.4), its text aside. */
export function auditWords(
  entry: PlusAuditEntry,
  namer: Namer,
  server: Pick<PlusServer, "money">,
  t: T,
  lang: Lang,
): string {
  const item = entry.i ? itemLabel(entry.i, namer, lang) : "";
  const price =
    entry.p == null
      ? ""
      : entry.u && entry.u !== "money"
        ? qtyItem(entry.p, itemLabel(entry.u, namer, lang), lang)
        : moneyText(entry.p, server, lang);
  const deal = item ? dealText(entry.q ?? 1, item, price, lang) : price;
  const reason =
    entry.r && REASONS.has(entry.r)
      ? t(`afkPlus.audit.reasons.${entry.r}`)
      : t("afkPlus.audit.reasons.other");
  const who = entry.w ?? t("afkPlus.audit.someone");
  const kind = KINDS.has(entry.k) ? entry.k : "other";
  switch (kind) {
    case "pm_in":
      return t("afkPlus.audit.lines.pm_in", { w: who });
    case "offer":
    case "counter":
      return t(`afkPlus.audit.lines.${kind}`, { w: who, p: price });
    case "agree":
      return t("afkPlus.audit.lines.agree", { w: who, deal });
    case "trade_done":
      return deal
        ? t("afkPlus.audit.lines.trade_done", { deal })
        : t("afkPlus.audit.lines.trade_doneBare");
    case "trade_fail":
    case "cancel":
      return t(`afkPlus.audit.lines.${kind}`, { r: reason });
    case "refuse":
      return entry.w
        ? t("afkPlus.audit.lines.refuseWho", { w: entry.w, r: reason })
        : t("afkPlus.audit.lines.refuse", { r: reason });
    case "paused":
      return entry.r
        ? t("afkPlus.audit.lines.paused", { r: reason })
        : t("afkPlus.audit.lines.pausedBare");
    case "found":
      return price
        ? t("afkPlus.audit.lines.found", { i: item, p: price })
        : t("afkPlus.audit.lines.foundBare", { i: item });
    case "confirm_item":
      return item
        ? t("afkPlus.audit.lines.confirm_itemOf", { i: item })
        : t("afkPlus.audit.lines.confirm_item");
    case "ask_owner":
      return item
        ? t("afkPlus.audit.lines.ask_ownerOf", { i: item })
        : t("afkPlus.audit.lines.ask_owner");
    // A safety stop says why when its reason says more than "güvenlik kuralı".
    case "gate":
      return entry.r && entry.r !== "gate" && REASONS.has(entry.r)
        ? t("afkPlus.audit.lines.gateWhy", { r: reason })
        : t("afkPlus.audit.lines.gate");
    default:
      return t(`afkPlus.audit.lines.${kind}`);
  }
}

function AuditRow({
  entry,
  namer,
  server,
}: {
  entry: PlusAuditEntry;
  namer: Namer;
  server: Pick<PlusServer, "money">;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const lang = currentLang();
  const [shown, setShown] = useState(false);
  const mark = markOf(entry);
  const words = auditWords(entry, namer, server, t, lang);
  const said = entry.x != null && MESSAGES.has(entry.k);
  const folded = said && entry.h && !shown;
  return (
    <motion.li
      layout={layout ? "position" : false}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: { duration: 0.24 } }}
      // The time, the mark and the words' first line share one 18 px line box: the mark sits centred
      // on that line, level with the time (on baselines a 15 px mark rode high, the owner, 28 Oct).
      className="grid grid-cols-[44px_18px_minmax(0,1fr)] items-start gap-x-2 py-1.5 text-[13px] leading-[18px]"
    >
      <time className="text-[12px] text-ash-400 tabular-nums">
        {clockText(Math.floor(entry.t / 1000), lang)}
      </time>
      <span aria-hidden className={cn("flex h-[18px] items-center", mark?.tone)}>
        {mark?.icon}
      </span>
      <p className="min-w-0 text-pretty">
        {said ? (
          <>
            <span className="font-medium text-bone-50">
              {entry.k === "pm_in"
                ? words
                : t(
                    entry.k === "reply"
                      ? "afkPlus.audit.lines.reply"
                      : "afkPlus.audit.lines.owner_msg",
                  )}
            </span>{" "}
            {folded ? (
              <span className="text-ash-400">
                {t("afkPlus.audit.hidden")}{" "}
                <button
                  type="button"
                  onClick={() => setShown(true)}
                  className="cursor-pointer rounded-sm font-medium text-gold-300 underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
                >
                  {t("afkPlus.audit.show")}
                </button>
              </span>
            ) : (
              <span className="text-bone-100 [overflow-wrap:anywhere]">"{entry.x}"</span>
            )}
          </>
        ) : (
          <span className="text-bone-100 tabular-nums">{words}</span>
        )}
      </p>
    </motion.li>
  );
}

const SKELETON_WIDTHS = ["w-[72%]", "w-[88%]", "w-[64%]", "w-[80%]", "w-[70%]"];

function AuditSkeleton() {
  return (
    <div className="space-y-3 pt-2" aria-hidden>
      {SKELETON_WIDTHS.map((width) => (
        <div key={width} className="grid grid-cols-[44px_18px_1fr] gap-x-2">
          <Skeleton className="h-3.5 w-9" />
          <span />
          <Skeleton className={cn("h-3.5", width)} />
        </div>
      ))}
    </div>
  );
}

/** A section with nothing in it: a quiet sprite, one sentence and, when it helps, one action. */
export function QuietEmpty({
  icon,
  children,
  action,
}: {
  icon: ContentIconKind;
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="flex items-center gap-3.5 rounded-[var(--radius-panel)] bg-white/[0.03] px-4 py-4">
      <span className="chamfer grid size-10 shrink-0 place-items-center bg-obsidian-800 [--cut:6px]">
        <ContentIcon kind={icon} size={16} quiet />
      </span>
      <p className="max-w-[52ch] min-w-0 flex-1 text-[13px] text-ash-300">{children}</p>
      {action ? <div className="shrink-0">{action}</div> : null}
    </div>
  );
}

export function AuditSection({
  accountId,
  approvals,
  namer,
  server,
  link,
  machineAway,
  enabled,
  canLower = true,
  maintenance = false,
  onJoin,
}: {
  accountId: string;
  approvals: PlusApproval[];
  namer: Namer;
  server: Pick<PlusServer, "money">;
  link: LiveLink;
  machineAway: boolean;
  /** Studio's `features.audit`. */
  enabled: boolean;
  /** The assistant runs, so it can haggle to a lower price (`ApprovalCard`). */
  canLower?: boolean;
  /** AFK+ is under maintenance (it closes waiting approvals). */
  maintenance?: boolean;
  /** "Oyuna gir" on a handoff. */
  onJoin?: () => void;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const audit = usePlusAudit(accountId, enabled);
  const lang = currentLang();
  const now = Date.now();
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState<AuditFilter>("all");
  // What still waits, the one that runs out first first; what closed in the last ten minutes on
  // its own under it (a closed card among the waiting ones would read as one still open).
  const nowSeconds = useApprovalNow(approvals);
  const pending = approvals
    .filter((a) => waitsNow(a, nowSeconds))
    .sort((a, b) => a.expiresAt - b.expiresAt);
  const closed = approvals
    .filter((a) => closedLately(a, nowSeconds))
    .sort((a, b) => b.expiresAt - a.expiresAt);
  const card = (approval: PlusApproval) => (
    <motion.div
      key={approval.id}
      layout={layout ? "position" : false}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: { duration: 0.22 } }}
      exit={{ opacity: 0, transition: { duration: 0.15 } }}
    >
      <ApprovalCard
        accountId={accountId}
        approval={approval}
        namer={namer}
        server={server}
        canLower={canLower}
        maintenance={maintenance}
        onJoin={onJoin}
      />
    </motion.div>
  );
  const all = audit.data?.items ?? [];
  const items = all.filter((entry) => inFilter(entry.k, filter));
  const away = machineAway || (audit.data?.away ?? false);
  const off = !enabled || (audit.data?.off ?? false);
  const [retrying, setRetrying] = useState(false);
  const retry = async () => {
    setRetrying(true);
    try {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: plusKey(accountId) }),
        audit.refetch(),
      ]);
    } finally {
      setRetrying(false);
    }
  };

  const groups: Array<{ key: string; label: string; items: PlusAuditEntry[] }> = [];
  for (const entry of items) {
    const at = Math.floor(entry.t / 1000);
    const key = dayGroup(at, now);
    let group = groups.find((g) => g.key === key);
    if (!group) {
      const label =
        key === "today"
          ? t("afkPlus.audit.today")
          : key === "yesterday"
            ? t("afkPlus.audit.yesterday")
            : dayText(at, lang);
      group = { key, label, items: [] };
      groups.push(group);
    }
    group.items.push(entry);
  }

  return (
    <div className="flex flex-col gap-6">
      <AnimatePresence initial={false}>
        {pending.length ? (
          <motion.section
            key="approvals"
            layout={layout ? "position" : false}
            aria-labelledby="afk-plus-approvals"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: { duration: 0.22 } }}
            exit={{ opacity: 0, transition: { duration: 0.15 } }}
          >
            <GroupTitle id="afk-plus-approvals">{t("afkPlus.audit.approvalsTitle")}</GroupTitle>
            <div className="flex flex-col gap-2.5">
              <AnimatePresence initial={false}>{pending.map(card)}</AnimatePresence>
            </div>
            {/* Once under what waits for the player: the assistant's deals and finds (27 Oct 2026). */}
            <p className="mt-2 text-[12px] text-ash-400">{t("afkPlus.aiNote")}</p>
          </motion.section>
        ) : null}
        {closed.length ? (
          <motion.section
            key="closed"
            layout={layout ? "position" : false}
            aria-labelledby="afk-plus-approvals-closed"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: { duration: 0.22 } }}
            exit={{ opacity: 0, transition: { duration: 0.15 } }}
          >
            <GroupTitle id="afk-plus-approvals-closed">
              {t("afkPlus.audit.approvalsRecent")}
            </GroupTitle>
            <div className="flex flex-col gap-2.5">
              <AnimatePresence initial={false}>{closed.map(card)}</AnimatePresence>
            </div>
          </motion.section>
        ) : null}
      </AnimatePresence>

      <motion.section layout={layout ? "position" : false} aria-labelledby="afk-plus-feed">
        <GroupTitle
          id="afk-plus-feed"
          aside={
            link === "reconnecting" ? (
              <span role="status" className="text-[12px] text-ash-400">
                {t("afkPlus.audit.reconnecting")}
              </span>
            ) : null
          }
        >
          {t("afkPlus.audit.title")}
        </GroupTitle>
        {off ? (
          <QuietEmpty icon="logs">{t("afkPlus.audit.off")}</QuietEmpty>
        ) : audit.isLoading ? (
          <AuditSkeleton />
        ) : away && !all.length ? (
          <QuietEmpty
            icon="server"
            action={
              <Button size="sm" variant="ghost" disabled={retrying} onClick={() => void retry()}>
                {t("afkPlus.audit.retry")}
              </Button>
            }
          >
            {t("afkPlus.audit.offlineEmpty")}
          </QuietEmpty>
        ) : !all.length ? (
          <QuietEmpty icon="chat">{t("afkPlus.audit.empty")}</QuietEmpty>
        ) : (
          <div className="flex flex-col gap-3">
            <Segmented
              label={t("afkPlus.audit.filter")}
              value={filter}
              onValueChange={setFilter}
              className="self-start"
              options={[
                { value: "all", label: t("afkPlus.audit.filters.all") },
                { value: "deals", label: t("afkPlus.audit.filters.deals") },
                { value: "messages", label: t("afkPlus.audit.filters.messages") },
                { value: "warnings", label: t("afkPlus.audit.filters.warnings") },
              ]}
            />
            {away ? (
              <p role="status" className="text-[12.5px] text-ash-300">
                {t("afkPlus.audit.offline")}
              </p>
            ) : null}
            {groups.length ? (
              groups.map((group) => (
                <div key={group.key}>
                  <h4 className="mb-0.5 text-[12px] font-semibold text-ash-300">{group.label}</h4>
                  <ul>
                    <AnimatePresence initial={false}>
                      {group.items.map((entry) => (
                        <AuditRow key={entry.s} entry={entry} namer={namer} server={server} />
                      ))}
                    </AnimatePresence>
                  </ul>
                </div>
              ))
            ) : (
              <p className="py-2 text-[13px] text-ash-300">{t("afkPlus.audit.noneHere")}</p>
            )}
            {audit.data?.more ? (
              <Button
                size="sm"
                variant="ghost"
                className="self-start"
                disabled={audit.loadingOlder}
                onClick={() => void audit.older()}
              >
                {t("afkPlus.audit.older")}
              </Button>
            ) : null}
            <p className="text-[12px] text-ash-400">
              {/* The AI note once a page: under what waits when something does. */}
              {pending.length
                ? t("afkPlus.audit.kept")
                : `${t("afkPlus.audit.kept")} ${t("afkPlus.aiNote")}`}
            </p>
          </div>
        )}
      </motion.section>
    </div>
  );
}
