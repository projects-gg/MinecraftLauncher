import {
  ArrowDown,
  CaretDown,
  CaretUp,
  Copy,
  FolderOpen,
  Info,
  ShareNetwork,
  WarningCircle,
} from "@phosphor-icons/react";
import { Button, cn, IconButton, SearchInput, Skeleton, Switch, toast } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  type KeyboardEvent,
  memo,
  type RefObject,
  useDeferredValue,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { copyErrorDetails, showError } from "@/features/content/jobs";
import { api, type GameLogSession, type LogLevel, type LogLine } from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { formatBytes, formatCount } from "@/lib/format";
import { InlineNotice, ToggleChip } from "../parts";
import { formatWhen, logFileWhen } from "../when";
import {
  appendLine,
  buildRows,
  findMatches,
  formatLine,
  type LevelGroup,
  type LogRow,
  levelGroup,
  levelGroups,
  levelRank,
  type Match,
  type RowPart,
  rowParts,
  withoutOverlap,
} from "./log-model";
import { gameLogKind, gameLogTitle } from "./log-sources";
import { ShareLogDialog } from "./share-log-dialog";

const ROW_HEIGHT = 20;
/** Characters shown per row; longer lines end with "…" (search and copy still see all). */
const MAX_ROW_CHARS = 4000;
/** Live sessions keep at most this many entries in memory. */
const MAX_LIVE_ENTRIES = 200_000;

const levelTone: Record<LogLevel, string> = {
  trace: "text-ash-400",
  debug: "text-ash-400",
  info: "text-ash-300",
  warn: "text-gold-300 font-semibold",
  error: "text-flare-400 font-semibold",
  fatal: "text-flare-400 font-bold",
};

const messageTone: Record<LogLevel, string> = {
  trace: "text-ash-400",
  debug: "text-ash-400",
  info: "text-bone-100",
  warn: "text-gold-200",
  error: "text-flare-400",
  fatal: "text-flare-400",
};

const rowTint: Record<LogLevel, string> = {
  trace: "",
  debug: "",
  info: "",
  warn: " bg-gold-400/[0.05]",
  error: " bg-flare-400/[0.07]",
  fatal: " bg-flare-400/[0.12]",
};

const groupTone: Record<LevelGroup, "error" | "warn" | "info" | "neutral"> = {
  error: "error",
  warn: "warn",
  info: "info",
  debug: "neutral",
};

/** Plain class strings (no merging): this runs for every visible row on every scroll. */
function partClass(part: RowPart, row: LogRow): string {
  if (part.hit === "current") return "rounded-[2px] bg-gold-400 text-gold-ink";
  if (part.hit === "hit") return "rounded-[2px] bg-gold-400/30 text-bone-50";
  if (part.tone === "dim") return "text-ash-400";
  if (part.tone === "level") return levelTone[row.level];
  if (!row.head && row.level === "info") return "text-ash-300";
  return messageTone[row.level];
}

const LogRowView = memo(function LogRowView({
  row,
  start,
  hits,
  hitLength,
  current,
}: {
  row: LogRow;
  start: number;
  hits: number[] | undefined;
  hitLength: number;
  current: number | undefined;
}) {
  const parts = rowParts(row, hits, hitLength, current, MAX_ROW_CHARS);
  return (
    <div
      className={`absolute top-0 left-0 w-full whitespace-pre px-3${rowTint[row.level]}`}
      style={{ height: ROW_HEIGHT, transform: `translateY(${start}px)` }}
    >
      {parts.map((part, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: parts have no identity
        <span key={i} className={partClass(part, row)}>
          {part.text}
        </span>
      ))}
    </div>
  );
});

export interface LogListHandle {
  scrollToRow: (row: number) => void;
  scrollToEnd: () => void;
}

/**
 * The virtualized rows (fixed height, so 50k+ lines scroll smoothly). Scrolling re-renders only
 * this list; the toolbar above stays put. Keeps the end in view while `follow` is on.
 */
function LogList({
  handle,
  rows,
  widest,
  hitsByRow,
  hitLength,
  current,
  follow,
  live,
  setFollowing,
  setUnseen,
  label,
}: {
  handle: RefObject<LogListHandle | null>;
  rows: LogRow[];
  widest: number;
  hitsByRow: Map<number, number[]>;
  hitLength: number;
  current: Match | undefined;
  follow: boolean;
  live: boolean;
  setFollowing: (follow: boolean) => void;
  setUnseen: (unseen: boolean) => void;
  label: string;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  });
  const count = rows.length;

  useImperativeHandle(
    handle,
    () => ({
      scrollToRow: (row) => virtualizer.scrollToIndex(row, { align: "center" }),
      scrollToEnd: () => {
        if (count > 0) virtualizer.scrollToIndex(count - 1, { align: "end" });
      },
    }),
    [virtualizer, count],
  );

  const scrolledOnce = useRef(false);
  const lastCount = useRef(0);
  const atBottom = useRef(true);
  useLayoutEffect(() => {
    const grew = count > lastCount.current;
    lastCount.current = count;
    if (count === 0) return;
    if (!scrolledOnce.current || follow) {
      // Open at the end: that is where the latest lines and crashes are.
      scrolledOnce.current = true;
      virtualizer.scrollToIndex(count - 1, { align: "end" });
    } else if (grew && live) {
      setUnseen(true);
    }
  }, [count, follow, live, virtualizer, setUnseen]);

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el || !live) return;
    const bottom = el.scrollHeight - el.scrollTop - el.clientHeight < ROW_HEIGHT * 2;
    if (bottom === atBottom.current) return;
    atBottom.current = bottom;
    // Scrolling up pauses following; scrolling back to the end resumes it.
    setFollowing(bottom);
    if (bottom) setUnseen(false);
  };

  return (
    <div
      ref={scrollRef}
      onScroll={onScroll}
      role="log"
      aria-label={label}
      // biome-ignore lint/a11y/noNoninteractiveTabindex: the log scrolls with the keyboard
      tabIndex={0}
      data-selectable
      className="min-h-0 flex-1 overflow-auto py-2 font-mono text-[12.5px] leading-5 [contain:strict] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300/60 focus-visible:ring-inset"
    >
      <div
        className="relative"
        style={{
          height: virtualizer.getTotalSize(),
          minWidth: `calc(${widest + 3}ch + 1.5rem)`,
        }}
      >
        {virtualizer.getVirtualItems().map((item) => (
          <LogRowView
            key={item.key}
            row={rows[item.index] as LogRow}
            start={item.start}
            hits={hitsByRow.get(item.index)}
            hitLength={hitLength}
            current={current?.row === item.index ? current.start : undefined}
          />
        ))}
      </div>
    </div>
  );
}

function LogSkeleton() {
  const widths = ["w-[72%]", "w-[58%]", "w-[83%]", "w-[46%]", "w-[66%]", "w-[90%]", "w-[52%]"];
  return (
    <div className="space-y-2 p-4" aria-hidden>
      {Array.from({ length: 16 }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: static placeholder rows
        <Skeleton key={i} className={cn("h-3.5", widths[i % widths.length])} />
      ))}
    </div>
  );
}

/**
 * One session's log: level colors, level chips and "only errors", search with highlight and
 * keyboard navigation, live follow for the running game, copy, reveal and share.
 */
export function LogViewer({
  instanceId,
  session,
}: {
  instanceId: string;
  session: GameLogSession;
}) {
  const { t } = useTranslation();
  const log = useQuery({
    // The game's own files change in place (latest.log): keyed by their last write too.
    queryKey: [
      "game-log",
      instanceId,
      session.source,
      session.fileName,
      session.source === "game" ? session.updatedAt : null,
    ],
    queryFn: () => api.library.readLog(instanceId, session.fileName, session.source),
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnWindowFocus: false,
  });

  // --- entries: the saved log plus, for the running game, lines that arrive live ----------
  const [entries, setEntries] = useState<LogLine[] | null>(null);
  const pending = useRef<LogLine[]>([]);
  const loaded = useRef(false);

  useEffect(() => {
    if (!log.data) return;
    const next = [...log.data.lines];
    for (const line of withoutOverlap(next, pending.current)) appendLine(next, line);
    pending.current = [];
    loaded.current = true;
    setEntries(next);
  }, [log.data]);

  useEffect(() => {
    if (!session.live) return;
    const queue: LogLine[] = [];
    let timer: ReturnType<typeof setTimeout> | undefined;
    const flush = () => {
      timer = undefined;
      const lines = queue.splice(0);
      if (!loaded.current) {
        pending.current.push(...lines);
        return;
      }
      setEntries((prev) => {
        const next = (prev ?? []).slice();
        for (const line of lines) appendLine(next, line);
        if (next.length > MAX_LIVE_ENTRIES) next.splice(0, next.length - MAX_LIVE_ENTRIES);
        return next;
      });
    };
    const unsubscribe = api.library.onGameLog((id, line) => {
      if (id !== instanceId) return;
      queue.push(line);
      timer ??= setTimeout(flush, 120);
    });
    return () => {
      unsubscribe();
      if (timer) clearTimeout(timer);
    };
  }, [instanceId, session.live]);

  // The game closed: read the finished log once more (it has everything now).
  const wasLive = useRef(session.live);
  const { refetch } = log;
  useEffect(() => {
    if (wasLive.current && !session.live) void refetch();
    wasLive.current = session.live;
  }, [session.live, refetch]);

  // --- filters, rows and search --------------------------------------------------------------
  const [enabled, setEnabled] = useState<Record<LevelGroup, boolean>>({
    error: true,
    warn: true,
    info: true,
    debug: true,
  });
  const [onlyErrors, setOnlyErrors] = useState(false);
  const [search, setSearch] = useState("");
  const query = useDeferredValue(search.trim());

  const counts = useMemo(() => {
    const out: Record<LevelGroup, number> = { error: 0, warn: 0, info: 0, debug: 0 };
    for (const line of entries ?? []) out[levelGroup(line.level)]++;
    return out;
  }, [entries]);

  const rows = useMemo(
    () =>
      buildRows(entries ?? [], (line) =>
        onlyErrors ? levelRank[line.level] >= levelRank.error : enabled[levelGroup(line.level)],
      ),
    [entries, onlyErrors, enabled],
  );
  const widest = useMemo(() => {
    let max = 0;
    for (const row of rows) if (row.text.length > max) max = row.text.length;
    return Math.min(max, MAX_ROW_CHARS + 1);
  }, [rows]);

  const matches = useMemo(() => findMatches(rows, query), [rows, query]);
  const hitsByRow = useMemo(() => {
    const map = new Map<number, number[]>();
    for (const m of matches) {
      const list = map.get(m.row);
      if (list) list.push(m.start);
      else map.set(m.row, [m.start]);
    }
    return map;
  }, [matches]);
  const [current, setCurrent] = useState(0);

  // --- scrolling and follow mode ------------------------------------------------------------
  const list = useRef<LogListHandle>(null);
  const searchRef = useRef<HTMLDivElement>(null);
  const [following, setFollowing] = useState(session.live);
  const [unseen, setUnseen] = useState(false);

  // A new search jumps to its first hit (only when the query changes, not when live lines add
  // hits).
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs for a new query only
  useEffect(() => {
    setCurrent(0);
    const first = matches[0];
    if (query && first) {
      setFollowing(false);
      list.current?.scrollToRow(first.row);
    }
  }, [query]);

  const go = (delta: number) => {
    if (matches.length === 0) return;
    const next = (current + delta + matches.length) % matches.length;
    setCurrent(next);
    setFollowing(false);
    const match = matches[next];
    if (match) list.current?.scrollToRow(match.row);
  };

  // Ctrl+F focuses the search field while the logs are open (not AltGr, which is Ctrl+Alt).
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "f") {
        e.preventDefault();
        searchRef.current?.querySelector("input")?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const onSearchKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      go(e.shiftKey ? -1 : 1);
    } else if (e.key === "Escape" && search) {
      e.preventDefault();
      e.stopPropagation();
      setSearch("");
    }
  };

  // --- actions ------------------------------------------------------------------------------
  const [sharing, setSharing] = useState(false);
  const copyAll = async () => {
    const text = (entries ?? []).map(formatLine).join("\n");
    try {
      await copyText(text);
      toast.success(t("instance.logs.copied"));
    } catch {
      toast.error(t("instance.logs.copyFailed"));
    }
  };
  const reveal = () =>
    void api.library
      .revealLog(instanceId, session.fileName, session.source)
      .catch((error) => showError(error));

  const filtered = onlyErrors || levelGroups.some((g) => !enabled[g]);
  const clearFilters = () => {
    setOnlyErrors(false);
    setEnabled({ error: true, warn: true, info: true, debug: true });
  };

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-3">
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <h3 className="truncate font-display text-[17px] font-semibold text-bone-50">
            {session.source === "game"
              ? gameLogTitle(session)
              : session.live
                ? t("instance.logs.liveTitle")
                : t("instance.logs.sessionTitle", {
                    when: formatWhen(session.startedAt ?? session.updatedAt),
                  })}
          </h3>
          <p className="flex gap-3 text-[12px] text-ash-400 tabular-nums">
            {/* A dated archive's title already names its day. */}
            {session.source === "game" &&
            !(gameLogKind(session.fileName).kind === "dated" && session.startedAt) ? (
              <span>{logFileWhen(session)}</span>
            ) : null}
            {entries ? (
              <span>{t("instance.logs.lineCount", { lines: formatCount(entries.length) })}</span>
            ) : null}
            <span>{formatBytes(log.data?.sizeBytes ?? session.sizeBytes)}</span>
          </p>
        </div>
        {session.live ? (
          <Switch
            checked={following}
            onCheckedChange={(on) => {
              setFollowing(on);
              setUnseen(false);
              if (on) list.current?.scrollToEnd();
            }}
            label={t("instance.logs.follow")}
            className="mr-2"
          />
        ) : null}
        <IconButton
          label={t("instance.logs.copyAll")}
          icon={<Copy size={18} />}
          disabled={!entries?.length}
          onClick={() => void copyAll()}
        />
        <IconButton
          label={t("instance.logs.reveal")}
          icon={<FolderOpen size={18} />}
          onClick={reveal}
        />
        <Button
          variant="secondary"
          icon={<ShareNetwork size={16} className="text-gold-300" />}
          disabled={!entries?.length}
          onClick={() => setSharing(true)}
        >
          {t("instance.logs.share.action")}
        </Button>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        {/* biome-ignore lint/a11y/noStaticElementInteractions: keys bubble up from the search field */}
        <div ref={searchRef} onKeyDown={onSearchKey} className="w-60">
          <SearchInput
            value={search}
            onChange={setSearch}
            placeholder={t("instance.logs.search")}
            label={t("instance.logs.search")}
          />
        </div>
        {query ? (
          <div className="flex items-center gap-0.5">
            <span
              className="min-w-14 px-1.5 text-center text-[12px] text-ash-400 tabular-nums"
              aria-live="polite"
            >
              {matches.length === 0
                ? t("instance.logs.noHits")
                : `${formatCount(current + 1)}/${formatCount(matches.length)}${matches.length >= 5000 ? "+" : ""}`}
            </span>
            <IconButton
              label={t("instance.logs.prevHit")}
              icon={<CaretUp size={16} />}
              size="sm"
              disabled={matches.length === 0}
              onClick={() => go(-1)}
            />
            <IconButton
              label={t("instance.logs.nextHit")}
              icon={<CaretDown size={16} />}
              size="sm"
              disabled={matches.length === 0}
              onClick={() => go(1)}
            />
          </div>
        ) : null}
        <span className="flex-1" />
        <fieldset
          className="flex min-w-0 items-center gap-1"
          aria-label={t("instance.logs.levels")}
        >
          {levelGroups.map((group) => (
            <ToggleChip
              key={group}
              tone={groupTone[group]}
              pressed={onlyErrors ? group === "error" : enabled[group]}
              disabled={onlyErrors}
              onPressedChange={(pressed) => setEnabled((prev) => ({ ...prev, [group]: pressed }))}
            >
              {t(`instance.logs.level.${group}`)}
              <span className="font-medium tabular-nums">{formatCount(counts[group])}</span>
            </ToggleChip>
          ))}
        </fieldset>
        <Switch
          checked={onlyErrors}
          onCheckedChange={setOnlyErrors}
          label={t("instance.logs.onlyErrors")}
          className="ml-2"
        />
      </div>

      {log.data?.truncated ? (
        <InlineNotice icon={<Info size={16} className="text-ash-300" />}>
          {t("instance.logs.truncated")}
        </InlineNotice>
      ) : null}

      <div className="glass relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-[var(--radius-panel)]">
        {log.isError ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
            <WarningCircle size={28} className="text-flare-400" />
            <p className="max-w-[46ch] text-[13.5px] text-bone-100">
              {log.error instanceof Error && log.error.message
                ? log.error.message
                : t("instance.logs.readFailed")}
            </p>
            <div className="flex gap-2">
              <Button variant="ghost" size="sm" onClick={() => void copyErrorDetails(log.error)}>
                {t("common.copyDetails")}
              </Button>
              <Button variant="secondary" size="sm" onClick={() => void log.refetch()}>
                {t("common.retry")}
              </Button>
            </div>
          </div>
        ) : !entries ? (
          <LogSkeleton />
        ) : entries.length === 0 ? (
          <p className="m-auto px-6 text-center text-[13.5px] text-ash-400">
            {session.live ? t("instance.logs.waiting") : t("instance.logs.empty")}
          </p>
        ) : rows.length === 0 ? (
          <div className="m-auto flex flex-col items-center gap-3 px-6 text-center">
            <p className="text-[13.5px] text-ash-400">{t("instance.logs.noMatch")}</p>
            {filtered ? (
              <Button variant="ghost" size="sm" onClick={clearFilters}>
                {t("instance.logs.clearFilters")}
              </Button>
            ) : null}
          </div>
        ) : (
          <LogList
            handle={list}
            rows={rows}
            widest={widest}
            hitsByRow={hitsByRow}
            hitLength={query.length}
            current={matches[current]}
            follow={following}
            live={session.live}
            setFollowing={setFollowing}
            setUnseen={setUnseen}
            label={t("instance.logs.viewerLabel")}
          />
        )}
        {session.live && !following && unseen ? (
          <div className="pointer-events-none absolute inset-x-0 bottom-3 flex justify-center">
            <Button
              size="sm"
              variant="secondary"
              icon={<ArrowDown size={15} className="text-gold-300" />}
              className="pointer-events-auto shadow-[0_10px_30px_rgb(34_6_6/0.5)]"
              onClick={() => {
                setFollowing(true);
                setUnseen(false);
                list.current?.scrollToEnd();
              }}
            >
              {t("instance.logs.newLines")}
            </Button>
          </div>
        ) : null}
      </div>

      <ShareLogDialog
        instanceId={instanceId}
        session={session}
        open={sharing}
        onOpenChange={setSharing}
      />
    </div>
  );
}
