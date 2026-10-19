// The instance's "Genel" tab: where to play next ("Devam et", the last worlds, the newest
// screenshots) beside what the instance is (mod health, versions, the room it takes, quick ways
// to add content) and its featured server. Play time and the last session stay in the header.
import { Skeleton, useMotionPrefs } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { motion } from "motion/react";
import { useEffect, useId, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useResumeTarget } from "@/features/resume/resume-button";
import { api, type ContentKind, type InstanceDetail, type WorldDetail } from "@/lib/api";
import { ModpackCard } from "../modpack-update";
import { canQuickPlay } from "../quick-play";
import { useGameRunning } from "../use-game-running";
import { LeadCard } from "./lead-card";
import {
  featuredServer,
  instanceBytes,
  leadCard,
  neverPlayed,
  recentShots,
  recentWorlds,
} from "./model";
import { type OverviewWorld, ServerSection, ShotsSection, WorldsSection } from "./sections";
import { SidePanel } from "./side-panel";

export type OverviewTarget = "worlds" | "servers" | "screenshots" | "modcenter";

const fromDetail = (world: WorldDetail): OverviewWorld => ({
  folder: world.folder,
  name: world.name,
  lastPlayedAt: world.lastPlayedAt,
  iconUrl: world.iconUrl,
  mode: world.hardcore ? "hardcore" : world.gameMode,
  readable: world.readable,
});

/**
 * Shaped like the play column, at its sizes: the card (83 px), two world rows (49.5 px each)
 * under a section head, three screenshot tiles; the content then fades in where it stood.
 */
function PlaySkeleton() {
  return (
    <div aria-hidden className="space-y-5">
      <Skeleton className="h-[83px] rounded-[var(--radius-panel)]" />
      <div>
        <Skeleton className="mt-2 mb-3.5 ml-1 h-3.5 w-24" />
        {[0, 1].map((row) => (
          <div key={row} className="flex h-[49.5px] items-center gap-3 pl-1">
            <Skeleton className="size-9 shrink-0" />
            <div className="flex-1 space-y-2.5">
              <Skeleton className="h-3.5 w-40" />
              <Skeleton className="h-3 w-24" />
            </div>
          </div>
        ))}
      </div>
      <div>
        <Skeleton className="mt-2 mb-3.5 ml-1 h-3.5 w-40" />
        <div className="grid grid-cols-3 gap-2.5">
          {[0, 1, 2].map((tile) => (
            <Skeleton key={tile} className="aspect-video rounded-[var(--radius-panel)]" />
          ))}
        </div>
      </div>
    </div>
  );
}

export function OverviewTab({
  instance,
  onOpenTab,
  onBrowse,
}: {
  instance: InstanceDetail;
  onOpenTab: (tab: OverviewTarget) => void;
  onBrowse: (kind: ContentKind) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const motionPrefs = useMotionPrefs();
  const notesId = useId();
  const id = instance.id;
  const locked = useGameRunning(id);
  // The worlds and screenshots tabs' own queries: what one reads, the other shows at once.
  const worlds = useQuery({ queryKey: ["worlds", id], queryFn: () => api.library.worlds(id) });
  const shots = useQuery({
    queryKey: ["screenshots", id],
    queryFn: () => api.library.screenshots(id),
  });
  const backups = useQuery({
    queryKey: ["world-backups", id],
    queryFn: () => api.library.backups(id),
  });
  // Read here so the modpack card arrives with the column instead of pushing it down later.
  const modpack = useQuery({
    queryKey: ["modpack-info", id],
    queryFn: () => api.content.modpackInfo(id),
  });
  const resume = useResumeTarget(instance);

  // A session changes the worlds, the play time, the server list and the folder's size: read
  // them again once the game closes (the screenshots follow the session's own notice).
  const wasLocked = useRef(locked);
  useEffect(() => {
    if (wasLocked.current && !locked) {
      for (const key of ["worlds", "instance", "instance-size"]) {
        void queryClient.invalidateQueries({ queryKey: [key, id] });
      }
    }
    wasLocked.current = locked;
  }, [locked, id, queryClient]);

  const ready = !worlds.isPending && !shots.isPending && !modpack.isPending && resume !== undefined;
  // Content that replaces the shimmer fades in; content that was already there just shows.
  const shimmered = useRef(false);
  if (!ready) shimmered.current = true;

  // The worlds list knows which worlds cannot be read; the instance's own copy is the fallback.
  const worldList: OverviewWorld[] = worlds.data
    ? worlds.data.map(fromDetail)
    : instance.worlds.map((world) => ({
        folder: world.folder,
        name: world.name,
        lastPlayedAt: world.lastPlayedAt,
        iconUrl: world.iconUrl,
        mode: world.gameMode,
        readable: true,
      }));
  const shotList = shots.data ?? [];
  const target = resume ?? null;
  const worldPlan = recentWorlds(worldList, target);
  const serverPlan = featuredServer(instance.servers, target);
  const shotPlan = recentShots(shotList);
  const lead = leadCard({
    played: !neverPlayed(instance),
    resume: Boolean(target),
    worlds: worldList.length,
    shots: shotList.length,
    server: Boolean(serverPlan.server),
  });

  // What the lists above add up to: a change there (a world, a screenshot, a backup, a mod) means
  // the folder changed, and it stands in if the folder cannot be measured.
  const known =
    worlds.isPending || shots.isPending || backups.isPending
      ? null
      : instanceBytes({
          mods: instance.mods,
          resourcePacks: instance.resourcePacks,
          shaderPacks: instance.shaderPacks,
          worlds: worlds.data ?? instance.worlds,
          screenshots: shots.data,
          backups: backups.data,
        });
  // The whole folder (every version set, settings, logs): measured off the UI thread and answered
  // from the backend's cache while its top levels are unchanged, so every visit asks again for
  // free. Leaving the tab stops a walk still running (the query's signal).
  const folder = useQuery({
    queryKey: ["instance-size", id],
    queryFn: ({ signal }) => api.library.diskUsage(id, signal),
    retry: false,
  });
  const seen = useRef(known);
  useEffect(() => {
    if (known !== null && seen.current !== null && seen.current !== known) {
      void queryClient.invalidateQueries({ queryKey: ["instance-size", id] });
    }
    if (known !== null) seen.current = known;
  }, [known, id, queryClient]);
  // A never downloaded instance holds only its own small files (the instance, its server list):
  // it says "İndirilmedi" rather than "892 B", unless the player already put things in it. Its
  // size waits for the lists, so the bytes never flash before the words.
  const neverInstalled = instance.state === "needsInstall";
  const notDownloaded = neverInstalled && known === 0;
  const sizeBytes =
    neverInstalled && known === null ? null : (folder.data ?? (folder.isError ? known : null));

  return (
    <div className="grid max-w-[980px] items-start gap-x-6 gap-y-5 md:grid-cols-[minmax(0,1fr)_clamp(264px,32%,320px)]">
      <div className="min-w-0" aria-busy={!ready || undefined}>
        {ready ? (
          <motion.div
            initial={shimmered.current ? { opacity: 0 } : false}
            animate={{ opacity: 1, transition: { duration: 0.2, ease: "easeOut" } }}
            className="space-y-5"
          >
            {modpack.data ? (
              // The card brings its own top margin and width for its old place.
              <div className="[&>section]:mt-0 [&>section]:max-w-none">
                <ModpackCard instance={instance} />
              </div>
            ) : null}
            {lead ? <LeadCard kind={lead} instanceId={id} target={target} locked={locked} /> : null}
            {worldPlan.shown.length > 0 ? (
              <WorldsSection
                instanceId={id}
                worlds={worldPlan.shown}
                others={worldPlan.others}
                more={worldPlan.more}
                quickPlay={canQuickPlay(instance.mcVersion)}
                locked={locked}
                onAll={() => onOpenTab("worlds")}
              />
            ) : null}
            {shotPlan.shown.length > 0 ? (
              <ShotsSection
                instanceId={id}
                shots={shotList}
                shown={shotPlan.shown}
                more={shotPlan.more}
                zoom={motionPrefs.layout}
                onAll={() => onOpenTab("screenshots")}
              />
            ) : null}
            {instance.settings.notes ? (
              <section aria-labelledby={notesId} className="pl-1">
                <h3 id={notesId} className="font-display text-[15px] font-semibold text-bone-50">
                  {t("instance.overview.notes")}
                </h3>
                <p
                  className="mt-1.5 max-w-[70ch] text-[13.5px] whitespace-pre-wrap text-ash-300"
                  data-selectable
                >
                  {instance.settings.notes}
                </p>
              </section>
            ) : null}
          </motion.div>
        ) : (
          <PlaySkeleton />
        )}
      </div>
      {/* The featured server sits under the panel: it is the last thing in this column, so it
          arrives without moving anything, and the two columns end near each other. */}
      <div className="min-w-0 space-y-5">
        <SidePanel
          instance={instance}
          sizeBytes={sizeBytes}
          notDownloaded={notDownloaded}
          onOpenModCenter={() => onOpenTab("modcenter")}
          onBrowse={onBrowse}
        />
        {ready && serverPlan.server ? (
          <motion.div
            initial={shimmered.current ? { opacity: 0 } : false}
            animate={{ opacity: 1, transition: { duration: 0.2, ease: "easeOut" } }}
          >
            <ServerSection
              instanceId={id}
              server={serverPlan.server}
              more={serverPlan.more}
              locked={locked}
              onAll={() => onOpenTab("servers")}
            />
          </motion.div>
        ) : null}
      </div>
    </div>
  );
}
