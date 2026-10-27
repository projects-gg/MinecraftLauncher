import {
  Archive,
  ArrowCounterClockwise,
  CloudArrowDown,
  Package,
  Signpost,
} from "@phosphor-icons/react";
import { Button, Chip, cn, Dialog, SearchInput, Skeleton, Switch, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { newJobId, showError } from "@/features/content/jobs";
import {
  api,
  type InstanceSummary,
  type InstanceVersion,
  type JobProgress,
  type SwitchResult,
} from "@/lib/api";
import { formatBytes, formatRelative } from "@/lib/format";
import { usePrefs } from "@/stores/prefs";
import { FirstVisitResult } from "./first-visit";
import { type Direction, enter, fade, stagger, stepVariants, swap, useMotionPrefs } from "./motion";
import { JobLine } from "./parts";

type Step = "list" | "preview" | "done";

function Line({
  icon,
  index,
  reduce,
  children,
}: {
  icon: ReactNode;
  index: number;
  reduce: boolean;
  children: ReactNode;
}) {
  return (
    <motion.li
      initial={{ opacity: 0, transform: reduce ? "none" : "translateY(6px)" }}
      animate={{
        opacity: 1,
        transform: reduce ? "none" : "translateY(0px)",
        transition: { ...enter(0.26), delay: stagger(index, 0.045) },
      }}
      className="flex gap-3"
    >
      <span className="mt-0.5 shrink-0 text-gold-300">{icon}</span>
      <div className="min-w-0 flex-1 text-[13.5px] leading-relaxed text-bone-100">{children}</div>
    </motion.li>
  );
}

function OptionRow({ option, onPick }: { option: InstanceVersion; onPick: () => void }) {
  const { t } = useTranslation();
  const facts: string[] = [];
  if (option.lastPlayedAt) {
    facts.push(t("modCenter.versions.lastPlayed", { when: formatRelative(option.lastPlayedAt) }));
  }
  if (option.sizeBytes > 0 && (option.hasSet || option.prepared)) {
    facts.push(formatBytes(option.sizeBytes));
  }
  const disabled = option.current || !option.available;
  return (
    <li>
      <button
        type="button"
        disabled={disabled}
        onClick={onPick}
        className={cn(
          "group/opt relative flex w-full items-center gap-3 rounded-[var(--radius-panel)] px-3 py-2.5 text-left",
          disabled
            ? "cursor-default"
            : "cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
        )}
      >
        {disabled ? null : (
          <span
            aria-hidden
            className="pointer-events-none absolute inset-0 rounded-[inherit] bg-white/5 opacity-0 transition-opacity duration-150 group-hover/opt:opacity-100 group-active/opt:bg-white/8"
          />
        )}
        <span
          className={cn(
            "relative w-[88px] shrink-0 font-display text-[17px] font-semibold tabular-nums",
            option.available || option.current ? "text-bone-50" : "text-ash-400",
          )}
        >
          {option.minecraft}
        </span>
        <span className="relative flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
          {option.current ? <Chip tone="gold">{t("modCenter.versions.current")}</Chip> : null}
          {option.recommended ? <Chip>{t("modCenter.versions.recommended")}</Chip> : null}
          {option.snapshot ? <Chip>{t("modCenter.versions.snapshot")}</Chip> : null}
          {option.hasSet && !option.current ? <Chip>{t("modCenter.versions.hasSet")}</Chip> : null}
          {option.prepared ? <Chip>{t("modCenter.versions.prepared")}</Chip> : null}
          {!option.available && !option.current ? (
            <span className="text-[12px] text-ash-400">{t("modCenter.versions.notOffered")}</span>
          ) : null}
        </span>
        {facts.length > 0 ? (
          <span className="relative flex shrink-0 gap-3 text-[12px] text-ash-400 tabular-nums">
            {facts.map((fact) => (
              <span key={fact}>{fact}</span>
            ))}
          </span>
        ) : null}
      </button>
    </li>
  );
}

const optionShapes = [
  { key: "a", chip: false, facts: "w-0" },
  { key: "b", chip: true, facts: "w-36" },
  { key: "c", chip: true, facts: "w-40" },
  { key: "d", chip: true, facts: "w-10" },
  { key: "e", chip: true, facts: "w-36" },
];

/** Shaped like the version rows: number, a chip, facts on the right. */
function ListSkeleton() {
  return (
    <div aria-busy className="space-y-0.5">
      {optionShapes.map((shape) => (
        <div key={shape.key} className="flex h-11 items-center gap-3 px-3">
          <Skeleton className="h-5 w-16 shrink-0" />
          <span className="w-4 shrink-0" />
          {shape.chip ? <Skeleton className="h-[22px] w-14" /> : null}
          <span className="flex-1" />
          {shape.facts !== "w-0" ? <Skeleton className={`h-3 ${shape.facts}`} /> : null}
        </div>
      ))}
    </div>
  );
}

const previewLines = [["w-3/5"], ["w-1/2", "w-2/5", "w-1/3"], ["w-2/5", "w-1/2"], ["w-1/2"]];

/** Shaped like the preview: an icon and one to three lines per fact. */
function PreviewSkeleton() {
  return (
    <div aria-busy className="space-y-4">
      {previewLines.map((lines) => (
        <div key={lines.join()} className="flex gap-3">
          <Skeleton className="size-5 shrink-0 rounded-full" />
          <div className="flex-1 space-y-2 pt-0.5">
            {lines.map((width) => (
              <Skeleton key={width} className={`h-3.5 ${width}`} />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

/**
 * The instance's Minecraft version: pick one, see what is put aside, brought back and
 * downloaded, then switch. While switching the preview stays (dimmed) and its actions turn
 * into progress; after a first visit the choices that came along are shown and the player's
 * own mods can follow. The panel glides between steps instead of jumping.
 */
export function VersionDialog({
  instance,
  open,
  onClose,
  onOpenModCenter,
}: {
  instance: InstanceSummary;
  open: boolean;
  onClose: () => void;
  onOpenModCenter?: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const prefs = useMotionPrefs();
  const showSnapshots = usePrefs((s) => s.showSnapshots);
  const [step, setStep] = useState<Step>("list");
  const [direction, setDirection] = useState<Direction>(1);
  const [target, setTarget] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [result, setResult] = useState<SwitchResult | null>(null);
  // A set of the target that cleanup moved to the trash comes back unless the player turns it off.
  const [restoreOld, setRestoreOld] = useState(true);

  useEffect(() => {
    if (open) {
      setStep("list");
      setDirection(1);
      setTarget(null);
      setQuery("");
      setResult(null);
      setBusy(false);
      setRestoreOld(true);
    }
  }, [open]);

  const go = (next: Step, towards: Direction) => {
    setDirection(towards);
    setStep(next);
  };

  const versions = useQuery({
    queryKey: ["instance-versions", instance.id, showSnapshots],
    queryFn: () => api.modCenter.versions(instance.id, showSnapshots),
    enabled: open,
    staleTime: 30_000,
  });
  const preview = useQuery({
    queryKey: ["switch-preview", instance.id, target],
    queryFn: () => api.modCenter.switchPreview(instance.id, target ?? ""),
    enabled: open && step === "preview" && Boolean(target),
    staleTime: 15_000,
    retry: false,
  });

  const trashed = preview.data?.trashedSet;
  const restoring = Boolean(trashed && restoreOld);

  // The button fades out when pressed and still takes the second press of a double click: one
  // switch only (a second one restored the set twice, failed, and showed the button again while
  // the first switch still ran).
  const running = useRef(false);
  const run = async () => {
    if (!target || running.current) return;
    running.current = true;
    setBusy(true);
    setProgress({ stage: "preparing", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 });
    try {
      // "Eski seti geri getir": the set comes back from the trash first, so the switch returns
      // to it instead of starting fresh.
      if (restoring && trashed) {
        await api.modCenter.restoreTrashedSet(instance.id, trashed.trashId);
        void queryClient.invalidateQueries({ queryKey: ["cleanup-trash"] });
      }
      const done = await api.modCenter.switchVersion(instance.id, target, newJobId(), (p) =>
        setProgress(p),
      );
      const nothingToShow =
        !done.firstVisit && done.synced && done.carried.length === 0 && done.newItems.length === 0;
      if (nothingToShow) {
        toast.success(t("modCenter.switch.readyBack", { version: done.to }));
        onClose();
      } else {
        setResult(done);
        go("done", 1);
      }
      for (const key of [
        ["instance", instance.id],
        ["instances"],
        ["mod-center", instance.id],
        ["instance-versions", instance.id],
        ["content-scan", instance.id, "mod"],
      ]) {
        void queryClient.invalidateQueries({ queryKey: key });
      }
    } catch (error) {
      showError(error);
      // A set that came back before the switch failed is no longer in the trash.
      void queryClient.invalidateQueries({ queryKey: ["switch-preview", instance.id] });
    } finally {
      running.current = false;
      setBusy(false);
      setProgress(null);
    }
  };

  const options = (versions.data?.options ?? []).filter(
    (o) => !query.trim() || o.minecraft.includes(query.trim()),
  );
  const title =
    step === "done" && result
      ? result.firstVisit
        ? t("modCenter.switch.readyFirst", { version: result.to })
        : t("modCenter.switch.readyBack", { version: result.to })
      : step === "list"
        ? t("modCenter.versions.title")
        : t("modCenter.switch.title", { version: target ?? "" });

  let body: ReactNode = null;
  if (step === "list") {
    body = (
      <div>
        {versions.data?.gameRunning ? (
          <p className="mb-3 text-[13px] text-gold-200">{t("modCenter.switch.gameRunning")}</p>
        ) : null}
        {(versions.data?.options.length ?? 0) > 10 ? (
          <SearchInput
            value={query}
            onChange={setQuery}
            placeholder={t("modCenter.versions.search")}
            label={t("modCenter.versions.search")}
            clearLabel={t("library.clearSearch")}
            className="mb-2 w-full"
          />
        ) : null}
        {versions.isPending ? (
          <ListSkeleton />
        ) : versions.isError ? (
          <p className="text-[13px] text-ash-300">{t("modCenter.versions.failed")}</p>
        ) : (
          <ul className="max-h-[52vh] space-y-0.5 overflow-y-auto">
            {options.map((option) => (
              <OptionRow
                key={option.minecraft}
                option={option}
                onPick={() => {
                  setTarget(option.minecraft);
                  setRestoreOld(true);
                  go("preview", 1);
                }}
              />
            ))}
          </ul>
        )}
        {versions.data?.partial ? (
          <p className="mt-3 text-[12.5px] text-ash-400">{t("modCenter.versions.partial")}</p>
        ) : null}
      </div>
    );
  } else if (step === "preview") {
    if (preview.isPending) {
      body = <PreviewSkeleton />;
    } else if (preview.isError || !preview.data) {
      body = (
        <p className="text-[13.5px] text-ash-300">
          {preview.error instanceof Error ? preview.error.message : t("modCenter.switch.failed")}
        </p>
      );
    } else {
      const p = preview.data;
      const firstVisitLines: ReactNode[] = [];
      if (p.firstVisit) {
        if (p.hasConfig) {
          firstVisitLines.push(<p key="config">{t("modCenter.switch.firstVisit")}</p>);
        }
        if (p.carriedChoices > 0) {
          firstVisitLines.push(
            <p key="carried">{t("modCenter.switch.carried", { count: p.carriedChoices })}</p>,
          );
        }
        if (p.newItems > 0) {
          firstVisitLines.push(
            <p key="new">{t("modCenter.switch.newItems", { count: p.newItems })}</p>,
          );
        }
        if (p.restoredMods > 0) {
          firstVisitLines.push(
            <p key="prepared">{t("modCenter.switch.prepared", { count: p.restoredMods })}</p>,
          );
        }
      }
      // The old set from the trash comes back like a parked one: no first-visit facts.
      const old = restoring ? p.trashedSet : undefined;
      const downloadFiles = old ? old.downloadFiles : p.downloadFiles;
      const downloadBytes = old ? old.downloadBytes : p.downloadBytes;
      const lines: Array<{ key: string; icon: ReactNode; content: ReactNode }> = [
        {
          key: "parked",
          icon: <Archive size={20} />,
          content: t("modCenter.switch.parked", {
            version: p.from,
            count: p.parkedMods,
            size: formatBytes(p.parkedBytes),
          }),
        },
      ];
      if (old) {
        lines.push({
          key: "restored-old",
          icon: <ArrowCounterClockwise size={20} />,
          content: t("modCenter.switch.restored", { version: p.to, count: old.mods }),
        });
      } else if (!p.firstVisit) {
        lines.push({
          key: "restored",
          icon: <ArrowCounterClockwise size={20} />,
          content: t("modCenter.switch.restored", { version: p.to, count: p.restoredMods }),
        });
      } else if (firstVisitLines.length > 0) {
        lines.push({ key: "first", icon: <Signpost size={20} />, content: firstVisitLines });
      }
      lines.push({
        key: "downloads",
        icon: <CloudArrowDown size={20} />,
        content: (
          <>
            <p>
              {downloadFiles > 0
                ? t("modCenter.switch.downloads", {
                    count: downloadFiles,
                    size: formatBytes(downloadBytes),
                  })
                : t("modCenter.switch.noDownloads")}
            </p>
            {p.gameDownload ? (
              <p className="text-ash-300">{t("modCenter.switch.gameDownload")}</p>
            ) : null}
          </>
        ),
      });
      if (p.ownMods > 0 && p.firstVisit && !old) {
        lines.push({
          key: "own",
          icon: <Package size={20} />,
          content: t("modCenter.switch.ownMods", { count: p.ownMods, version: p.from }),
        });
      }
      const trashedFacts = p.trashedSet
        ? [
            t("modCenter.switch.restoreOldFacts", {
              count: p.trashedSet.mods,
              size: formatBytes(p.trashedSet.sizeBytes),
            }),
            p.trashedSet.lastPlayedAt
              ? t("modCenter.versions.lastPlayed", {
                  when: formatRelative(p.trashedSet.lastPlayedAt),
                })
              : null,
          ]
            .filter(Boolean)
            // Two short sentences, no middle-dot chain (docs/design.md §7).
            .join(". ")
        : "";
      body = (
        <div className={cn("transition-opacity duration-300", busy && "opacity-45")}>
          {p.trashedSet ? (
            <Switch
              checked={restoreOld}
              onCheckedChange={setRestoreOld}
              disabled={busy}
              label={t("modCenter.switch.restoreOld")}
              description={trashedFacts}
              className="mb-5"
            />
          ) : null}
          <ul className="space-y-4">
            {lines.map((line, i) => (
              <Line key={line.key} icon={line.icon} index={i} reduce={prefs.reduce}>
                {line.content}
              </Line>
            ))}
          </ul>
          {versions.data?.gameRunning ? (
            <p className="mt-4 text-[13px] text-gold-200">{t("modCenter.switch.gameRunning")}</p>
          ) : null}
        </div>
      );
    }
  } else if (result) {
    body = (
      <FirstVisitResult
        instanceId={instance.id}
        result={result}
        onOpenModCenter={
          onOpenModCenter
            ? () => {
                onClose();
                onOpenModCenter();
              }
            : undefined
        }
      />
    );
  }

  let footer: ReactNode;
  if (step === "preview") {
    footer = (
      <div className="relative flex min-h-10 w-full items-center justify-end">
        <AnimatePresence initial={false} mode="popLayout">
          {busy ? (
            <motion.div key="progress" {...swap(0.2)} className="w-full">
              <JobLine progress={progress} label={t("modCenter.switch.progress")} />
            </motion.div>
          ) : (
            <motion.div key="actions" {...swap(0.2)} className="flex items-center gap-2.5">
              <Button variant="ghost" onClick={() => go("list", -1)}>
                {t("modCenter.switch.back")}
              </Button>
              <Button
                variant="primary"
                disabled={!preview.data || versions.data?.gameRunning}
                onClick={() => void run()}
              >
                {t("modCenter.switch.confirm", { version: target ?? "" })}
              </Button>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    );
  } else if (step === "done") {
    footer = (
      <motion.div {...fade(0.2)}>
        <Button variant="primary" onClick={onClose}>
          {t("modCenter.switch.close")}
        </Button>
      </motion.div>
    );
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && !busy && onClose()}
      title={
        <span className="relative block">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span key={title} {...swap(0.18)} className="block">
              {title}
            </motion.span>
          </AnimatePresence>
        </span>
      }
      description={step === "list" ? t("modCenter.versions.description") : undefined}
      closeLabel={t("common.close")}
      width={640}
      footer={footer}
      morph={prefs.layout}
      dismissible={!busy}
    >
      <AnimatePresence mode="popLayout" initial={false} custom={direction}>
        <motion.div
          key={step}
          custom={direction}
          variants={stepVariants(prefs.reduce)}
          initial="enter"
          animate="center"
          exit="exit"
        >
          {body}
        </motion.div>
      </AnimatePresence>
    </Dialog>
  );
}
