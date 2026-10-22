import { ArrowSquareOut, Copy, FolderOpen, Trash } from "@phosphor-icons/react";
import { Button, ContextMenu, cn, IconButton, Skeleton, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { copyErrorDetails, showError } from "@/features/content/jobs";
import { api, type InstanceDetail, type Screenshot, type TrashItem } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { ContentIcon } from "@/shell/content-icon";
import { ConfirmDialog, TabEmpty, TabSection, TrashRow } from "../parts";
import { shotTime, trashLabel } from "../trash-label";
import { formatWhen } from "../when";
import { Lightbox } from "./lightbox";

/** Pictures rendered at first; more on request (huge folders stay light). */
const PAGE = 60;

/**
 * A gallery tile's picture: the cached thumbnail, or one made when the tile comes near the
 * viewport (the original when no thumbnail can be made). Fades in once decoded; a shimmer holds
 * the space meanwhile.
 */
function ShotImage({ instanceId, shot }: { instanceId: string; shot: Screenshot }) {
  const box = useRef<HTMLDivElement>(null);
  const [near, setNear] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [broken, setBroken] = useState(false);
  useEffect(() => {
    const element = box.current;
    if (shot.thumbUrl || !element) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry?.isIntersecting) {
          setNear(true);
          observer.disconnect();
        }
      },
      { rootMargin: "400px" },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [shot.thumbUrl]);
  const made = useQuery({
    queryKey: ["screenshot-thumb", instanceId, shot.fileName, shot.sizeBytes, shot.takenAt],
    queryFn: () => api.library.screenshotThumb(instanceId, shot.fileName),
    enabled: !shot.thumbUrl && near,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
  const thumb = shot.thumbUrl ?? made.data;
  // `null`: no thumbnail can be made; a failed thumbnail load also shows the original.
  const src = broken || made.data === null || made.isError ? shot.url : thumb;
  return (
    <div ref={box} className="relative aspect-video w-full bg-obsidian-800">
      {loaded ? null : <Skeleton className="absolute inset-0 rounded-none" />}
      {src ? (
        <img
          ref={(img) => {
            // Already decoded (cached): no fade from nothing.
            if (img?.complete && img.naturalWidth > 0) setLoaded(true);
          }}
          src={src}
          alt=""
          loading="lazy"
          decoding="async"
          draggable={false}
          onLoad={() => setLoaded(true)}
          onError={() => {
            if (src !== shot.url) setBroken(true);
          }}
          className={cn(
            "relative aspect-video w-full object-cover transition-[opacity,transform,scale] duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] group-hover:scale-[1.03] motion-reduce:group-hover:scale-100",
            loaded ? "opacity-100" : "opacity-0",
          )}
        />
      ) : null}
    </div>
  );
}

/** Screenshots of an instance: responsive gallery, lightbox, clipboard and the trash. */
export function ScreenshotsTab({ instance }: { instance: InstanceDetail }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const id = instance.id;
  const shots = useQuery({
    queryKey: ["screenshots", id],
    queryFn: () => api.library.screenshots(id),
  });
  const trash = useQuery({ queryKey: ["trash", id], queryFn: () => api.library.trash(id) });
  const [shown, setShown] = useState(PAGE);
  const [viewing, setViewing] = useState<number | null>(null);
  const [deleting, setDeleting] = useState<Screenshot | null>(null);
  const [purging, setPurging] = useState<TrashItem | null>(null);

  const list = shots.data ?? [];
  const deleted = (trash.data ?? []).filter((item) => item.kind === "screenshot");

  // Keep the lightbox on a picture that still exists after a delete.
  useEffect(() => {
    if (viewing !== null && viewing >= list.length) {
      setViewing(list.length > 0 ? list.length - 1 : null);
    }
  }, [viewing, list.length]);

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ["screenshots", id] });
    void queryClient.invalidateQueries({ queryKey: ["trash", id] });
  };

  const copy = async (shot: Screenshot) => {
    try {
      const result = await api.library.copyScreenshot(id, shot.fileName);
      if (result === "image") toast.success(t("instance.screenshots.copied"));
      else toast(t("instance.screenshots.copiedPath"));
    } catch (error) {
      showError(error);
    }
  };
  const open = (shot: Screenshot) =>
    void api.library.openScreenshot(id, shot.fileName).catch(showError);
  const reveal = (shot: Screenshot) =>
    void api.library.revealFile(id, "screenshots", shot.fileName).catch(showError);

  const restore = async (item: TrashItem) => {
    try {
      await api.library.restoreFromTrash(id, item.id);
      toast.success(t("instance.screenshots.restored"));
    } catch (error) {
      showError(error);
    } finally {
      refresh();
    }
  };

  const remove = async (shot: Screenshot) => {
    queryClient.setQueryData<Screenshot[]>(["screenshots", id], (prev) =>
      prev?.filter((s) => s.fileName !== shot.fileName),
    );
    try {
      const item = await api.library.deleteScreenshot(id, shot.fileName);
      toast(t("instance.screenshots.deleted"), {
        action: { label: t("instance.trash.undo"), onClick: () => void restore(item) },
      });
    } catch (error) {
      showError(error);
    } finally {
      refresh();
    }
  };

  const purge = async (item: TrashItem) => {
    try {
      await api.library.deleteFromTrash(id, item.id);
      toast(t("instance.trash.purged", { name: trashLabel(item) }));
    } catch (error) {
      showError(error);
    } finally {
      refresh();
    }
  };

  if (shots.isPending) {
    return (
      <div className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3" aria-hidden>
        {Array.from({ length: 8 }, (_, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: static placeholders
          <Skeleton key={i} className="aspect-video rounded-[var(--radius-panel)]" />
        ))}
      </div>
    );
  }
  if (shots.isError) {
    return (
      <TabEmpty
        icon={<ContentIcon kind="failed" />}
        title={t("instance.screenshots.listFailed")}
        body={shots.error instanceof Error ? shots.error.message : undefined}
        action={
          <>
            <Button variant="ghost" onClick={() => void copyErrorDetails(shots.error)}>
              {t("common.copyDetails")}
            </Button>
            <Button variant="secondary" onClick={() => void shots.refetch()}>
              {t("common.retry")}
            </Button>
          </>
        }
      />
    );
  }

  const totalBytes = list.reduce((sum, s) => sum + s.sizeBytes, 0);

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <p className="flex-1 text-[13px] text-ash-400 tabular-nums">
          {list.length > 0
            ? t("instance.screenshots.summary", {
                count: list.length,
                size: formatBytes(totalBytes),
              })
            : null}
        </p>
        <Button
          variant="ghost"
          icon={<FolderOpen size={16} />}
          onClick={() => void api.openInstanceFolder(id, "screenshots").catch(showError)}
        >
          {t("instance.screenshots.openFolder")}
        </Button>
      </div>

      {list.length === 0 ? (
        <TabEmpty
          icon={<ContentIcon kind="screenshots" />}
          title={t("instance.screenshots.emptyTitle")}
          body={t("instance.screenshots.empty")}
          className="pt-4"
        />
      ) : (
        <>
          <ul className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3">
            {list.slice(0, shown).map((shot, i) => {
              const when = formatWhen(shotTime(shot)) || t("instance.screenshots.untitled");
              return (
                <ContextMenu
                  key={shot.fileName}
                  items={[
                    {
                      label: t("instance.screenshots.copy"),
                      icon: <Copy size={16} />,
                      onSelect: () => void copy(shot),
                    },
                    {
                      label: t("instance.screenshots.openViewer"),
                      icon: <ArrowSquareOut size={16} />,
                      onSelect: () => open(shot),
                    },
                    {
                      label: t("instance.screenshots.reveal"),
                      icon: <FolderOpen size={16} />,
                      onSelect: () => reveal(shot),
                    },
                    null,
                    {
                      label: t("instance.screenshots.delete"),
                      icon: <Trash size={16} />,
                      danger: true,
                      onSelect: () => setDeleting(shot),
                    },
                  ]}
                >
                  <li className="group relative">
                    <button
                      type="button"
                      onClick={() => setViewing(i)}
                      aria-label={t("instance.screenshots.view", { when })}
                      className="relative block w-full cursor-zoom-in overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-800 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:ring-offset-2 focus-visible:ring-offset-obsidian-950"
                    >
                      <ShotImage instanceId={id} shot={shot} />
                      <span className="pointer-events-none absolute inset-x-0 bottom-0 bg-[linear-gradient(to_top,rgb(10_9_12/0.85),transparent)] px-3 pt-6 pb-2 text-left text-[12px] text-bone-100 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
                        {when}
                      </span>
                    </button>
                    <div className="absolute top-2 right-2 flex gap-1 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
                      <IconButton
                        label={t("instance.screenshots.copy")}
                        icon={<Copy size={16} />}
                        size="sm"
                        className="bg-obsidian-950/75 hover:bg-obsidian-900"
                        onClick={() => void copy(shot)}
                      />
                      <IconButton
                        label={t("instance.screenshots.delete")}
                        icon={<Trash size={16} />}
                        size="sm"
                        className="bg-obsidian-950/75 hover:bg-obsidian-900"
                        onClick={() => setDeleting(shot)}
                      />
                    </div>
                  </li>
                </ContextMenu>
              );
            })}
          </ul>
          {list.length > shown ? (
            <div className="flex justify-center">
              <Button variant="secondary" onClick={() => setShown((n) => n + PAGE)}>
                {t("instance.screenshots.more", { count: list.length - shown })}
              </Button>
            </div>
          ) : null}
        </>
      )}

      <AnimatePresence initial={false}>
        {deleted.length > 0 ? (
          <motion.div
            key="deleted"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: { duration: 0.2 } }}
            exit={{ opacity: 0, transition: { duration: 0.12 } }}
          >
            <TabSection
              title={t("instance.screenshots.deletedTitle")}
              caption={t("instance.trash.hint")}
            >
              <ul className="relative max-w-[980px] space-y-0.5">
                <AnimatePresence initial={false} mode="popLayout">
                  {deleted.map((item) => (
                    <TrashRow
                      key={item.id}
                      item={item}
                      onRestore={() => void restore(item)}
                      onPurge={() => setPurging(item)}
                    />
                  ))}
                </AnimatePresence>
              </ul>
            </TabSection>
          </motion.div>
        ) : null}
      </AnimatePresence>

      <Lightbox
        shots={list}
        index={viewing}
        onIndexChange={setViewing}
        onClose={() => setViewing(null)}
        onCopy={(shot) => void copy(shot)}
        onOpen={open}
        onReveal={reveal}
        onDelete={setDeleting}
      />
      <ConfirmDialog
        open={Boolean(deleting)}
        title={t("instance.screenshots.deleteConfirm.title")}
        body={t("instance.screenshots.deleteConfirm.body")}
        confirmLabel={t("instance.screenshots.deleteConfirm.confirm")}
        onConfirm={() => deleting && void remove(deleting)}
        onClose={() => setDeleting(null)}
      />
      <ConfirmDialog
        open={Boolean(purging)}
        title={t("instance.trash.purgeConfirm.title", {
          name: purging ? trashLabel(purging) : "",
        })}
        body={t("instance.trash.purgeConfirm.body")}
        confirmLabel={t("instance.trash.purgeConfirm.confirm")}
        onConfirm={() => purging && void purge(purging)}
        onClose={() => setPurging(null)}
      />
    </div>
  );
}
