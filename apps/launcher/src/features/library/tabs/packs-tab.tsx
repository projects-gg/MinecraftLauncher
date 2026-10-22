import { FilePlus, FolderOpen, Info, MagnifyingGlassPlus, Trash } from "@phosphor-icons/react";
import {
  Button,
  Chip,
  ContextMenu,
  cn,
  easeOutExpo,
  IconButton,
  SearchInput,
  useMotionPrefs,
} from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { showError, useContentActions, useUndoableRemoval } from "@/features/content/jobs";
import { ProjectDialog } from "@/features/content/project-dialog";
import { PageDropZone } from "@/features/drop/page-zone";
import { SetBadgeChip } from "@/features/modsets/parts";
import { api, type ContentKind, type PackEntry } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { matchesQuery } from "@/lib/search";
import { ContentIcon } from "@/shell/content-icon";
import { TabEmpty } from "../parts";

/** From this many packs on, the tab offers a search field. */
const SEARCH_FROM = 9;

/** Resource or shader packs of an instance. Files dropped onto the window are sorted by the
 * window-wide drop layer (features/drop), which reads what each file is; an empty tab shows its
 * "Buraya bırak" zone in place. */
export function PacksTab({
  instanceId,
  kind,
  packs,
  needsLoader = false,
  onBrowse,
}: {
  instanceId: string;
  kind: Exclude<ContentKind, "mod">;
  packs: PackEntry[];
  /** Shader packs of a vanilla instance: nothing can load them. */
  needsLoader?: boolean;
  onBrowse: () => void;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const actions = useContentActions(instanceId, kind);
  const removal = useUndoableRemoval(instanceId, kind);
  const [opened, setOpened] = useState<PackEntry | null>(null);
  const [query, setQuery] = useState("");
  const shown = packs.filter((p) => !removal.isHidden(p.fileName));
  // The search applies only while its field is there. Turkish-aware: "iris" finds "Iris".
  const searching = shown.length >= SEARCH_FROM;
  const matching = searching
    ? shown.filter((p) => matchesQuery(p.name, query) || matchesQuery(p.description, query))
    : shown;
  // The list that takes the empty zone's place (packs were just dropped or added) rises in.
  const wasEmpty = useRef(shown.length === 0);
  const arriving = wasEmpty.current && shown.length > 0;
  useEffect(() => {
    wasEmpty.current = shown.length === 0;
  });

  const addButton = (
    <Button variant="ghost" icon={<FilePlus size={17} />} onClick={() => void actions.addFiles()}>
      {t("instance.mods.addFile")}
    </Button>
  );
  const browseButton = (
    <Button
      variant="secondary"
      icon={<MagnifyingGlassPlus size={17} className="text-gold-300" />}
      onClick={onBrowse}
    >
      {t(`content.browse.title.${kind}`)}
    </Button>
  );

  if (shown.length === 0) {
    return needsLoader ? (
      <TabEmpty
        icon={<ContentIcon kind={kind} />}
        title={t("instance.packs.needsLoaderTitle")}
        body={t("instance.packs.needsLoaderBody")}
      />
    ) : (
      <PageDropZone
        kind={kind}
        label={t(`instance.packs.emptyTitle.${kind}`)}
        actions={
          <>
            {addButton}
            {browseButton}
          </>
        }
        className="max-w-[980px]"
      />
    );
  }

  const totalBytes = shown.reduce((sum, p) => sum + p.sizeBytes, 0);

  return (
    <div className="relative max-w-[980px]">
      <div className="mb-4 flex flex-wrap items-center gap-2.5">
        <p className="flex-1 text-[13px] text-ash-300 tabular-nums">
          {t(`instance.packs.summary.${kind}`, {
            count: shown.length,
            size: formatBytes(totalBytes),
          })}
        </p>
        {searching ? (
          <div className="w-56">
            <SearchInput
              value={query}
              onChange={setQuery}
              placeholder={t("instance.packs.search")}
              label={t("instance.packs.search")}
            />
          </div>
        ) : null}
        {addButton}
        {browseButton}
      </div>
      {matching.length === 0 ? (
        <div className="flex flex-col items-center gap-3 py-10 text-center">
          <p className="text-[13.5px] text-ash-300">{t("instance.packs.noMatch")}</p>
          <Button variant="ghost" size="sm" onClick={() => setQuery("")}>
            {t("instance.packs.clearSearch")}
          </Button>
        </div>
      ) : (
        <ul
          // Searching swaps the list in place (no fades and glides while typing). Two columns at
          // every window size: with 300 px a 1280 px window made three 318 px cards in the 980 px
          // column and cut names a 1024 px window showed whole (POLISH6).
          key={query.trim()}
          className={cn(
            "grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-3",
            arriving && "panel-in",
          )}
        >
          {/* A removed pack fades where it was and the others glide into its place. */}
          <AnimatePresence initial={false} mode="popLayout">
            {matching.map((p) => (
              <motion.li
                key={p.fileName}
                layout={layout ? "position" : false}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: { duration: 0.2 } }}
                exit={{ opacity: 0, transition: { duration: 0.12 } }}
                transition={{ layout: { duration: 0.3, ease: easeOutExpo } }}
              >
                <ContextMenu
                  items={[
                    ...(p.projectId
                      ? [
                          {
                            label: t("content.actions.details"),
                            icon: <Info size={16} />,
                            onSelect: () => setOpened(p),
                          },
                        ]
                      : []),
                    {
                      label: t("instance.openFolder"),
                      icon: <FolderOpen size={16} />,
                      onSelect: () =>
                        void api
                          .openInstanceFolder(
                            instanceId,
                            kind === "shaderPack" ? "shaderpacks" : "resourcepacks",
                          )
                          .catch(showError),
                    },
                    null,
                    {
                      label: t("content.actions.remove"),
                      icon: <Trash size={16} />,
                      danger: true,
                      onSelect: () => removal.remove(p.fileName, p.name),
                    },
                  ]}
                >
                  <div className="glass group flex h-full items-center gap-3 rounded-[var(--radius-panel)] p-3">
                    <span
                      className={cn(
                        "chamfer flex size-12 shrink-0 items-center justify-center overflow-hidden bg-obsidian-800 [--cut:6px]",
                        !p.enabled && "opacity-60",
                      )}
                    >
                      {p.iconUrl ? (
                        <img
                          src={p.iconUrl}
                          alt=""
                          draggable={false}
                          className="size-full object-cover [image-rendering:pixelated]"
                        />
                      ) : (
                        <ContentIcon kind={kind} quiet />
                      )}
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="flex min-w-0 items-center gap-2">
                        {p.projectId ? (
                          <button
                            type="button"
                            onClick={() => setOpened(p)}
                            className="block min-w-0 cursor-pointer truncate text-left text-[14px] font-semibold text-bone-50 transition-colors duration-150 hover:text-gold-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
                          >
                            {p.name}
                          </button>
                        ) : (
                          <p className="min-w-0 truncate text-[14px] font-semibold text-bone-50">
                            {p.name}
                          </p>
                        )}
                        {p.enabled ? null : <Chip>{t("instance.packs.disabled")}</Chip>}
                        {p.modSet ? <SetBadgeChip badge={p.modSet} compact /> : null}
                      </div>
                      {/* Two lines, as the game's own pack list shows them: a 1200 px window
                          left a single line about 170 px wide (POLISH3). */}
                      {p.description ? (
                        <p className="line-clamp-2 text-[12.5px] text-ash-300">{p.description}</p>
                      ) : null}
                    </div>
                    <span className="shrink-0 text-[12px] text-ash-300 tabular-nums">
                      {formatBytes(p.sizeBytes)}
                    </span>
                    <IconButton
                      label={t("content.actions.remove")}
                      icon={<Trash size={16} />}
                      size="sm"
                      className="opacity-100 transition-opacity [@media(hover:hover)]:opacity-0 [@media(hover:hover)]:group-focus-within:opacity-100 [@media(hover:hover)]:group-hover:opacity-100"
                      onClick={() => removal.remove(p.fileName, p.name)}
                    />
                  </div>
                </ContextMenu>
              </motion.li>
            ))}
          </AnimatePresence>
        </ul>
      )}
      <ProjectDialog
        projectId={opened?.projectId ?? null}
        hint={
          opened
            ? { title: opened.name, author: "", iconUrl: opened.iconUrl, description: "" }
            : undefined
        }
        target={{ instanceId, kind }}
        onClose={() => setOpened(null)}
      />
    </div>
  );
}
