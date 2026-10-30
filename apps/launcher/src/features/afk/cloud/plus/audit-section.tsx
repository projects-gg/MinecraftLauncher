// "Etkinlik" (PLUS-ACTIVITY; product.md → Denetim kaydı; the site's and the phone's same place and
// words): what waits for the player's word first ("Seni bekleyenler": approvals running out, players
// whose message waits for an answer), then every step the assistant took, newest first, by day, a
// player's day in one row that opens in place to its steps and its conversation ("Cevap yaz"). The
// machine keeps the log 7 days and this computer keeps what it saw (pl-cloud-afk `plus_audit.rs`):
// it shows at once, the newer steps follow, the live stream adds the rest while the panel is on
// screen. No new read: the rows are the audit entries grouped here (activity.ts); a conversation is
// read live only while its row opens it. Colour only in a step's mark: gold where the player is
// waited for, jade a trade done, flare a step a safety rule stopped; no gold in the list itself.
import {
  CaretDown,
  ChatText,
  CheckCircle,
  Coins,
  HandCoins,
  Handshake,
  MagnifyingGlass,
  Pause,
  Play,
  ShieldWarning,
  Timer,
  X,
  XCircle,
} from "@phosphor-icons/react";
import {
  Button,
  cn,
  enter,
  IconButton,
  SearchInput,
  Segmented,
  Skeleton,
  useMotionPrefs,
} from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import type { CloudAccount } from "../api";
import {
  type ActivityGroup,
  type AuditFilter,
  activityGroups,
  inFilter,
  isMessage,
  KINDS,
  outcomeOf,
  rowFound,
  tradesToday,
  waitingThreads,
} from "./activity";
import type { PlusApproval, PlusAuditEntry, PlusInboxThread, PlusPanel, PlusServer } from "./api";
import { ApprovalCard } from "./approval-card";
import { closedLately, useApprovalNow, waitsNow } from "./approval-clock";
import { GroupTitle, QuietEmpty, QuietLink } from "./fields";
import { PlusItemIcon } from "./item-icon";
import { itemLabel, type Namer } from "./items";
import { Conversation } from "./messages-section";
import { moneyText } from "./orders";
import { clockText, dayGroup, dayText } from "./time";
import { plusAi } from "./types";
import { type LiveLink, plusKey, usePlusAudit, usePlusUi } from "./use-plus";
import { currentLang, dealText, formatAmount, type Lang, qtyItem } from "./words";

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
  // DONUT-ORDER's delivery to the server's order menu, and the chat ads stopped for the day.
  menu_done: { icon: <CheckCircle size={15} weight="fill" />, tone: "text-jade-400" },
  menu_fail: { icon: <XCircle size={15} />, tone: "text-ash-400" },
  ads_stopped: { icon: <Pause size={15} />, tone: "text-ash-400" },
};

/** A refusal for an unsafe message or an instruction attempt is a safety rule's work. */
function markOf(entry: PlusAuditEntry) {
  if (entry.k === "refuse" && (entry.r === "unsafe" || entry.r === "injection")) {
    return { icon: <ShieldWarning size={15} weight="fill" />, tone: "text-flare-400" };
  }
  return MARK[entry.k];
}

type T = (key: string, values?: Record<string, unknown>) => string;

/** Why a delivery to the server's order menu did not happen (`menu_fail`; afk-agent README). */
const MENU_REASONS = new Set([
  "mismatch",
  "window",
  "timeout",
  "unclear",
  "inventory",
  "gate",
  "item",
]);

/**
 * The words of a step (afkplus-free.md §5.4; the site's same lines, afk-agent README "Audit log"),
 * its text aside. A player's steps sit under their name (the row), so no line says it again.
 */
export function auditWords(
  entry: PlusAuditEntry,
  namer: Namer,
  server: Pick<PlusServer, "money">,
  t: T,
  lang: Lang,
): string {
  const l = (key: string, values?: Record<string, unknown>) =>
    t(`afkPlus.audit.lines.${key}`, values);
  const item = entry.i ? itemLabel(entry.i, namer, lang) : null;
  const price =
    entry.p == null
      ? null
      : entry.u && entry.u !== "money"
        ? qtyItem(formatAmount(entry.p, lang), itemLabel(entry.u, namer, lang), lang)
        : moneyText(entry.p, server, lang);
  const deal = item && price ? dealText(entry.q ?? 1, item, price, lang) : (item ?? price ?? "");
  // "64 Elmas": the item with its count when the entry has one (the order menu's lines).
  const counted = item && entry.q != null ? qtyItem(formatAmount(entry.q, lang), item, lang) : item;
  // An approval's steps name the deal when the entry carries it.
  const withDeal = (line: string) => (item && price ? l("withDeal", { line, deal }) : line);
  const reason =
    entry.r && REASONS.has(entry.r)
      ? t(`afkPlus.audit.reasons.${entry.r}`)
      : t("afkPlus.audit.reasons.other");
  const kind = KINDS.has(entry.k) ? entry.k : "other";
  switch (kind) {
    case "pm_in":
      return l("pm_in");
    case "offer":
    case "counter":
      return item ? l(kind, { p: price ?? "", i: item }) : l(`${kind}Bare`, { p: price ?? "" });
    case "agree":
      return l("agree", { deal });
    case "ask_owner":
      return item && price
        ? withDeal(l("ask_owner"))
        : item
          ? l("ask_ownerOf", { i: item })
          : l("ask_owner");
    case "owner_ok":
    case "owner_no":
    case "owner_timeout":
      return withDeal(l(kind));
    case "trade_done":
      return deal ? l("trade_done", { deal }) : l("trade_doneBare");
    case "trade_fail":
    case "cancel":
      return l(kind, { r: reason });
    case "refuse":
      return entry.w ? l("refuseWho", { r: reason }) : l("refuse", { r: reason });
    case "paused":
      return entry.r ? l("paused", { r: reason }) : l("pausedBare");
    case "found": {
      // Where it was seen (DONUT-ORDER): the server's order menu says a piece's price (`p` is the
      // whole for `q`), chat and a message the whole; an older agent's entry has no `r` and keeps
      // the old line.
      const what = counted ?? "";
      const whole = price ? `${what}, ${price}` : what;
      if (entry.r === "menu") {
        const each =
          entry.p != null && entry.q
            ? moneyText(Math.round(entry.p / entry.q), server, lang)
            : null;
        return each ? l("foundMenu", { what, each }) : l("foundMenuBare", { what });
      }
      if (entry.r === "chat") return l("foundChat", { what: whole });
      if (entry.r === "pm") return l("foundPm", { what: whole });
      return item && price ? l("found", { i: item, p: price }) : l("foundBare", { i: item ?? "" });
    }
    case "menu_done":
      return l("menu_done", { deal });
    case "menu_fail": {
      const why =
        entry.r && MENU_REASONS.has(entry.r)
          ? t(`afkPlus.audit.menuReasons.${entry.r}`)
          : t("afkPlus.audit.menuReasons.other");
      return counted ? l("menu_fail", { what: counted, r: why }) : l("menu_failBare", { r: why });
    }
    case "ads_stopped":
      return entry.r === "spam" ? l("ads_stopped") : l("ads_stoppedBare");
    case "confirm_item":
      return item ? l("confirm_itemOf", { i: item }) : l("confirm_item");
    case "order_done":
    case "order_expired":
      return item ? l(`${kind}Of`, { i: item }) : l(kind);
    // A safety stop says why when its reason says more than "güvenlik kuralı".
    case "gate":
      return entry.r && entry.r !== "gate" && REASONS.has(entry.r)
        ? l("gateWhy", { r: reason })
        : l("gate");
    default:
      return l(kind);
  }
}

/** A message step's own words: who said it ("Kaan_07 yazdı:", "ProBot AI cevap verdi:"). */
function saidWords(entry: PlusAuditEntry, words: string, t: T): string {
  if (entry.k === "pm_in") return words;
  return t(entry.k === "reply" ? "afkPlus.audit.lines.reply" : "afkPlus.audit.lines.owner_msg");
}

/** One step: its time, its mark and its words (a message's text folded when it was unsafe). */
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
  const lang = currentLang();
  const [shown, setShown] = useState(false);
  const mark = markOf(entry);
  const words = auditWords(entry, namer, server, t, lang);
  const said = entry.x != null && isMessage(entry.k);
  const folded = said && entry.h && !shown;
  return (
    // The time, the mark and the words' first line share one 18 px line box: the mark sits centred
    // on that line, level with the time (on baselines a 15 px mark rode high, the owner, 28 Oct).
    <li className="grid grid-cols-[44px_18px_minmax(0,1fr)] items-start gap-x-2 py-1.5 text-[13px] leading-[18px]">
      <time className="text-[12px] text-ash-400 tabular-nums">
        {clockText(Math.floor(entry.t / 1000), lang)}
      </time>
      <span aria-hidden className={cn("flex h-[18px] items-center", mark?.tone)}>
        {mark?.icon}
      </span>
      <p className="min-w-0 text-pretty">
        {said ? (
          <>
            <span className="font-medium text-bone-50">{saidWords(entry, words, t)}</span>{" "}
            {folded ? (
              <span className="text-ash-400">
                {t("afkPlus.audit.hidden")}{" "}
                <QuietLink tone="bone" onClick={() => setShown(true)}>
                  {t("afkPlus.audit.show")}
                </QuietLink>
              </span>
            ) : (
              <span className="text-bone-100 [overflow-wrap:anywhere]">"{entry.x}"</span>
            )}
          </>
        ) : (
          <span className="text-bone-100 tabular-nums">{words}</span>
        )}
      </p>
    </li>
  );
}

const SKELETON_WIDTHS = ["w-[72%]", "w-[88%]", "w-[64%]", "w-[80%]"];

function AuditSkeleton() {
  return (
    <div className="mt-2 space-y-2" aria-hidden>
      {SKELETON_WIDTHS.map((width) => (
        <div key={width} className="flex items-center gap-3 px-3.5 py-2">
          <Skeleton className="size-[32px] shrink-0" />
          <div className="flex-1 space-y-1.5">
            <Skeleton className="h-3.5 w-28" />
            <Skeleton className={cn("h-3", width)} />
          </div>
        </div>
      ))}
    </div>
  );
}

/**
 * A row of the list: a player's day (who, how it went: the most telling step, its message quoted;
 * when; the day's item in its tile), or an event without a player in the same layout, its words not
 * bold (the site's, TASTE-SWEEP r1: one layout in one list). A player's day opens in place: its steps
 * in order fade in (a 4 px rise, a fade alone under reduced motion; nothing grows), then the
 * conversation's "Cevap yaz" or the conversation. One step says all in the row itself: it opens only
 * to what comes under it, and with nothing there it stays a plain row (no step repeating the row).
 */
function ActivityRow({
  group,
  namer,
  server,
  open,
  onToggle,
  glide,
  children,
}: {
  group: ActivityGroup;
  namer: Namer;
  server: Pick<PlusServer, "money">;
  open: boolean;
  onToggle: () => void;
  /** The rows under one that opens glide to their place (off under reduced motion, while searching). */
  glide: boolean;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const lang = currentLang();
  const newest = group.entries[0] as PlusAuditEntry;
  const head = outcomeOf(group);
  const words = auditWords(head, namer, server, t, lang);
  const said = head.x != null && isMessage(head.k);
  const line = said ? saidWords(head, words, t) : words;
  const quoted = said && !head.h ? ` "${head.x}"` : "";
  const item = group.entries.find((entry) => entry.i)?.i ?? null;
  const mark = markOf(head);
  const bodyId = `afk-plus-activity-${newest.s}`;
  const single = group.entries.length === 1;
  const still = single && !children;
  const parts = (
    <>
      {item ? (
        <PlusItemIcon item={item} custom={namer.custom} size={32} />
      ) : (
        <span
          aria-hidden
          className={cn(
            "chamfer grid size-[32px] shrink-0 place-items-center bg-obsidian-800 [--cut:5px]",
            mark?.tone ?? "text-ash-400",
          )}
        >
          {/* A conversation's day (no deal, no item): the chat's mark. */}
          {mark?.icon ?? <ChatText size={15} />}
        </span>
      )}
      {group.who ? (
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[13.5px] leading-[1.35] font-semibold text-bone-50">
            {group.who}
          </span>
          <span className="block text-[12.5px] leading-snug text-pretty text-ash-300 [overflow-wrap:anywhere]">
            {line}
            {quoted}
          </span>
        </span>
      ) : (
        <span className="min-w-0 flex-1 text-[13px] leading-snug text-pretty text-bone-100 [overflow-wrap:anywhere]">
          {line}
        </span>
      )}
      <time
        className={cn(
          "shrink-0 text-[12px] text-ash-400 tabular-nums",
          group.who && "self-start pt-0.5",
        )}
      >
        {clockText(Math.floor(newest.t / 1000), lang)}
      </time>
    </>
  );
  const shell = {
    layout: glide ? ("position" as const) : false,
    transition: { duration: 0.3, ease: [0.16, 1, 0.3, 1] as const },
    // A row the stream or a filter brings in fades in; the first look shows at once.
    initial: { opacity: 0 },
    animate: { opacity: 1, transition: { duration: 0.24 } },
  };
  if (still) {
    return (
      <motion.li {...shell}>
        <div className="flex w-full items-center gap-3 px-3.5 py-2.5">
          {parts}
          {/* The caret's room, so every row's time lines up. */}
          <span aria-hidden className="w-[14px] shrink-0" />
        </div>
      </motion.li>
    );
  }
  return (
    <motion.li {...shell}>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={onToggle}
        className={cn(
          "flex w-full cursor-pointer items-center gap-3 rounded-[inherit] px-3.5 py-2.5 text-left outline-none",
          "transition-colors duration-150 hover:bg-white/[0.035] focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:ring-inset",
          open && "bg-white/[0.03]",
        )}
      >
        {parts}
        <CaretDown
          size={14}
          aria-hidden
          className={cn(
            "shrink-0 text-ash-400",
            !reduce && "transition-transform duration-200 ease-out",
            open && "rotate-180",
          )}
        />
      </button>
      {open ? (
        <motion.div
          id={bodyId}
          className="px-3.5 pt-0.5 pb-3"
          initial={{ opacity: 0, transform: reduce ? "none" : "translateY(-4px)" }}
          animate={{
            opacity: 1,
            transform: reduce ? "none" : "translateY(0px)",
            transition: enter(0.22),
          }}
        >
          {single ? null : (
            <ul className="flex flex-col border-t border-white/6 pt-1.5">
              {[...group.entries].reverse().map((entry) => (
                <AuditRow key={entry.s} entry={entry} namer={namer} server={server} />
              ))}
            </ul>
          )}
          {children ? (
            <div
              className={cn("flex flex-col", single ? "border-t border-white/6 pt-2.5" : "mt-2.5")}
            >
              {children}
            </div>
          ) : null}
        </motion.div>
      ) : null}
    </motion.li>
  );
}

/** A player whose message waits for the player's answer, among what waits: no gold, one action. */
function WaitingMessage({ thread, onReply }: { thread: PlusInboxThread; onReply: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-1 rounded-[var(--radius-panel)] bg-obsidian-900/70 px-3.5 py-3 ring-1 ring-white/8 ring-inset">
      <p className="text-[13.5px] font-semibold text-bone-50">
        {t("afkPlus.audit.wrote", { name: thread.name })}
      </p>
      <p className="text-[13px] leading-snug text-bone-100 [overflow-wrap:anywhere]">
        "{thread.last.text}"
      </p>
      <div className="mt-1">
        <QuietLink tone="bone" onClick={onReply}>
          {t("afkPlus.audit.reply")}
        </QuietLink>
      </div>
    </div>
  );
}

export function AuditSection({
  account,
  panel,
  namer,
  link,
  enabled,
  inboxOn,
  threads,
  canLower = true,
  maintenance = false,
  onJoin,
}: {
  account: CloudAccount;
  panel: PlusPanel;
  namer: Namer;
  link: LiveLink;
  /** Studio's `features.audit` (and the rules' audit switch). */
  enabled: boolean;
  /** Studio's `features.inbox`: the conversations, "Cevap yaz" and who waits for an answer. */
  inboxOn: boolean;
  /** The conversations' list (null while it loads or the machine is away). */
  threads: PlusInboxThread[] | null;
  /** The assistant runs, so it can haggle to a lower price (`ApprovalCard`). */
  canLower?: boolean;
  /** AFK+ is under maintenance (it closes waiting approvals). */
  maintenance?: boolean;
  /** "Oyuna gir" on a handoff. */
  onJoin?: () => void;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const accountId = account.id;
  const server = panel.settings.server;
  const approvals = panel.approvals;
  const audit = usePlusAudit(accountId, enabled);
  const lang = currentLang();
  const queryClient = useQueryClient();
  const thread = usePlusUi((s) => s.thread);
  const setThread = usePlusUi((s) => s.setThread);
  const [filter, setFilter] = useState<AuditFilter>("all");
  const [search, setSearch] = useState("");
  const [openRows, setOpenRows] = useState<ReadonlySet<string>>(() => new Set());
  // What still waits, the one that runs out first first; what closed in the last ten minutes on
  // its own under it (a closed card among the waiting ones would read as one still open).
  const nowSeconds = useApprovalNow(approvals);
  const pending = approvals
    .filter((a) => waitsNow(a, nowSeconds))
    .sort((a, b) => a.expiresAt - b.expiresAt);
  const closed = approvals
    .filter((a) => closedLately(a, nowSeconds))
    .sort((a, b) => b.expiresAt - a.expiresAt);
  const threadList = threads ?? [];
  const toAnswer = inboxOn ? waitingThreads(threadList) : [];
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
  const away = panel.machineAway || (audit.data?.away ?? false);
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

  // By day ("Bugün", "Dün", a date), then a player's day in one row.
  const now = Date.now();
  const days: Array<{ key: string; label: string; entries: PlusAuditEntry[] }> = [];
  for (const entry of items) {
    const at = Math.floor(entry.t / 1000);
    const key = dayGroup(at, now);
    const last = days.at(-1);
    if (last?.key === key) {
      last.entries.push(entry);
      continue;
    }
    const label =
      key === "today"
        ? t("afkPlus.audit.today")
        : key === "yesterday"
          ? t("afkPlus.audit.yesterday")
          : dayText(at, lang);
    days.push({ key, label, entries: [entry] });
  }
  // The search finds an item by its name in either language or an alias ("elytra" finds Elitra).
  const itemNames = (ref: string): string[] => {
    const info = namer.data?.byId.get(ref);
    return [
      itemLabel(ref, namer, lang),
      ...(info ? [info.name.tr, info.name.en, ...info.aliases] : []),
    ];
  };
  const shownDays = days
    .map((day) => ({
      ...day,
      rows: activityGroups(day.entries).filter((row) => rowFound(row, search, itemNames)),
    }))
    .filter((day) => day.rows.length > 0);
  // The open conversation's row: the newest day that has the player.
  const threadName = thread?.toLowerCase() ?? null;
  let threadRow: string | null = null;
  if (threadName) {
    for (const day of shownDays) {
      const row = day.rows.find((r) => r.who?.toLowerCase() === threadName);
      if (row) {
        threadRow = `${day.key}:${row.key}`;
        break;
      }
    }
  }
  const toggle = (key: string) => {
    if (key === threadRow) setThread(null);
    setOpenRows((old) => {
      const next = new Set(old);
      if (next.has(key) || key === threadRow) next.delete(key);
      else next.add(key);
      return next;
    });
  };
  const sameName = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
  const blocked = (name: string) =>
    threadList.some((th) => sameName(th.name, name) && th.state === "blocked");
  // The machine keeps a conversation with who wrote in the last 7 days; with it away nothing is live.
  const keepsOne = (name: string) =>
    inboxOn && !away && threadList.some((th) => sameName(th.name, name));
  // Under a player's steps: the conversation when it is open, else "Cevap yaz".
  const replyFor = (name: string): ReactNode =>
    threadName === name.toLowerCase() && inboxOn ? (
      <Conversation
        key={name}
        accountId={accountId}
        name={name}
        blocked={blocked(name)}
        autoFocus
      />
    ) : keepsOne(name) ? (
      <div>
        <QuietLink tone="bone" onClick={() => setThread(name)}>
          {t("afkPlus.audit.reply")}
        </QuietLink>
      </div>
    ) : null;

  // The slim line over the list (the owner, 29 Oct: more on one screen, never crowded): today's
  // trades and what was spent against the total limit (today's when there is none). The kontör is
  // the rail's alone (it stays in view beside this).
  const spent = panel.settings.spent ?? plusAi(account)?.spent ?? null;
  const total = panel.settings.config.limits.totalBudget;
  const spentLine = spent
    ? total
      ? t("afkPlus.audit.spentOf", {
          spent: formatAmount(spent.total, lang),
          total: moneyText(total, server, lang),
        })
      : spent.today > 0
        ? t("afkPlus.settings.spentToday", { amount: moneyText(spent.today, server, lang) })
        : null
    : null;
  const trades = tradesToday(all, now);
  // Not while the machine is away: its counts would be stale (the line over the list says so).
  const summary =
    audit.data && !off && !away ? (
      <ul
        aria-label={t("afkPlus.audit.summary")}
        className="flex flex-wrap gap-x-5 gap-y-1 text-[12.5px] text-ash-300 tabular-nums"
      >
        <li className="inline-flex items-center gap-1.5">
          <HandCoins size={15} aria-hidden className="shrink-0 text-ash-400" />
          {trades > 0
            ? t("afkPlus.audit.tradesToday", { count: trades })
            : t("afkPlus.audit.tradesNone")}
        </li>
        {spentLine ? (
          <li className="inline-flex items-center gap-1.5">
            <Coins size={15} aria-hidden className="shrink-0 text-ash-400" />
            {spentLine}
          </li>
        ) : null}
      </ul>
    ) : null;
  const glide = layout && !search.trim();

  return (
    <div className="flex flex-col gap-6">
      <AnimatePresence initial={false}>
        {pending.length || toAnswer.length ? (
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
              <AnimatePresence initial={false}>
                {pending.map(card)}
                {toAnswer.map((th) => (
                  <motion.div
                    key={`m:${th.name}`}
                    layout={layout ? "position" : false}
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1, transition: { duration: 0.22 } }}
                    exit={{ opacity: 0, transition: { duration: 0.15 } }}
                  >
                    <WaitingMessage thread={th} onReply={() => setThread(th.name)} />
                  </motion.div>
                ))}
              </AnimatePresence>
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
        {/* A conversation with no row in these days (or one the search hides): the conversation
            alone, under the player's name, with its own way to close. */}
        {thread && !threadRow && inboxOn ? (
          <motion.section
            key={`alone:${thread}`}
            layout={layout ? "position" : false}
            aria-labelledby="afk-plus-alone"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: { duration: 0.22 } }}
            exit={{ opacity: 0, transition: { duration: 0.15 } }}
            className="rounded-[var(--radius-panel)] bg-white/[0.03] px-3.5 pt-2.5 pb-3 ring-1 ring-white/7 ring-inset"
          >
            <div className="mb-1 flex items-center gap-2">
              <h3
                id="afk-plus-alone"
                className="min-w-0 flex-1 truncate font-display text-[15px] font-semibold text-bone-50"
              >
                {thread}
              </h3>
              <IconButton
                label={t("common.close")}
                icon={<X size={16} />}
                size="sm"
                onClick={() => setThread(null)}
              />
            </div>
            <Conversation accountId={accountId} name={thread} blocked={blocked(thread)} autoFocus />
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
          // Never started yet ("Daha sonra" in the setup): it cannot get a message, so say what
          // starts it; no slim line and no retention note over an empty log.
          <QuietEmpty icon="chat">
            {t(
              panel.settings.config.ready === true
                ? "afkPlus.audit.notStarted"
                : "afkPlus.audit.empty",
            )}
          </QuietEmpty>
        ) : (
          <div className="flex flex-col gap-3">
            {summary}
            <div className="flex flex-wrap items-center gap-2">
              <Segmented
                label={t("afkPlus.audit.filter")}
                value={filter}
                onValueChange={setFilter}
                options={[
                  { value: "all", label: t("afkPlus.audit.filters.all") },
                  { value: "deals", label: t("afkPlus.audit.filters.deals") },
                  { value: "messages", label: t("afkPlus.audit.filters.messages") },
                  { value: "warnings", label: t("afkPlus.audit.filters.warnings") },
                ]}
              />
              <SearchInput
                value={search}
                onChange={setSearch}
                placeholder={t("afkPlus.audit.search")}
                label={t("afkPlus.audit.search")}
                clearLabel={t("library.clearSearch")}
                className="min-w-[180px] flex-1"
              />
            </div>
            {away ? (
              <p role="status" className="text-[12.5px] text-ash-300">
                {t("afkPlus.audit.offline")}
              </p>
            ) : null}
            {!items.length ? (
              <p className="py-2 text-[13px] text-ash-300">{t("afkPlus.audit.noneHere")}</p>
            ) : !shownDays.length ? (
              <p className="py-2 text-[13px] text-ash-300">{t("afkPlus.audit.noMatch")}</p>
            ) : (
              shownDays.map((day) => (
                <div key={day.key}>
                  <h4 className="mb-1.5 text-[12px] font-semibold text-ash-300">{day.label}</h4>
                  <ul className="divide-y divide-white/6 rounded-[var(--radius-panel)] bg-white/[0.025] ring-1 ring-white/7 ring-inset">
                    <AnimatePresence initial={false}>
                      {day.rows.map((row) => {
                        const key = `${day.key}:${row.key}`;
                        return (
                          <ActivityRow
                            key={key}
                            group={row}
                            namer={namer}
                            server={server}
                            open={openRows.has(key) || key === threadRow}
                            onToggle={() => toggle(key)}
                            glide={glide}
                          >
                            {row.who ? replyFor(row.who) : null}
                          </ActivityRow>
                        );
                      })}
                    </AnimatePresence>
                  </ul>
                </div>
              ))
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
              {pending.length || toAnswer.length
                ? t("afkPlus.audit.kept")
                : `${t("afkPlus.audit.kept")} ${t("afkPlus.aiNote")}`}
            </p>
          </div>
        )}
      </motion.section>
    </div>
  );
}
