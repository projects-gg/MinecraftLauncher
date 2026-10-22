import { FileArrowUp, FileZip, Package, Sun, TShirt, Warning } from "@phosphor-icons/react";
import { Button, Chip, cn, DropZone, EntranceGate, Field, Input, Skeleton, Switch } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useDrop, useDropAnnouncement, useDropZone } from "@/features/drop/store";
import { announcementOf, zoneTitle } from "@/features/drop/text";
import { hasModpack } from "@/features/drop/zones";
import { api, type ModpackImportResult, type PackOptionalFile, type PackPreview } from "@/lib/api";
import { notifyWhenAway } from "@/lib/attention";
import { formatBytes, formatCount } from "@/lib/format";
import { ContentIcon } from "@/shell/content-icon";
import { trackTask } from "@/stores/progress";
import { newJobId, type RunningJob, showError } from "./jobs";
import { optionalFolder, optionalTotals, prettyFileName } from "./optional-files";
import { JobStatus, ProjectIcon } from "./parts";

const folderIcon = {
  mods: Package,
  resourcepacks: TShirt,
  shaderpacks: Sun,
  other: FileZip,
} as const;

/** A pack's optional files: each one comes along unless the player turns it off. */
function OptionalFiles({
  files,
  off,
  onToggle,
  disabled,
}: {
  files: PackOptionalFile[];
  off: Set<string>;
  onToggle: (path: string, on: boolean) => void;
  disabled: boolean;
}) {
  const { t } = useTranslation();
  const ids = useMemo(
    () => files.map((f) => f.projectId).filter((id): id is string => Boolean(id)),
    [files],
  );
  const briefs = useQuery({
    queryKey: ["project-briefs", ids],
    queryFn: () => api.content.projectBriefs(ids),
    enabled: ids.length > 0,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
  const byId = new Map((briefs.data ?? []).map((b) => [b.id, b]));
  const chosen = files.filter((f) => !off.has(f.path)).length;
  return (
    <section aria-label={t("modpack.import.optionalTitle")}>
      <div className="mb-2 flex items-baseline justify-between gap-3">
        <h3 className="text-[13px] font-semibold text-bone-50">
          {t("modpack.import.optionalTitle")}
        </h3>
        <span className="text-[12px] text-ash-400 tabular-nums">
          {t("modpack.import.optionalCount", { on: chosen, total: files.length })}
        </span>
      </div>
      <ul className="max-h-[196px] divide-y divide-white/6 overflow-y-auto rounded-[var(--radius-panel)] bg-obsidian-950/40 ring-1 ring-white/7">
        {files.map((file) => {
          const brief = file.projectId ? byId.get(file.projectId) : undefined;
          const name = brief?.title ?? prettyFileName(file.fileName);
          const folder = optionalFolder(file.folder);
          const Icon = folderIcon[folder];
          const on = !off.has(file.path);
          return (
            <li key={file.path} className="flex items-center gap-3 px-3 py-2">
              {brief?.iconUrl ? (
                <ProjectIcon src={brief.iconUrl} size={32} />
              ) : (
                <span className="chamfer flex size-8 shrink-0 items-center justify-center bg-obsidian-800 [--cut:6px]">
                  <Icon size={16} className="text-gold-400/80" />
                </span>
              )}
              <div className="min-w-0 flex-1">
                <motion.p
                  key={name}
                  initial={{ opacity: 0.4 }}
                  animate={{ opacity: 1, transition: { duration: 0.2 } }}
                  className={cn(
                    "truncate text-[13.5px] font-medium transition-colors duration-200",
                    on ? "text-bone-50" : "text-ash-400",
                  )}
                >
                  {name}
                </motion.p>
                <p className="flex gap-3 text-[12px] text-ash-500 tabular-nums">
                  <span>{t(`modpack.import.folders.${folder}`)}</span>
                  <span>{formatBytes(file.size)}</span>
                </p>
              </div>
              <Switch
                checked={on}
                onCheckedChange={(next) => onToggle(file.path, next)}
                label={name}
                hideLabel
                disabled={disabled}
              />
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function PreviewCard({
  preview,
  name,
  onName,
  off,
  onToggle,
  disabled,
}: {
  preview: PackPreview;
  name: string;
  onName: (name: string) => void;
  off: Set<string>;
  onToggle: (path: string, on: boolean) => void;
  disabled: boolean;
}) {
  const { t } = useTranslation();
  const loader = preview.loader === "vanilla" ? "Minecraft" : t(`loaders.${preview.loader}`);
  const downloads = optionalTotals(preview, off);
  return (
    <div className="grid grid-cols-1 gap-4">
      <div className="glass flex items-start gap-4 rounded-[var(--radius-panel)] p-4">
        <span className="chamfer flex size-14 shrink-0 items-center justify-center bg-obsidian-800 [--cut:7px]">
          <ContentIcon kind="pack" />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="truncate font-display text-[17px] font-semibold text-bone-50">
              {preview.name}
            </p>
            <Chip>{t(`modpack.import.formats.${preview.format}`)}</Chip>
          </div>
          <p className="mt-0.5 text-[13px] text-ash-300">
            {loader} {preview.mcVersion}
            {preview.loaderVersion ? ` (${preview.loaderVersion})` : ""}
            {preview.packVersion ? `, ${preview.packVersion}` : ""}
          </p>
          {preview.summary ? (
            <p className="mt-1.5 line-clamp-2 text-[13px] text-ash-400">{preview.summary}</p>
          ) : null}
          <ul className="mt-2.5 flex flex-wrap gap-x-5 gap-y-1 text-[12.5px] text-ash-400 tabular-nums">
            {downloads.files > 0 ? (
              <li>
                {t("modpack.import.downloads", {
                  count: formatCount(downloads.files),
                  size: formatBytes(downloads.bytes),
                })}
              </li>
            ) : null}
            {preview.bundledFiles > 0 ? (
              <li>{t("modpack.import.bundled", { count: formatCount(preview.bundledFiles) })}</li>
            ) : null}
          </ul>
        </div>
      </div>
      {preview.manualFiles > 0 || preview.unsupported.length > 0 ? (
        <div className="flex gap-2.5 rounded-[var(--radius-panel)] border border-gold-500/25 bg-gold-500/8 px-3.5 py-3 text-[13px] text-bone-100">
          <Warning size={18} className="mt-px shrink-0 text-gold-300" />
          <div className="space-y-1">
            {preview.manualFiles > 0 ? (
              <p>{t("modpack.import.manual", { count: preview.manualFiles })}</p>
            ) : null}
            {preview.unsupported.length > 0 ? (
              <p>{t("modpack.import.unsupported", { items: preview.unsupported.join(", ") })}</p>
            ) : null}
          </div>
        </div>
      ) : null}
      {preview.optional.length > 0 ? (
        <OptionalFiles files={preview.optional} off={off} onToggle={onToggle} disabled={disabled} />
      ) : null}
      <Field label={t("create.name")} htmlFor="import-name">
        <Input
          id="import-name"
          value={name}
          maxLength={48}
          disabled={disabled}
          onChange={(e) => onName(e.target.value)}
        />
      </Field>
    </div>
  );
}

/**
 * "Import from file" in the new-instance dialog: drop or pick a pack, preview it, import it as
 * a new instance with progress. Returns the body and the footer for the dialog.
 */
export function useModpackImport({
  active,
  onCancel,
  onDone,
}: {
  active: boolean;
  onCancel: () => void;
  onDone: (result: ModpackImportResult) => void;
}): { body: ReactNode; footer: ReactNode; cancelJob: () => void } {
  const { t } = useTranslation();
  const [path, setPath] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [job, setJob] = useState<RunningJob | null>(null);
  // Optional files the player turned off (all come along by default, as the pack intends).
  const [off, setOff] = useState<Set<string>>(() => new Set());

  const preview = useQuery({
    queryKey: ["pack-preview", path],
    queryFn: () => api.content.previewPack(path ?? ""),
    enabled: Boolean(path),
    retry: false,
    staleTime: Number.POSITIVE_INFINITY,
  });
  useEffect(() => {
    if (preview.data) {
      setName(preview.data.name);
      setOff(new Set());
    }
  }, [preview.data]);
  const toggleOptional = (filePath: string, on: boolean) =>
    setOff((prev) => {
      const next = new Set(prev);
      if (on) next.delete(filePath);
      else next.add(filePath);
      return next;
    });
  useEffect(() => {
    if (!active) {
      setPath(null);
      setName("");
    }
  }, [active]);
  // A modpack dropped onto the window opens this step with it. It stays pending until the dialog
  // closes: the page transition can mount this dialog twice, and the second one must see it too.
  const pendingPack = useDrop((s) => s.pendingPack);
  useEffect(() => {
    if (active && pendingPack) setPath(pendingPack);
  }, [active, pendingPack]);

  // Files dropped anywhere on the window go to this step while it waits for a pack (the
  // window-wide drop layer steps aside); a modpack among them wins over the other files.
  const zone = useDropZone(
    {
      layer: "claim",
      accepts: (drag) => hasModpack(drag.files),
      onDrop: (paths, files) => {
        const pick = files?.find((f) => f.kind === "modpack")?.path ?? paths[0];
        if (pick) setPath(pick);
      },
    },
    active && !job,
  );
  const dragging = zone.drag !== null;
  const zoneName = zoneTitle(t, zone.state, zone.drag?.paths.length ?? 1);
  // At rest and when refusing, the line says which files work; while a pack is over the window,
  // what happens to it.
  const zoneHint =
    dragging && zone.state !== "refuse" ? t("modpack.import.dropHint") : t("create.dropHint");
  useDropAnnouncement(
    dragging && zone.state !== "absorb" && zone.drag?.files
      ? announcementOf(zone.state, zoneName, zoneHint)
      : null,
    zone.drag?.id,
  );

  const choose = async () => {
    try {
      const chosen = await api.content.pickFiles("modpack", false);
      const first = chosen?.[0];
      if (first) setPath(first);
    } catch (error) {
      showError(error);
    }
  };

  const start = async () => {
    if (!path || !preview.data) return;
    // The dropped pack (if it was one) is being imported: it no longer waits.
    useDrop.getState().setPendingPack(null);
    const jobId = newJobId();
    setJob({ jobId });
    const task = trackTask(`modpack:${jobId}`);
    try {
      const result = await api.content.importPack(
        { path, name: name.trim() || undefined, icon: "map", jobId, skipOptional: [...off] },
        (progress) => {
          setJob({ jobId, progress });
          task.report(progress.percent);
        },
      );
      setPath(null);
      void notifyWhenAway(
        task.done(),
        t("attention.pack.title"),
        t("attention.pack.body", { name: result.instance.name }),
      );
      onDone(result);
    } catch (error) {
      showError(error);
    } finally {
      task.done();
      setJob(null);
    }
  };

  const cancelJob = () => {
    if (job) void api.content.cancel(job.jobId);
  };

  const fileName = path?.split(/[\\/]/).pop() ?? "";

  const content = !path ? null : preview.isPending ? (
    <div className="grid gap-4" aria-busy>
      <div className="glass flex items-start gap-4 rounded-[var(--radius-panel)] p-4">
        <Skeleton className="size-14 shrink-0" />
        <div className="flex-1 space-y-2">
          <Skeleton className="h-5 w-56" />
          <Skeleton className="h-4 w-40" />
          <Skeleton className="h-4 w-64" />
        </div>
      </div>
      <p className="text-[12.5px] text-ash-500">
        {t("modpack.import.reading")} {fileName}
      </p>
    </div>
  ) : preview.isError ? (
    <div className="rounded-[var(--radius-panel)] border border-flare-400/30 bg-flare-400/8 px-4 py-3.5">
      <p className="text-[13.5px] text-bone-100">
        {preview.error instanceof Error ? preview.error.message : t("content.browse.errorTitle")}
      </p>
      <p className="mt-1 truncate text-[12px] text-ash-400">{fileName}</p>
    </div>
  ) : (
    <PreviewCard
      preview={preview.data}
      name={name}
      onName={setName}
      off={off}
      onToggle={toggleOptional}
      disabled={Boolean(job)}
    />
  );

  const body = !path ? (
    <DropZone
      ref={zone.ref}
      state={zone.state}
      title={zoneName}
      hint={zoneHint}
      className="min-h-[248px]"
    >
      <Button variant="secondary" onClick={() => void choose()}>
        {t("create.chooseFile")}
      </Button>
    </DropZone>
  ) : (
    // Another pack can be dropped over the one shown: a zone covers its preview meanwhile (the
    // short reading and error states take the drop without one).
    <div ref={zone.ref} className="relative">
      {content}
      <AnimatePresence>
        {preview.data && zone.state !== "idle" ? (
          <EntranceGate key="drop">
            <DropZone
              aria-hidden
              variant="cover"
              state={zone.state}
              title={zoneName}
              hint={zoneHint}
              className="absolute inset-0 z-10"
            />
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </div>
  );

  const footer = job ? (
    <JobStatus job={job} onCancel={cancelJob} className="w-full" detailed />
  ) : (
    <>
      <Button variant="ghost" onClick={onCancel}>
        {t("create.cancel")}
      </Button>
      {path ? (
        <Button variant="secondary" onClick={() => void choose()}>
          {t("modpack.import.otherFile")}
        </Button>
      ) : null}
      {path && preview.data ? (
        <Button
          variant="primary"
          icon={<FileArrowUp size={16} weight="bold" />}
          onClick={() => void start()}
        >
          {t("modpack.import.submit")}
        </Button>
      ) : null}
    </>
  );

  return { body, footer, cancelJob };
}
