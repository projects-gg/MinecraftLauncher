// PACK-EDIT: "Sürümü Değiştir" for the official pack's mods. A quiet link on the row opens a short
// list of the mod's Modrinth versions for the instance's game version; the player picks one and
// it is installed in place of the pack's (it stays until the pack moves to a newer one). The row
// then says which version runs, with "Paketin Sürümüne Dön" beside it.
import { ArrowCounterClockwise, Check, Swap } from "@phosphor-icons/react";
import { Button, Chip, cn, Dialog, Skeleton, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { newJobId, showError } from "@/features/content/jobs";
import { api, type JobProgress, type ModCenterItem, type PackItemVersion } from "@/lib/api";
import { formatBytes, formatRelative } from "@/lib/format";
import { swap, useMotionPrefs } from "./motion";
import { shortVersion } from "./pack-versions-text";
import { JobLine } from "./parts";

/** The signed config's `features.packEdit`, from the home content (the query Home reads). */
export function usePackEdit(): boolean {
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  return Boolean(data?.packEdit);
}

/** "Sürümü Değiştir": quiet like "Mod sayfası" beside it. */
export function ChangeLink({
  name,
  disabled = false,
  onClick,
}: {
  name: string;
  /** The game is running or changes are being applied: shown, dimmed. */
  disabled?: boolean;
  onClick: () => void;
}) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      aria-label={t("packEdit.changeFor", { name })}
      className="inline-flex cursor-pointer items-center gap-1 whitespace-nowrap rounded-sm align-baseline text-[12.5px] font-medium text-ash-300 transition-[color,opacity] duration-150 hover:text-gold-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300 disabled:cursor-default disabled:opacity-55 disabled:hover:text-ash-300"
    >
      <Swap size={12} aria-hidden className="shrink-0" />
      {t("packEdit.change")}
    </button>
  );
}

/**
 * The line under a recommended mod that runs the player's own version (with the way back) or
 * whose copy is the player's own jar. Nothing otherwise.
 */
export function VersionLine({
  item,
  locked,
  onChange,
  onBack,
}: {
  item: ModCenterItem;
  locked: boolean;
  /** "Sürümü Değiştir" (only while `features.packEdit` is on). */
  onChange?: () => void;
  onBack: () => void;
}) {
  const { t } = useTranslation();
  if (item.standIn) {
    return <p className="mt-1 text-[12.5px] leading-snug text-ash-300">{t("packEdit.standIn")}</p>;
  }
  const own = item.versionOverride;
  if (!own) return null;
  const version = shortVersion(own.versionNumber);
  const note =
    own.kind === "needed" && own.neededBy.length > 0
      ? t("packEdit.needed", { names: own.neededBy.join(", "), version })
      : t("packEdit.chosen", { version });
  return (
    <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1">
      <p className="text-[12.5px] leading-snug text-ash-300">{note}</p>
      {onChange ? <ChangeLink name={item.title} disabled={locked} onClick={onChange} /> : null}
      <Button
        size="sm"
        variant="ghost"
        disabled={locked}
        icon={<ArrowCounterClockwise size={14} />}
        onClick={onBack}
      >
        {t("packEdit.back")}
      </Button>
    </div>
  );
}

function VersionRow({
  version,
  picked,
  onPick,
}: {
  version: PackItemVersion;
  picked: boolean;
  onPick: () => void;
}) {
  const { t } = useTranslation();
  return (
    <li>
      <button
        type="button"
        aria-pressed={picked}
        onClick={onPick}
        className="group/opt relative flex w-full cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] px-3 py-2.5 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        {/* Hover and "picked" plates fade in place (opacity only). */}
        <span
          aria-hidden
          className="pointer-events-none absolute inset-0 rounded-[inherit] bg-white/5 opacity-0 transition-opacity duration-150 group-hover/opt:opacity-100"
        />
        <span
          aria-hidden
          className={cn(
            "pointer-events-none absolute inset-0 rounded-[inherit] bg-white/[0.07] transition-opacity duration-150",
            picked ? "opacity-100" : "opacity-0",
          )}
        />
        <span className="relative flex w-4 shrink-0 justify-center text-bone-50">
          <Check
            size={14}
            weight="bold"
            aria-hidden
            className={cn("transition-opacity duration-150", picked ? "opacity-100" : "opacity-0")}
          />
        </span>
        <span className="relative min-w-0 font-display text-[15px] font-semibold text-bone-50 tabular-nums">
          {shortVersion(version.number)}
        </span>
        <span className="relative flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
          {version.current ? <Chip>{t("packEdit.dialog.inUse")}</Chip> : null}
          {version.pack ? <Chip>{t("packEdit.dialog.inPack")}</Chip> : null}
          {version.stable ? null : <Chip>{t("packEdit.dialog.beta")}</Chip>}
        </span>
        <span className="relative hidden shrink-0 gap-3 text-[12px] text-ash-400 tabular-nums sm:flex">
          <span>{formatRelative(version.published)}</span>
          <span className="w-[60px] text-right">{formatBytes(version.sizeBytes)}</span>
        </span>
      </button>
    </li>
  );
}

const skeletonRows = ["w-14", "w-16", "w-12", "w-16", "w-14"];

/** Shaped like the version rows. */
function ListSkeleton() {
  return (
    <div aria-busy className="space-y-0.5">
      {skeletonRows.map((width, index) => (
        <div key={`${width}-${index.toString()}`} className="flex h-11 items-center gap-3 px-3">
          <span className="w-4 shrink-0" />
          <Skeleton className={`h-4 ${width}`} />
          <span className="flex-1" />
          <Skeleton className="h-3 w-28" />
        </div>
      ))}
    </div>
  );
}

/** "<Mod> Sürümleri": pick a version of a recommended mod and install it. */
export function PackVersionsDialog({
  instanceId,
  item,
  onClose,
}: {
  instanceId: string;
  /** The mod; `null` closes the dialog. */
  item: ModCenterItem | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const prefs = useMotionPrefs();
  const open = item !== null;
  // The last mod stays named while the dialog closes.
  const [shown, setShown] = useState<ModCenterItem | null>(item);
  const [picked, setPicked] = useState<string | null>(null);
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const running = useRef(false);
  useEffect(() => {
    if (item) {
      setShown(item);
      setPicked(null);
      setProgress(null);
    }
  }, [item]);

  const versions = useQuery({
    queryKey: ["pack-item-versions", instanceId, shown?.id],
    queryFn: () => api.modCenter.itemVersions(instanceId, shown?.id ?? ""),
    enabled: open && Boolean(shown),
    staleTime: 60_000,
    retry: false,
  });
  const list = versions.data ?? [];
  const current = list.find((v) => v.current);
  const chosen = list.find((v) => v.id === picked);
  const busy = progress !== null;
  const ready = Boolean(chosen && chosen.id !== current?.id);

  const install = async () => {
    if (!shown || !chosen || running.current) return;
    running.current = true;
    setProgress({ stage: "resolving", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 });
    try {
      await api.modCenter.setVersion(
        instanceId,
        shown.id,
        chosen.pack ? null : chosen.id,
        newJobId(),
        setProgress,
      );
      await queryClient.refetchQueries({ queryKey: ["mod-center", instanceId], exact: true });
      toast.success(
        chosen.pack
          ? t("packEdit.backDone", { name: shown.title })
          : t("packEdit.dialog.done", { name: shown.title, version: shortVersion(chosen.number) }),
      );
      onClose();
    } catch (error) {
      showError(error);
    } finally {
      running.current = false;
      setProgress(null);
      void queryClient.invalidateQueries({ queryKey: ["pack-item-versions", instanceId] });
      void queryClient.invalidateQueries({ queryKey: ["instance", instanceId] });
      void queryClient.invalidateQueries({ queryKey: ["content-scan", instanceId, "mod"] });
    }
  };

  const footer = (
    <div className="relative flex min-h-9 w-full items-center justify-end gap-2">
      <AnimatePresence initial={false} mode="popLayout">
        {busy ? (
          <motion.div key="busy" {...swap(0.2)} className="min-w-0 flex-1">
            <JobLine progress={progress} label={t("packEdit.dialog.installing")} />
          </motion.div>
        ) : (
          <motion.div key="actions" {...swap(0.2)} className="flex items-center gap-2">
            <Button variant="ghost" onClick={onClose}>
              {t("packEdit.dialog.cancel")}
            </Button>
            <Button variant="primary" disabled={!ready} onClick={() => void install()}>
              {chosen?.pack ? t("packEdit.dialog.useBack") : t("packEdit.dialog.use")}
            </Button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );

  const body = versions.isPending ? (
    <ListSkeleton />
  ) : versions.isError ? (
    <div className="flex flex-col items-start gap-3 px-3 py-4">
      <p className="text-[13.5px] text-bone-100">{t("packEdit.dialog.failed")}</p>
      <Button size="sm" variant="secondary" onClick={() => void versions.refetch()}>
        {t("packEdit.dialog.retry")}
      </Button>
    </div>
  ) : list.length <= 1 ? (
    <p className="px-3 py-6 text-center text-[13px] text-ash-400">{t("packEdit.dialog.empty")}</p>
  ) : (
    <ul
      aria-label={t("packEdit.dialog.title", { name: shown?.title ?? "" })}
      className={cn(
        "-mx-2 max-h-[min(380px,52vh)] space-y-0.5 overflow-y-auto px-2 transition-opacity duration-200",
        busy && "pointer-events-none opacity-55",
      )}
    >
      {list.map((version) => (
        <VersionRow
          key={version.id}
          version={version}
          picked={picked === version.id}
          onPick={() => setPicked(version.id)}
        />
      ))}
    </ul>
  );

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && !busy && onClose()}
      title={t("packEdit.dialog.title", { name: shown?.title ?? "" })}
      description={t("packEdit.dialog.description")}
      closeLabel={t("common.close")}
      width={520}
      footer={footer}
      morph={prefs.layout}
      dismissible={!busy}
    >
      {body}
    </Dialog>
  );
}
