// Settings → Java (J1): the launcher's own Java runtimes, the Javas on the computer and custom
// paths instances point at, each with the instances that use it. Launcher runtimes update in
// the background; "Güncelle" asks for it now, "Kaldır" removes an unused one, "Otomatik'e geç"
// moves instances off a custom Java.
import { ArrowsClockwise, ClockCountdown, FolderOpen, WarningCircle } from "@phosphor-icons/react";
import {
  Button,
  Chip,
  cn,
  easeOutExpo,
  IconButton,
  Progress,
  Skeleton,
  Tooltip,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  groupRuntimes,
  locationLabel,
  type RuntimeAction,
  readableVersion,
  runtimeAction,
  runtimeTitle,
} from "@/features/java/texts";
import { api, type JavaOverview, type JavaRuntime } from "@/lib/api";
import { formatBytes, formatPercent } from "@/lib/format";

const QUERY_KEY = ["java-overview"] as const;

function errorText(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}

/** "Java" over the version number: the identity of a row, the same as in the Java question. */
function Major({ runtime }: { runtime: JavaRuntime }) {
  const broken = runtime.problem !== undefined || runtime.major === undefined;
  return (
    <div className="flex w-[58px] shrink-0 flex-col items-start" aria-hidden>
      <span className="text-[11.5px] leading-none font-medium text-ash-400">Java</span>
      {broken ? (
        <WarningCircle size={24} weight="fill" className="mt-1 text-flare-400" />
      ) : (
        <span className="mt-0.5 font-display text-[26px] leading-none font-semibold text-bone-50 tabular-nums">
          {runtime.major}
        </span>
      )}
    </div>
  );
}

function Users({ runtime }: { runtime: JavaRuntime }) {
  const { t } = useTranslation();
  if (runtime.users.length === 0) {
    return runtime.kind === "managed" ? (
      <p className="text-[12.5px] text-ash-400">{t("java.unused")}</p>
    ) : null;
  }
  return (
    <ul aria-label={t("java.users")} className="flex flex-wrap gap-1.5">
      {runtime.users.map((user) => {
        const chip = (
          <Chip tone={user.fit ? "flare" : "neutral"} className="max-w-[180px]">
            {user.fit ? <WarningCircle size={12} weight="fill" aria-hidden /> : null}
            <span className="truncate">{user.name}</span>
            {user.fit ? <span className="sr-only">{t(`java.${user.fit}`)}</span> : null}
          </Chip>
        );
        return (
          <li key={user.instanceId}>
            {user.fit ? <Tooltip content={t(`java.${user.fit}`)}>{chip}</Tooltip> : chip}
          </li>
        );
      })}
    </ul>
  );
}

function Action({
  action,
  gameActive,
  busy,
  onUpdate,
  onRemove,
  onUseAuto,
}: {
  action: RuntimeAction;
  gameActive: boolean;
  busy: boolean;
  onUpdate: () => void;
  onRemove: () => void;
  onUseAuto: (ids: string[]) => void;
}) {
  const { t } = useTranslation();
  switch (action.kind) {
    case "updating":
      return (
        <div className="flex w-[132px] flex-col items-end gap-1.5">
          <span className="flex w-full justify-between text-[12.5px] text-ash-300">
            {t("java.updating")}
            <span className="tabular-nums">{formatPercent(action.percent)}</span>
          </span>
          <Progress value={action.percent} label={t("java.updating")} className="w-full" />
        </div>
      );
    case "waiting":
      return (
        <span className="flex items-center gap-1.5 text-[12.5px] text-ash-300">
          <ClockCountdown size={15} aria-hidden />
          {t("java.waiting")}
        </span>
      );
    case "failed":
      return (
        <span className="flex items-center gap-2">
          <span className="text-[12.5px] text-flare-300">{t("java.failed")}</span>
          <Button size="sm" variant="ghost" disabled={busy} onClick={onUpdate}>
            {t("java.retry")}
          </Button>
        </span>
      );
    case "update":
      return (
        <Tooltip content={t("java.updateTo", { version: action.version })}>
          <Button
            size="sm"
            variant="secondary"
            disabled={busy}
            icon={<ArrowsClockwise size={15} />}
            onClick={onUpdate}
          >
            {t("java.update")}
          </Button>
        </Tooltip>
      );
    case "remove": {
      const button = (
        <Button size="sm" variant="ghost" disabled={busy || gameActive} onClick={onRemove}>
          {t("java.remove")}
        </Button>
      );
      return (
        <span className="flex items-center gap-2">
          <span className="text-[12.5px] text-ash-400 tabular-nums">
            {formatBytes(action.bytes)}
          </span>
          {gameActive ? (
            <Tooltip content={t("java.removeBlocked")}>
              {/* A disabled button gets no pointer events; the wrapper carries the tooltip. */}
              <span>{button}</span>
            </Tooltip>
          ) : (
            button
          )}
        </span>
      );
    }
    case "useAuto":
      return (
        <Button
          size="sm"
          variant="secondary"
          disabled={busy}
          onClick={() => onUseAuto(action.instanceIds)}
        >
          {t("java.useAuto")}
        </Button>
      );
    case "none":
      return null;
  }
}

function RuntimeRow({
  runtime,
  live,
  gameActive,
  glide,
  onUpdate,
  onRemove,
  onUseAuto,
}: {
  runtime: JavaRuntime;
  live?: number;
  gameActive: boolean;
  glide: boolean;
  onUpdate: (runtime: JavaRuntime) => Promise<void>;
  onRemove: (runtime: JavaRuntime) => Promise<void>;
  onUseAuto: (ids: string[]) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const run = (job: () => Promise<void>) => {
    setBusy(true);
    void job().finally(() => setBusy(false));
  };
  const action = runtimeAction(runtime, live);
  const location = locationLabel(t, runtime);
  return (
    <motion.li
      layout={glide ? "position" : false}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: { duration: 0.22, ease: easeOutExpo } }}
      transition={{ layout: { duration: 0.3, ease: easeOutExpo } }}
      exit={{ opacity: 0, transform: "scale(0.985)", transition: { duration: 0.16 } }}
      className="flex items-center gap-4 border-white/6 border-b py-3 last:border-b-0"
    >
      <Major runtime={runtime} />
      <div className="min-w-0 flex-1 space-y-1.5">
        <div className="flex min-w-0 flex-wrap items-baseline gap-x-2.5 gap-y-0.5">
          <span
            className={cn(
              "text-[13.5px] font-semibold",
              runtime.problem ? "text-flare-300" : "text-bone-50",
            )}
          >
            {runtime.version ? readableVersion(runtime.version) : runtimeTitle(t, runtime)}
          </span>
          {runtime.vendor ? (
            <span className="text-[12.5px] text-ash-300">{runtime.vendor}</span>
          ) : null}
          {runtime.bits32 ? (
            <Tooltip content={t("java.bits32Help")}>
              <Chip tone="flare" tabIndex={0}>
                {t("java.bits32")}
              </Chip>
            </Tooltip>
          ) : null}
        </div>
        <Tooltip content={runtime.location} side="bottom">
          <p className="w-fit max-w-full truncate text-[12.5px] text-ash-400">{location}</p>
        </Tooltip>
        <Users runtime={runtime} />
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <div className="relative flex min-h-8 items-center justify-end">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.div
              key={action.kind}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: { duration: 0.2, ease: easeOutExpo } }}
              exit={{ opacity: 0, transition: { duration: 0.12 } }}
            >
              <Action
                action={action}
                gameActive={gameActive}
                busy={busy}
                onUpdate={() => run(() => onUpdate(runtime))}
                onRemove={() => run(() => onRemove(runtime))}
                onUseAuto={(ids) => run(() => onUseAuto(ids))}
              />
            </motion.div>
          </AnimatePresence>
        </div>
        {runtime.problem === "missing" ? (
          // Keeps the actions of every row in one column.
          <span className="size-8 shrink-0" aria-hidden />
        ) : (
          <IconButton
            label={t("java.openFolder")}
            icon={<FolderOpen size={17} />}
            size="sm"
            tooltipSide="top"
            onClick={() =>
              void api.java
                .openFolder(runtime.id)
                .catch((error) => toast.error(errorText(error, t("java.openFailed"))))
            }
          />
        )}
      </div>
    </motion.li>
  );
}

function Group({
  title,
  action,
  children,
}: {
  title: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="mt-5 first:mt-2">
      <div className="flex h-8 items-center justify-between gap-3">
        <h3 className="text-[12.5px] font-medium text-ash-400">{title}</h3>
        {action}
      </div>
      {children}
    </section>
  );
}

function RowsSkeleton() {
  return (
    <div className="mt-2 space-y-1" aria-hidden>
      {[0, 1, 2].map((i) => (
        <div key={i} className="flex items-center gap-4 py-3">
          <div className="w-[58px] space-y-1.5">
            <Skeleton className="h-3 w-8" />
            <Skeleton className="h-6 w-9" />
          </div>
          <div className="flex-1 space-y-2">
            <Skeleton className="h-3.5 w-40" />
            <Skeleton className="h-3 w-64" />
          </div>
          <Skeleton className="h-8 w-24" />
        </div>
      ))}
    </div>
  );
}

export function JavaSection() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const glide = useMotionPrefs().layout;
  const { data, isPending, isError, refetch } = useQuery({
    queryKey: QUERY_KEY,
    queryFn: () => api.java.overview(),
    // Runtimes, instances and the computer's Javas change outside this page: fresh on every visit.
    staleTime: 0,
  });
  const [live, setLive] = useState<Record<string, number>>({});
  const [rescanning, setRescanning] = useState(false);

  // Updates report their progress live; the list itself refreshes when one starts or ends.
  useEffect(
    () => api.java.onProgress((id, percent) => setLive((prev) => ({ ...prev, [id]: percent }))),
    [],
  );
  useEffect(
    () =>
      api.java.onChange(() => {
        void queryClient.invalidateQueries({ queryKey: QUERY_KEY });
      }),
    [queryClient],
  );
  // Percentages of updates that are over do not stay behind.
  useEffect(() => {
    if (!data) return;
    setLive((prev) => {
      const running = new Set(
        data.runtimes.filter((r) => r.updating !== undefined).map((r) => r.id),
      );
      const kept = Object.fromEntries(Object.entries(prev).filter(([id]) => running.has(id)));
      return Object.keys(kept).length === Object.keys(prev).length ? prev : kept;
    });
  }, [data]);

  const update = async (runtime: JavaRuntime) => {
    if (!runtime.component || !runtime.platform) return;
    try {
      await api.java.update(runtime.component, runtime.platform);
      setLive((prev) => ({ ...prev, [runtime.id]: 0 }));
      await queryClient.invalidateQueries({ queryKey: QUERY_KEY });
    } catch (error) {
      toast.error(errorText(error, t("java.failed")));
    }
  };
  const remove = async (runtime: JavaRuntime) => {
    if (!runtime.component || !runtime.platform) return;
    try {
      const freed = await api.java.remove(runtime.component, runtime.platform);
      // The row leaves at once; the list refreshes behind it.
      queryClient.setQueryData<JavaOverview>(QUERY_KEY, (old) =>
        old ? { ...old, runtimes: old.runtimes.filter((r) => r.id !== runtime.id) } : old,
      );
      toast.success(t("java.removed", { major: runtime.major, size: formatBytes(freed) }));
      void queryClient.invalidateQueries({ queryKey: QUERY_KEY });
    } catch (error) {
      toast.error(errorText(error, t("java.removeFailed")));
    }
  };
  const switchToAuto = async (ids: string[]) => {
    try {
      const count = await api.java.switchToAuto(ids);
      if (count > 0) toast.success(t("java.switched", { count }));
      await queryClient.invalidateQueries({ queryKey: QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: ["instance"] });
    } catch (error) {
      toast.error(errorText(error, t("java.loadFailed")));
    }
  };
  const rescan = async () => {
    setRescanning(true);
    try {
      queryClient.setQueryData(QUERY_KEY, await api.java.overview(true));
    } catch (error) {
      toast.error(errorText(error, t("java.loadFailed")));
    } finally {
      setRescanning(false);
    }
  };

  if (isPending) return <RowsSkeleton />;
  if (isError || !data) {
    return (
      <div className="flex items-center justify-between gap-4 py-4">
        <p className="text-[13.5px] text-ash-300">{t("java.loadFailed")}</p>
        <Button size="sm" variant="secondary" onClick={() => void refetch()}>
          {t("java.retry")}
        </Button>
      </div>
    );
  }

  const groups = groupRuntimes(data.runtimes);
  const rows = (list: JavaRuntime[]) => (
    <ul>
      <AnimatePresence initial={false}>
        {list.map((runtime) => (
          <RuntimeRow
            key={runtime.id}
            runtime={runtime}
            live={live[runtime.id]}
            gameActive={data.gameActive}
            glide={glide}
            onUpdate={update}
            onRemove={remove}
            onUseAuto={switchToAuto}
          />
        ))}
      </AnimatePresence>
    </ul>
  );

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1, transition: { duration: 0.18 } }}>
      <Group title={t("java.groups.managed")}>
        {groups.managed.length > 0 ? (
          rows(groups.managed)
        ) : (
          <p className="py-3 text-[13px] text-ash-300">{t("java.emptyManaged")}</p>
        )}
      </Group>
      <Group
        title={t("java.groups.system")}
        action={
          <IconButton
            label={t("java.rescan")}
            size="sm"
            tooltipSide="left"
            disabled={rescanning}
            onClick={() => void rescan()}
            icon={
              <ArrowsClockwise
                size={16}
                className={rescanning ? "animate-spin motion-reduce:animate-none" : undefined}
              />
            }
          />
        }
      >
        {groups.system.length > 0 ? (
          rows(groups.system)
        ) : (
          <p className="py-3 text-[13px] text-ash-300">{t("java.emptySystem")}</p>
        )}
        {data.systemIncomplete ? (
          <p className="pb-1 text-[12.5px] text-ash-400">{t("java.incomplete")}</p>
        ) : null}
      </Group>
      {groups.custom.length > 0 ? (
        <Group title={t("java.groups.custom")}>{rows(groups.custom)}</Group>
      ) : null}
    </motion.div>
  );
}
