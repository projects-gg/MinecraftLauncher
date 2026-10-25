// "Akış": what waits for the player first (approvals), then what the assistant did, newest first,
// grouped by day. Gold only where the player is waited for, jade a trade done, flare a suspicious
// message blocked; every other line is quiet text. New lines arrive from the live stream while
// the panel is on screen and fade in where they belong; the lines below glide.
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
import { Button, ContentIcon, type ContentIconKind, cn, Skeleton, useMotionPrefs } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import type { PlusApproval, PlusCatalog, PlusFeedItem } from "./api";
import { ApprovalCard } from "./approval-card";
import { GroupTitle } from "./fields";
import { itemName } from "./item-icon";
import { clockText, dayGroup, dayText } from "./time";
import { type LiveLink, plusKey, usePlusFeed } from "./use-plus";
import { currentLang, formatAmount, qtyItem, withCase } from "./words";

const ICON: Record<string, { icon: ReactNode; tone: string } | undefined> = {
  approval: { icon: <Timer size={15} weight="bold" />, tone: "text-gold-300" },
  trade_done: { icon: <CheckCircle size={15} weight="fill" />, tone: "text-jade-400" },
  suspicious: { icon: <ShieldWarning size={15} weight="fill" />, tone: "text-flare-400" },
  trade_failed: { icon: <XCircle size={15} />, tone: "text-ash-400" },
  trade_cancelled: { icon: <XCircle size={15} />, tone: "text-ash-400" },
  deal: { icon: <Handshake size={15} />, tone: "text-bone-100" },
  found: { icon: <MagnifyingGlass size={15} weight="bold" />, tone: "text-gold-300" },
  ai_stopped: { icon: <Pause size={15} />, tone: "text-ash-400" },
  ai_resumed: { icon: <Play size={15} />, tone: "text-ash-400" },
};

/** "1 Elytra, 37.500 (Kaan_07)" / "1 × Elytra, 37,500 (Kaan_07)": what a line is about. */
function dealWords(item: PlusFeedItem, catalog: PlusCatalog | null): string {
  const parts: string[] = [];
  if (item.item) parts.push(qtyItem(item.qty ?? 1, itemName(catalog, item.item)));
  if (item.price !== null) parts.push(formatAmount(item.price));
  const what = parts.join(", ");
  if (!item.counterpart) return what;
  return what ? `${what} (${item.counterpart})` : item.counterpart;
}

/** The first words of a line: what happened. */
function useLabel(item: PlusFeedItem): string {
  const { t } = useTranslation();
  if (item.kind === "approval_answer") {
    return t(`afkPlus.feed.answers.${item.reason ?? "approved"}`, {
      defaultValue: t("afkPlus.feed.kinds.approval_answer"),
    });
  }
  return t(`afkPlus.feed.kinds.${item.kind}`, { defaultValue: t("afkPlus.feed.kinds.other") });
}

function FeedRow({ item, catalog }: { item: PlusFeedItem; catalog: PlusCatalog | null }) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const label = useLabel(item);
  const lang = currentLang();
  const mark = ICON[item.kind];
  let body: ReactNode;
  if (item.kind === "pm_in" || item.kind === "pm_out") {
    const mine = item.kind === "pm_out";
    // "Asistan, Kaan_07'ye:" (never "Asistan (Kaan_07)", which read as an assistant of that name).
    const speaker = mine
      ? item.counterpart
        ? t("afkPlus.feed.assistantTo", { name: withCase(item.counterpart, "dat", lang) })
        : t("afkPlus.feed.assistant")
      : (item.counterpart ?? "");
    body = (
      <>
        <span className="font-medium text-bone-50">{speaker}:</span>{" "}
        <span className="text-bone-100 [overflow-wrap:anywhere]">{item.text}</span>
      </>
    );
  } else if (item.kind === "suspicious") {
    body = (
      <>
        <span className="font-medium text-bone-50">{label}</span>
        {item.reason ? (
          <span className="text-ash-300">
            {": "}
            {t(`afkPlus.feed.suspicious.${item.reason}`, {
              defaultValue: t("afkPlus.feed.suspicious.other"),
            })}
          </span>
        ) : null}
      </>
    );
  } else {
    const words = item.kind === "summary" ? (item.text ?? "") : dealWords(item, catalog);
    // A trade that did not happen says why, as the report's receipt does.
    const why =
      (item.kind === "trade_failed" || item.kind === "trade_cancelled") && item.reason
        ? t(`afkPlus.report.reasons.${item.reason}`, {
            defaultValue: t("afkPlus.report.reasons.other"),
          })
        : null;
    body = (
      <>
        <span
          className={cn("font-medium", item.kind === "approval" ? "text-gold-200" : "text-bone-50")}
        >
          {label}
        </span>
        {words ? <span className="text-bone-100 tabular-nums"> {words}</span> : null}
        {why ? <span className="block text-[12px] text-ash-400">{why}</span> : null}
      </>
    );
  }
  return (
    <motion.li
      layout={layout ? "position" : false}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: { duration: 0.24 } }}
      className="grid grid-cols-[44px_18px_minmax(0,1fr)] items-baseline gap-x-2 py-1.5 text-[13px] leading-snug"
    >
      <time className="text-[12px] text-ash-400 tabular-nums">{clockText(item.at, lang)}</time>
      <span aria-hidden className={cn("translate-y-[2px]", mark?.tone)}>
        {mark?.icon}
      </span>
      <p className="min-w-0 text-pretty">{body}</p>
    </motion.li>
  );
}

const SKELETON_WIDTHS = ["w-[72%]", "w-[88%]", "w-[64%]", "w-[80%]", "w-[70%]"];

function FeedSkeleton() {
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

export function FeedSection({
  accountId,
  approvals,
  catalog,
  link,
  machineAway,
  canLower = true,
}: {
  accountId: string;
  approvals: PlusApproval[];
  catalog: PlusCatalog | null;
  link: LiveLink;
  machineAway: boolean;
  /** The assistant runs, so it can haggle to a lower price (`ApprovalCard`). */
  canLower?: boolean;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const feed = usePlusFeed(accountId, true);
  const lang = currentLang();
  const now = Date.now();
  const queryClient = useQueryClient();
  // The one that runs out first comes first.
  const pending = approvals
    .filter((a) => a.state === "pending")
    .sort((a, b) => a.expiresAt - b.expiresAt);
  // A daily summary without its figures says nothing in the feed (its notification had them).
  const items = (feed.data?.items ?? []).filter((item) => item.kind !== "summary" || item.text);
  const away = machineAway || (feed.data?.away ?? false);
  const [retrying, setRetrying] = useState(false);
  const retry = async () => {
    setRetrying(true);
    try {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: plusKey(accountId) }),
        feed.refetch(),
      ]);
    } finally {
      setRetrying(false);
    }
  };

  const groups: Array<{ key: string; label: string; items: PlusFeedItem[] }> = [];
  for (const item of items) {
    const key = dayGroup(item.at, now);
    let group = groups.find((g) => g.key === key);
    if (!group) {
      const label =
        key === "today"
          ? t("afkPlus.feed.today")
          : key === "yesterday"
            ? t("afkPlus.feed.yesterday")
            : dayText(item.at, lang);
      group = { key, label, items: [] };
      groups.push(group);
    }
    group.items.push(item);
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
            <GroupTitle id="afk-plus-approvals">{t("afkPlus.feed.approvalsTitle")}</GroupTitle>
            <div className="flex flex-col gap-2.5">
              <AnimatePresence initial={false}>
                {pending.map((approval) => (
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
                      catalog={catalog}
                      canLower={canLower}
                    />
                  </motion.div>
                ))}
              </AnimatePresence>
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
                {t("afkPlus.feed.reconnecting")}
              </span>
            ) : null
          }
        >
          {t("afkPlus.feed.title")}
        </GroupTitle>
        {feed.isLoading ? (
          <FeedSkeleton />
        ) : away && !items.length ? (
          <QuietEmpty
            icon="server"
            action={
              <Button size="sm" variant="ghost" disabled={retrying} onClick={() => void retry()}>
                {t("afkPlus.feed.retry")}
              </Button>
            }
          >
            {/* The rail already says the assistant can't be reached: here, what still works. */}
            {t("afkPlus.feed.offlineBody")}
          </QuietEmpty>
        ) : !items.length ? (
          <QuietEmpty icon="chat">{t("afkPlus.feed.empty")}</QuietEmpty>
        ) : (
          <div className="flex flex-col gap-3">
            {groups.map((group) => (
              <div key={group.key}>
                <h4 className="mb-0.5 text-[12px] font-semibold text-ash-300">{group.label}</h4>
                <ul>
                  <AnimatePresence initial={false}>
                    {group.items.map((item) => (
                      <FeedRow key={item.id} item={item} catalog={catalog} />
                    ))}
                  </AnimatePresence>
                </ul>
              </div>
            ))}
            {feed.data?.next ? (
              <Button
                size="sm"
                variant="ghost"
                className="self-start"
                disabled={feed.loadingOlder}
                onClick={() => void feed.older()}
              >
                {t("afkPlus.feed.older")}
              </Button>
            ) : null}
          </div>
        )}
      </motion.section>
    </div>
  );
}
