// The overview's shortcuts: the last worlds and the featured server with their own "Oyna" and
// "Katıl", and the newest screenshots opening the full-window viewer.
import { Play } from "@phosphor-icons/react";
import { Button, Chip, ContentIcon, cn, Skeleton, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { showError } from "@/features/content/jobs";
import { api, type SavedServer, type Screenshot, type TrashItem } from "@/lib/api";
import { formatRelative } from "@/lib/format";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { ConfirmDialog } from "../parts";
import { parseServerAddress } from "../quick-play";
import { Lightbox } from "../tabs/lightbox";
import { shotTime } from "../trash-label";
import { formatWhen } from "../when";
import { IconTile, SectionHead, ServerPlayers, useServerPing } from "./parts";

/** A world as the overview lists it (from the worlds list, or the instance's own copy). */
export interface OverviewWorld {
  folder: string;
  name: string;
  lastPlayedAt?: string;
  iconUrl?: string;
  /** Hardcore is a mode of its own here (it gets the red chip). */
  mode: "survival" | "creative" | "adventure" | "spectator" | "hardcore" | "unknown";
  readable: boolean;
}

const playIcon = <Play size={14} weight="fill" className="text-gold-300" />;

function ModeChip({ mode }: { mode: OverviewWorld["mode"] }) {
  const { t } = useTranslation();
  if (mode === "unknown") return null;
  return (
    <Chip tone={mode === "hardcore" ? "flare" : "neutral"}>
      {t(`instance.worlds.modes.${mode}`)}
    </Chip>
  );
}

export function WorldsSection({
  instanceId,
  worlds,
  others,
  more,
  quickPlay,
  locked,
  onAll,
}: {
  instanceId: string;
  worlds: OverviewWorld[];
  /** The card above shows the last world: these are the others. */
  others: boolean;
  more: boolean;
  /** The game can open a world right after start (1.20+). */
  quickPlay: boolean;
  locked: boolean;
  onAll: () => void;
}) {
  const { t } = useTranslation();
  const headId = useId();
  const play = useLaunch((s) => s.play);
  const selectInstance = usePrefs((s) => s.selectInstance);
  return (
    <section aria-labelledby={headId}>
      <SectionHead
        id={headId}
        title={others ? t("overview.otherWorlds") : t("overview.worlds")}
        onAll={more ? onAll : undefined}
      />
      <ul>
        {worlds.map((world) => (
          <WorldRow
            key={world.folder}
            world={world}
            quickPlay={quickPlay}
            locked={locked}
            onPlay={() => {
              selectInstance(instanceId);
              void play(instanceId, { quickPlay: { kind: "singleplayer", world: world.folder } });
            }}
          />
        ))}
      </ul>
    </section>
  );
}

function WorldRow({
  world,
  quickPlay,
  locked,
  onPlay,
}: {
  world: OverviewWorld;
  quickPlay: boolean;
  locked: boolean;
  onPlay: () => void;
}) {
  const { t } = useTranslation();
  const nameId = useId();
  return (
    <li className="flex items-center gap-3 py-1.5 pl-1">
      <IconTile
        src={world.iconUrl}
        pixelated
        size="size-9 [--cut:5px]"
        fallback={<ContentIcon kind="world" size={16} />}
      />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <span id={nameId} className="truncate text-[13.5px] font-semibold text-bone-50">
            {world.name}
          </span>
          <ModeChip mode={world.mode} />
          {world.readable ? null : <Chip tone="flare">{t("instance.worlds.unreadable")}</Chip>}
        </div>
        <p className="truncate text-[12.5px] text-ash-300">
          {world.lastPlayedAt
            ? t("instance.worlds.lastPlayed", { when: formatRelative(world.lastPlayedAt) })
            : t("instance.worlds.neverPlayed")}
        </p>
      </div>
      {quickPlay ? (
        <Button
          size="sm"
          variant="secondary"
          icon={playIcon}
          disabled={locked || !world.readable}
          aria-describedby={nameId}
          onClick={onPlay}
        >
          {t("instance.worlds.play")}
        </Button>
      ) : null}
    </li>
  );
}

export function ServerSection({
  instanceId,
  server,
  more,
  locked,
  onAll,
}: {
  instanceId: string;
  server: SavedServer;
  more: boolean;
  locked: boolean;
  onAll: () => void;
}) {
  const { t } = useTranslation();
  const headId = useId();
  const nameId = useId();
  const play = useLaunch((s) => s.play);
  const selectInstance = usePrefs((s) => s.selectInstance);
  const ping = useServerPing(server.address);
  const target = parseServerAddress(server.address);
  return (
    <section aria-labelledby={headId}>
      <SectionHead id={headId} title={t("overview.servers")} onAll={more ? onAll : undefined} />
      <div className="flex items-center gap-3 py-1.5 pl-1">
        <IconTile
          src={server.iconUrl ?? ping.data?.iconUrl}
          pixelated
          size="size-9 [--cut:5px]"
          fallback={<ContentIcon kind="server" size={16} />}
        />
        <div className="min-w-0 flex-1">
          <p id={nameId} className="truncate text-[13.5px] font-semibold text-bone-50">
            {server.name}
          </p>
          <div className="text-[12.5px] text-ash-300">
            <ServerPlayers address={server.address} />
          </div>
        </div>
        <Button
          size="sm"
          variant="secondary"
          icon={playIcon}
          disabled={locked || !target}
          aria-describedby={nameId}
          onClick={() => {
            if (!target) return;
            selectInstance(instanceId);
            void play(instanceId, {
              quickPlay: { kind: "multiplayer", host: target.host, port: target.port },
            });
          }}
        >
          {t("instance.servers.join")}
        </Button>
      </div>
    </section>
  );
}

/**
 * A tile's picture: the cached thumbnail, or one made now (the original when none can be
 * made). A shimmer holds the space until it is decoded, then it fades in; under the pointer it
 * leans in a little (not with reduced motion or in Performance mode).
 */
function ShotThumb({
  instanceId,
  shot,
  zoom,
}: {
  instanceId: string;
  shot: Screenshot;
  zoom: boolean;
}) {
  const [loaded, setLoaded] = useState(false);
  const [broken, setBroken] = useState(false);
  // The gallery's own query: a thumbnail made here is there too.
  const made = useQuery({
    queryKey: ["screenshot-thumb", instanceId, shot.fileName, shot.sizeBytes, shot.takenAt],
    queryFn: () => api.library.screenshotThumb(instanceId, shot.fileName),
    enabled: !shot.thumbUrl,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
  const thumb = shot.thumbUrl ?? made.data;
  const src = broken || made.data === null || made.isError ? shot.url : thumb;
  return (
    <span className="relative block aspect-video w-full bg-obsidian-800">
      {loaded ? null : <Skeleton className="absolute inset-0 rounded-none" />}
      {src ? (
        <img
          ref={(img) => {
            if (img?.complete && img.naturalWidth > 0) setLoaded(true);
          }}
          src={src}
          alt=""
          decoding="async"
          draggable={false}
          onLoad={() => setLoaded(true)}
          onError={() => {
            if (src !== shot.url) setBroken(true);
          }}
          className={cn(
            "relative block aspect-video w-full object-cover transition-[opacity,transform,scale] duration-300 ease-[cubic-bezier(0.16,1,0.3,1)]",
            zoom && "group-hover:scale-[1.03]",
            loaded ? "opacity-100" : "opacity-0",
          )}
        />
      ) : null}
    </span>
  );
}

/** Copy, open, show in the folder and delete (with "Geri al"), as the gallery does them. */
function useShotActions(instanceId: string) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ["screenshots", instanceId] });
    void queryClient.invalidateQueries({ queryKey: ["trash", instanceId] });
  };
  const restore = async (item: TrashItem) => {
    try {
      await api.library.restoreFromTrash(instanceId, item.id);
      toast.success(t("instance.screenshots.restored"));
    } catch (error) {
      showError(error);
    } finally {
      refresh();
    }
  };
  return {
    copy: async (shot: Screenshot) => {
      try {
        const result = await api.library.copyScreenshot(instanceId, shot.fileName);
        if (result === "image") toast.success(t("instance.screenshots.copied"));
        else toast(t("instance.screenshots.copiedPath"));
      } catch (error) {
        showError(error);
      }
    },
    open: (shot: Screenshot) =>
      void api.library.openScreenshot(instanceId, shot.fileName).catch(showError),
    reveal: (shot: Screenshot) =>
      void api.library.revealFile(instanceId, "screenshots", shot.fileName).catch(showError),
    remove: async (shot: Screenshot) => {
      queryClient.setQueryData<Screenshot[]>(["screenshots", instanceId], (prev) =>
        prev?.filter((s) => s.fileName !== shot.fileName),
      );
      try {
        const item = await api.library.deleteScreenshot(instanceId, shot.fileName);
        toast(t("instance.screenshots.deleted"), {
          action: { label: t("instance.trash.undo"), onClick: () => void restore(item) },
        });
      } catch (error) {
        showError(error);
      } finally {
        refresh();
      }
    },
  };
}

export function ShotsSection({
  instanceId,
  shots,
  shown,
  more,
  zoom,
  onAll,
}: {
  instanceId: string;
  /** Every screenshot, newest first: the viewer moves through all of them. */
  shots: Screenshot[];
  shown: Screenshot[];
  more: boolean;
  zoom: boolean;
  onAll: () => void;
}) {
  const { t } = useTranslation();
  const headId = useId();
  const actions = useShotActions(instanceId);
  const [viewing, setViewing] = useState<number | null>(null);
  const [deleting, setDeleting] = useState<Screenshot | null>(null);

  // Keep the viewer on a picture that still exists after a delete.
  useEffect(() => {
    if (viewing !== null && viewing >= shots.length) {
      setViewing(shots.length > 0 ? shots.length - 1 : null);
    }
  }, [viewing, shots.length]);

  return (
    <section aria-labelledby={headId}>
      <SectionHead id={headId} title={t("overview.shots")} onAll={more ? onAll : undefined} />
      <ul className="grid grid-cols-3 gap-2.5">
        {shown.map((shot, index) => (
          <li key={shot.fileName}>
            <button
              type="button"
              onClick={() => setViewing(index)}
              aria-label={t("instance.screenshots.view", {
                when: formatWhen(shotTime(shot)) || t("instance.screenshots.untitled"),
              })}
              className="group relative block w-full cursor-zoom-in overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-800 focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:ring-offset-2 focus-visible:ring-offset-obsidian-950 focus-visible:outline-none"
            >
              <ShotThumb instanceId={instanceId} shot={shot} zoom={zoom} />
            </button>
          </li>
        ))}
      </ul>
      <Lightbox
        shots={shots}
        index={viewing}
        onIndexChange={setViewing}
        onClose={() => setViewing(null)}
        onCopy={(shot) => void actions.copy(shot)}
        onOpen={actions.open}
        onReveal={actions.reveal}
        onDelete={setDeleting}
      />
      <ConfirmDialog
        open={Boolean(deleting)}
        title={t("instance.screenshots.deleteConfirm.title")}
        body={t("instance.screenshots.deleteConfirm.body")}
        confirmLabel={t("instance.screenshots.deleteConfirm.confirm")}
        onConfirm={() => deleting && void actions.remove(deleting)}
        onClose={() => setDeleting(null)}
      />
    </section>
  );
}
