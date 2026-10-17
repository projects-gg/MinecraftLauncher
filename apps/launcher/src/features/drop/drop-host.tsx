// Files dragged anywhere onto the window. As soon as a drag enters, the backend reads what the
// files are and a "Buraya bırak" zone shows where they will go: the open instance page's
// instance, else the one selected in the command bar. Mods go into its active set, resource and
// shader packs into their folders, a modpack opens the import flow; unknown files get one short
// notice. Places with their own drop handling (skin panel, the import step) take drops
// themselves and empty tabs show the zone in place (`useDropZone`); otherwise the zone below
// covers the window.
import { Package, Palette, PuzzlePiece, Question, SunHorizon } from "@phosphor-icons/react";
import { Chip, cn, DropZone, EntranceGate, easeOutExpo, GatedMotion, toast, usePerf } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { api, type DroppedFile, type InstanceSummary } from "@/lib/api";
import { playSound } from "@/lib/sound";
import {
  type DropNotice,
  type DropPlan,
  dropChips,
  dropNotices,
  leftOut,
  planDrop,
  type RowKind,
} from "./model";
import { startDragSource, useDrop, useDropAnnouncement } from "./store";
import { toTarget, useDropPlan, useDropTarget } from "./target";
import { announcementOf, windowHint, zoneTitle } from "./text";
import { WINDOW_INSET, WINDOW_ZONE, zoneState } from "./zones";

const kindIcons: Record<RowKind, typeof PuzzlePiece> = {
  mod: PuzzlePiece,
  resourcePack: Palette,
  shaderPack: SunHorizon,
  modpack: Package,
  unknown: Question,
};

/** Under the window zone's line: what is in the drag, kind by kind, and why some stays out. */
function PlanChips({ plan }: { plan: DropPlan }) {
  const { t } = useTranslation();
  const rows = dropChips(plan);
  const reason = leftOut(plan);
  if (rows.length === 0 && !reason) return null;
  return (
    <motion.div
      className="flex flex-col items-center"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: { duration: 0.2, ease: easeOutExpo } }}
    >
      {rows.length > 0 ? (
        <div className="flex flex-wrap justify-center gap-1.5">
          {rows.map((row) => {
            const Icon = kindIcons[row.kind];
            const muted = row.kind === "unknown" || Boolean(row.blocked);
            return (
              <Chip key={row.kind} className={cn("gap-1.5", muted && "bg-white/5 text-ash-300")}>
                <Icon size={12} weight="bold" />
                {t(`drop.chips.${row.kind}`, { count: row.count })}
              </Chip>
            );
          })}
        </div>
      ) : null}
      {reason ? (
        <p className="mt-2 text-[12.5px] text-ash-300">{t(`drop.blocked.${reason}`)}</p>
      ) : null}
    </motion.div>
  );
}

/** The window-wide drop zone (mounted once in the shell) and the window's own drop handling. */
export function DropHost({ disabled = false }: { disabled?: boolean }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const target = useDropTarget();
  const { performanceMode } = usePerf();
  const live = useRef({ target, disabled });
  live.current = { target, disabled };

  useEffect(() => {
    const announce = (into: InstanceSummary, notices: DropNotice[]) => {
      // The newest toast sits in front: the most important one goes last.
      for (const notice of [...notices].reverse()) {
        const message = t(`drop.notices.${notice.key}`, notice.values);
        const tab = notice.show;
        const action = tab
          ? {
              label: t("drop.show"),
              onClick: () =>
                void navigate({
                  to: "/library/$instanceId",
                  params: { instanceId: into.id },
                  search: { tab },
                }),
            }
          : undefined;
        if (notice.tone === "success") toast.success(message, { action });
        else if (notice.tone === "error") toast.error(message);
        else toast(message);
      }
    };

    const finish = async (paths: string[], known: DroppedFile[] | null) => {
      const into = live.current.target;
      const files = known ?? (await api.classifyFiles(paths).catch(() => [] as DroppedFile[]));
      const modpack = files.find((f) => f.kind === "modpack");
      const copies = files.some(
        (f) => f.kind === "mod" || f.kind === "resourcePack" || f.kind === "shaderPack",
      );
      if (copies && into) {
        try {
          const result = await api.dropFiles(into.id, paths);
          announce(into, dropNotices(result));
          if (result.added.length > 0) {
            playSound("success");
            void queryClient.invalidateQueries({ queryKey: ["instance", into.id] });
            void queryClient.invalidateQueries({ queryKey: ["content-scan", into.id] });
          }
        } catch (error) {
          toast.error(
            error instanceof Error && error.message
              ? error.message
              : t("drop.notices.failedMany", { count: paths.length }),
          );
        }
      } else {
        const unknown = files.filter((f) => f.kind === "unknown");
        const first = unknown[0];
        if (unknown.length === 1 && first) {
          toast(t("drop.notices.unknownOne", { name: first.name }));
        } else if (unknown.length > 1) {
          toast(t("drop.notices.unknownMany", { count: unknown.length }));
        }
        if (copies && !into) toast(t("drop.blocked.noInstance"));
      }
      if (modpack) {
        useDrop.getState().setPendingPack(modpack.path);
        void navigate({ to: "/library", search: { create: true } });
      }
    };

    return startDragSource({
      windowEnabled: () => !live.current.disabled,
      windowAccepts: (drag) =>
        drag.files ? planDrop(drag.files, toTarget(live.current.target)).acceptable : true,
      onWindowDrop: (paths, files) => void finish(paths, files),
    });
  }, [navigate, queryClient, t]);

  const drag = useDrop((s) => s.drag);
  const windowActive = useDrop((s) => s.drag !== null && s.active.includes(WINDOW_ZONE));
  const hovered = useDrop((s) => s.hovered);
  const absorbing = useDrop((s) => s.absorbing);
  const taken = useDrop((s) => s.taken);
  const announcement = useDrop((s) => s.announcement);
  const absorbed = absorbing === WINDOW_ZONE;
  // While it absorbs, the zone keeps the files it took.
  const shown = windowActive ? drag : absorbed ? taken : null;
  const plan = useDropPlan(shown?.files, target);
  const state = zoneState(WINDOW_ZONE, {
    active: windowActive,
    accepts: plan ? plan.acceptable : true,
    hovered,
    hoveredAccepts: true,
    absorbing,
  });
  const title = zoneTitle(t, state, shown?.paths.length ?? 1);
  const hint = windowHint(t, plan, target?.name, false);
  useDropAnnouncement(windowActive && hint ? announcementOf(state, title, hint) : null, drag?.id);

  return (
    <>
      <div role="status" aria-live="polite" aria-atomic="true" className="sr-only">
        {announcement}
      </div>
      <AnimatePresence>
        {shown ? (
          // The layer mounts as the drag enters: its fade waits until it is on screen, so the
          // entrance never loses its start (docs/design.md §5).
          <EntranceGate key="drop">
            <motion.div
              className="pointer-events-none fixed inset-0 z-[70]"
              exit={{ opacity: 0, transition: { duration: 0.14, ease: easeOutExpo } }}
            >
              <GatedMotion.div
                className="absolute inset-0"
                initial={{ opacity: 0 }}
                animate={{
                  opacity: absorbed ? 0 : 1,
                  transition: absorbed
                    ? { duration: 0.26, ease: easeOutExpo, delay: 0.06 }
                    : { duration: 0.18, ease: easeOutExpo },
                }}
              >
                <div className="absolute inset-0 bg-obsidian-950/72" />
                {performanceMode ? null : (
                  <div className="absolute inset-0 bg-[radial-gradient(560px_circle_at_50%_50%,rgb(232_154_44/0.08),transparent_70%)]" />
                )}
              </GatedMotion.div>
              <DropZone
                aria-hidden
                variant="window"
                state={state}
                title={title}
                hint={hint}
                below={plan ? <PlanChips plan={plan} /> : undefined}
                className="absolute"
                style={{ inset: WINDOW_INSET }}
              />
            </motion.div>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </>
  );
}
