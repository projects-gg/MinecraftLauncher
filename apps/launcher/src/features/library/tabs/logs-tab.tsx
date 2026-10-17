import { FolderOpen, Scroll, WarningCircle } from "@phosphor-icons/react";
import { Button, Chip, cn, Skeleton } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearch } from "@tanstack/react-router";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { copyErrorDetails, showError } from "@/features/content/jobs";
import { api, type GameLogSession, type InstanceDetail } from "@/lib/api";
import { formatBytes, formatDuration } from "@/lib/format";
import { TabEmpty } from "../parts";
import { useGameRunning } from "../use-game-running";
import { formatWhen, logFileWhen } from "../when";
import { gameLogKindLabel, logKey, pickLog } from "./log-sources";
import { LogViewer } from "./log-viewer";

function sessionLength(s: GameLogSession): string | null {
  if (s.live || !s.startedAt || !s.updatedAt) return null;
  const secs = (new Date(s.updatedAt).getTime() - new Date(s.startedAt).getTime()) / 1000;
  return secs >= 1 ? formatDuration(secs) : null;
}

function LogButton({
  log,
  active,
  onSelect,
}: {
  log: GameLogSession;
  active: boolean;
  onSelect: () => void;
}) {
  const { t } = useTranslation();
  const ref = useRef<HTMLButtonElement>(null);
  const own = log.source === "game";
  const length = own ? null : sessionLength(log);
  const kind = own ? gameLogKindLabel(log.fileName) : null;
  const title =
    (own ? logFileWhen(log) : formatWhen(log.startedAt ?? log.updatedAt)) ||
    (kind ?? t("instance.logs.session"));
  // A log chosen from elsewhere (the crash dialog) may be far down the list.
  useLayoutEffect(() => {
    if (active) ref.current?.scrollIntoView({ block: "nearest" });
  }, [active]);
  return (
    <button
      ref={ref}
      type="button"
      aria-current={active ? "true" : undefined}
      onClick={onSelect}
      className={cn(
        "relative flex w-full cursor-pointer flex-col rounded-[var(--radius-panel)] px-3 py-2.5 text-left transition-colors",
        "hover:bg-white/6 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
        active && "bg-white/8 hover:bg-white/8",
      )}
    >
      {active ? (
        <span
          aria-hidden
          className="absolute inset-y-2.5 left-0 w-[3px] rounded-full bg-gold-400"
        />
      ) : null}
      <span className="flex items-center gap-2">
        <span
          className={cn(
            "truncate text-[13.5px] font-semibold",
            active ? "text-gold-200" : "text-bone-50",
          )}
        >
          {title}
        </span>
        {log.live ? <Chip tone="jade">{t("instance.logs.live")}</Chip> : null}
      </span>
      <span className="mt-0.5 flex min-w-0 gap-3 text-[12px] text-ash-300 tabular-nums">
        {kind && kind !== title ? <span className="min-w-0 truncate">{kind}</span> : null}
        <span className="shrink-0">{formatBytes(log.sizeBytes)}</span>
        {length ? <span>{length}</span> : null}
      </span>
    </button>
  );
}

/** The sessions the launcher captured, then the game's own log files. */
function SessionList({
  logs,
  selected,
  onSelect,
}: {
  logs: GameLogSession[];
  selected: string | undefined;
  onSelect: (key: string) => void;
}) {
  const { t } = useTranslation();
  const groups = [
    {
      id: "launcher",
      title: t("instance.logs.sessions"),
      items: logs.filter((l) => l.source !== "game"),
    },
    {
      id: "game",
      title: t("instance.logs.gameFiles"),
      items: logs.filter((l) => l.source === "game"),
    },
  ].filter((group) => group.items.length > 0);
  return (
    <nav
      aria-label={t("instance.logs.sessions")}
      className="flex w-52 shrink-0 flex-col xl:w-[236px]"
    >
      <div className="-mr-1 min-h-0 flex-1 space-y-5 overflow-y-auto pr-1">
        {groups.map((group) => (
          <section key={group.id} aria-label={group.title}>
            <p className="mb-2 px-1 text-[12px] font-medium text-ash-400">{group.title}</p>
            <ul className="space-y-1">
              {group.items.map((log) => {
                const key = logKey(log);
                return (
                  <li key={key}>
                    <LogButton log={log} active={key === selected} onSelect={() => onSelect(key)} />
                  </li>
                );
              })}
            </ul>
          </section>
        ))}
      </div>
    </nav>
  );
}

/** The instance's captured game logs: sessions on the left, the selected log on the right. */
export function LogsTab({ instance }: { instance: InstanceDetail }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const running = useGameRunning(instance.id);
  const sessions = useQuery({
    queryKey: ["log-sessions", instance.id],
    queryFn: () => api.library.logSessions(instance.id),
  });
  const [picked, setPicked] = useState<string | null>(null);
  // `?log=<file>`: the crash dialog asks for the session that crashed.
  const { log: wanted } = useSearch({ strict: false }) as { log?: string };
  useEffect(() => {
    if (wanted) setPicked(null);
  }, [wanted]);

  // A game starting or ending adds a session or ends the live one.
  useEffect(() => {
    void running;
    void queryClient.invalidateQueries({ queryKey: ["log-sessions", instance.id] });
  }, [running, instance.id, queryClient]);

  // The live session's file appears with the game's first line: look again then.
  const hasLive = Boolean(sessions.data?.some((s) => s.live));
  const asked = useRef(0);
  useEffect(() => {
    if (hasLive) return;
    return api.library.onGameLog((id) => {
      if (id !== instance.id || Date.now() - asked.current < 2000) return;
      asked.current = Date.now();
      void queryClient.invalidateQueries({ queryKey: ["log-sessions", instance.id] });
    });
  }, [hasLive, instance.id, queryClient]);

  const list = sessions.data ?? [];
  // Until the player picks one, the asked-for session or the newest (the running game's).
  const selected = pickLog(list, picked, wanted);
  const openFolder = () => void api.library.revealLog(instance.id).catch(showError);

  if (sessions.isPending) {
    return (
      <div className="flex h-full min-h-0 gap-5" aria-busy>
        <div className="w-52 shrink-0 space-y-2 pt-6 xl:w-[236px]">
          {[0, 1, 2].map((i) => (
            <Skeleton key={i} className="h-[52px] rounded-[var(--radius-panel)]" />
          ))}
        </div>
        <div className="flex flex-1 flex-col gap-3">
          <Skeleton className="h-10 w-72" />
          <Skeleton className="h-8 w-full" />
          <Skeleton className="min-h-0 flex-1 rounded-[var(--radius-panel)]" />
        </div>
      </div>
    );
  }

  if (sessions.isError) {
    return (
      <TabEmpty
        icon={<WarningCircle size={26} weight="duotone" className="text-flare-400" />}
        title={t("instance.logs.listFailed")}
        body={
          sessions.error instanceof Error && sessions.error.message
            ? sessions.error.message
            : undefined
        }
        action={
          <>
            <Button variant="ghost" onClick={() => void copyErrorDetails(sessions.error)}>
              {t("common.copyDetails")}
            </Button>
            <Button variant="secondary" onClick={() => void sessions.refetch()}>
              {t("common.retry")}
            </Button>
          </>
        }
      />
    );
  }

  if (!selected) {
    return (
      <TabEmpty
        icon={<Scroll size={26} weight="duotone" />}
        title={t("instance.logs.emptyTitle")}
        body={t("instance.logs.emptyBody")}
        action={
          <Button variant="ghost" icon={<FolderOpen size={16} />} onClick={openFolder}>
            {t("instance.logs.openFolder")}
          </Button>
        }
      />
    );
  }

  return (
    <div className="flex h-full min-h-0 gap-5">
      <SessionList logs={list} selected={logKey(selected)} onSelect={setPicked} />
      <LogViewer key={logKey(selected)} instanceId={instance.id} session={selected} />
    </div>
  );
}
