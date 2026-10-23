import { ArrowsClockwise, DownloadSimple, Trash, X } from "@phosphor-icons/react";
import {
  Button,
  Chip,
  ContentIcon,
  cn,
  IconButton,
  Progress,
  spriteSize,
  usePressGuard,
} from "@pl/ui";
import type { TFunction } from "i18next";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ContentKind, InstalledContent } from "@/lib/api";
import { formatPercent, formatSpeed, numberFormat } from "@/lib/format";
import { revealWait } from "./job-rules";
import { jobAge, type RunningJob, stageLabel } from "./jobs";

/**
 * `job` once it has run `JOB_REVEAL_MS` (job-rules.ts; at once for a job that was already
 * running when this mounted), else `undefined`. A second press meanwhile changes nothing: the
 * job is already there.
 */
export function useShownJob(job: RunningJob | undefined): RunningJob | undefined {
  const jobId = job?.jobId;
  const [revealed, setRevealed] = useState(() =>
    jobId !== undefined && revealWait(jobAge(jobId)) === 0 ? jobId : undefined,
  );
  useEffect(() => {
    if (jobId === undefined) return;
    const wait = revealWait(jobAge(jobId));
    if (wait <= 0) {
      setRevealed(jobId);
      return;
    }
    const timer = window.setTimeout(() => setRevealed(jobId), wait);
    return () => window.clearTimeout(timer);
  }, [jobId]);
  return job && revealed === jobId ? job : undefined;
}

/** "245,2 Mn" / "245.2M" from a million up; below that the full number ("41.770" / "41,770"),
 * since the Turkish "B" (bin) reads like bytes next to file sizes. */
export function formatCompact(n: number): string {
  return n >= 1_000_000
    ? numberFormat({ notation: "compact", maximumFractionDigits: 1 }).format(n)
    : numberFormat().format(n);
}

/** The category's name in the interface's language; unknown (English) slugs are prettified. */
export function categoryLabel(t: TFunction, name: string): string {
  const fallback = name.replace(/-/g, " ").replace(/^./, (c) => c.toLocaleUpperCase("en"));
  return t(`content.categories.${name}`, { defaultValue: fallback });
}

/** Square project icon with a quiet placeholder (also when the image fails to load). */
export function ProjectIcon({
  src,
  size = 48,
  className,
}: {
  src?: string;
  size?: number;
  className?: string;
}) {
  const [failed, setFailed] = useState(false);
  return (
    <span
      className={cn(
        "chamfer flex shrink-0 items-center justify-center overflow-hidden bg-obsidian-800 [--cut:7px]",
        className,
      )}
      style={{ width: size, height: size }}
    >
      {src && !failed ? (
        <img
          src={src}
          alt=""
          loading="lazy"
          draggable={false}
          onError={() => setFailed(true)}
          className="size-full object-cover"
        />
      ) : (
        <ContentIcon kind="pack" size={spriteSize(size)} />
      )}
    </span>
  );
}

/** Live progress of an install/update with a cancel button. */
export function JobStatus({
  job,
  onCancel,
  className,
  detailed,
}: {
  job: RunningJob;
  onCancel?: () => void;
  className?: string;
  /** Also show files and speed (wide places like dialog footers). */
  detailed?: boolean;
}) {
  const { t } = useTranslation();
  // The bar takes the place of "Yükle" or "Güncelle" and its × sits where the button's edge
  // was: the rest of the double click that started the job never cancels it (as OYNA). The ×
  // takes presses 0.8 s after the job started (its bar shows after 0.15 s), or once the pointer
  // has left it; a job that did not start here counts from its bar.
  const [startedAt] = useState(() => {
    const age = jobAge(job.jobId);
    return performance.now() - (Number.isFinite(age) ? age : 0);
  });
  const guard = usePressGuard(job.jobId, { since: startedAt });
  const progress = job.progress;
  const percent = progress?.percent;
  const label = stageLabel(progress?.stage ?? "resolving");
  return (
    // Fades in where the button was (content that replaces other content in place).
    <div
      className={cn(
        "flex w-[168px] animate-[fade-in_150ms_var(--ease-out-expo)] items-center gap-2",
        className,
      )}
    >
      <div className="min-w-0 flex-1">
        <div className="mb-1.5 flex items-baseline gap-3 text-[12px] tabular-nums">
          <p className="min-w-0 flex-1 truncate text-ash-300" aria-live="polite">
            {label}
            {typeof percent === "number" ? ` ${formatPercent(percent)}` : ""}
          </p>
          {/* Right-aligned slots as wide as the figures get, so the line does not slide while
              the counts and the speed change. */}
          {detailed && progress && progress.totalFiles > 0 ? (
            <span
              className="shrink-0 text-right text-ash-400"
              style={{
                minWidth: `${t("play.files", { done: progress.totalFiles, total: progress.totalFiles }).length}ch`,
              }}
            >
              {t("play.files", { done: progress.doneFiles, total: progress.totalFiles })}
            </span>
          ) : null}
          {detailed && progress && (progress.totalFiles > 0 || progress.bytesPerSec > 0) ? (
            <span className="min-w-[10.5ch] shrink-0 text-right text-ash-400">
              {progress.bytesPerSec > 0 ? formatSpeed(progress.bytesPerSec) : null}
            </span>
          ) : null}
        </div>
        <Progress value={percent} label={label} />
      </div>
      {onCancel ? (
        <IconButton
          label={t("content.actions.cancel")}
          icon={<X size={15} />}
          size="sm"
          onClick={(event) => {
            if (guard.take(event)) onCancel?.();
          }}
          {...guard.handlers}
        />
      ) : null}
    </div>
  );
}

export type InstallState = "none" | "installed" | "update" | "pack";

export function installState(entry: InstalledContent | undefined): InstallState {
  if (!entry) return "none";
  if (entry.fromPack) return "pack";
  return entry.update ? "update" : "installed";
}

/** Install / installed / update / remove control for a project in an instance. */
export function InstallControl({
  state,
  job,
  onInstall,
  onUpdate,
  onRemove,
  onCancel,
  installLabel,
}: {
  state: InstallState;
  job?: RunningJob;
  onInstall: () => void;
  onUpdate: () => void;
  onRemove: () => void;
  onCancel: () => void;
  installLabel?: string;
}) {
  const { t } = useTranslation();
  const shown = useShownJob(job);
  // Until the bar comes the button stays pressed in (its 97% squeeze), and springs back if the
  // job fails at once.
  const held = job && !shown ? "scale-[0.97]" : undefined;
  if (shown) return <JobStatus job={shown} onCancel={onCancel} />;
  if (state === "pack") return <Chip>{t("content.actions.inPack")}</Chip>;
  if (state === "none") {
    return (
      <Button
        size="sm"
        variant="secondary"
        icon={<DownloadSimple size={15} weight="bold" className="text-gold-300" />}
        onClick={onInstall}
        className={held}
      >
        {installLabel ?? t("content.actions.install")}
      </Button>
    );
  }
  return (
    <div className="flex items-center gap-1.5">
      {state === "update" ? (
        <Button
          size="sm"
          variant="secondary"
          icon={<ArrowsClockwise size={15} weight="bold" className="text-gold-300" />}
          onClick={onUpdate}
          className={held}
        >
          {t("content.actions.update")}
        </Button>
      ) : (
        <Chip tone="jade">{t("content.actions.installed")}</Chip>
      )}
      <IconButton
        label={t("content.actions.remove")}
        icon={<Trash size={16} />}
        size="sm"
        onClick={onRemove}
      />
    </div>
  );
}

/** Modrinth project type of an add-on folder. */
export function projectTypeOf(kind: ContentKind) {
  return kind === "mod" ? "mod" : kind === "resourcePack" ? "resourcepack" : "shader";
}
