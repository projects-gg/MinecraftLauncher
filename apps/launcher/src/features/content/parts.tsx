import { ArrowsClockwise, DownloadSimple, Package, Trash, X } from "@phosphor-icons/react";
import { Button, Chip, cn, IconButton, Progress } from "@pl/ui";
import type { TFunction } from "i18next";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { ContentKind, InstalledContent } from "@/lib/api";
import { formatSpeed } from "@/lib/format";
import { type RunningJob, stageLabel } from "./jobs";

const compact = new Intl.NumberFormat("tr-TR", { notation: "compact", maximumFractionDigits: 1 });
const whole = new Intl.NumberFormat("tr-TR");

/** A job's × takes clicks this long after its bar appeared (OYNA waits 0.8 s the same way). */
const CANCEL_ARM_MS = 600;

/** "245,2 Mn" from a million up; below that the full number ("41.770"), since the Turkish
 * "B" (bin) reads like bytes next to file sizes. */
export function formatCompact(n: number): string {
  return n >= 1_000_000 ? compact.format(n) : whole.format(n);
}

/** Turkish category name; unknown slugs are prettified. */
export function categoryLabel(t: TFunction, name: string): string {
  const fallback = name.replace(/-/g, " ").replace(/^./, (c) => c.toLocaleUpperCase("tr"));
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
        <Package size={Math.round(size * 0.46)} weight="duotone" className="text-gold-400/80" />
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
  // was: the second press of a double click must not cancel the job it just started (as OYNA).
  const [shownAt] = useState(() => performance.now());
  const cancel = () => {
    if (performance.now() - shownAt >= CANCEL_ARM_MS) onCancel?.();
  };
  const progress = job.progress;
  const percent = progress?.percent;
  const label = stageLabel(progress?.stage ?? "resolving");
  return (
    <div className={cn("flex w-[168px] items-center gap-2", className)}>
      <div className="min-w-0 flex-1">
        <div className="mb-1.5 flex items-baseline gap-3 text-[12px] tabular-nums">
          <p className="min-w-0 flex-1 truncate text-ash-300" aria-live="polite">
            {label}
            {typeof percent === "number" ? ` %${Math.round(percent)}` : ""}
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
          onClick={cancel}
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
  if (job) return <JobStatus job={job} onCancel={onCancel} />;
  if (state === "pack") return <Chip>{t("content.actions.inPack")}</Chip>;
  if (state === "none") {
    return (
      <Button
        size="sm"
        variant="secondary"
        icon={<DownloadSimple size={15} weight="bold" className="text-gold-300" />}
        onClick={onInstall}
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
