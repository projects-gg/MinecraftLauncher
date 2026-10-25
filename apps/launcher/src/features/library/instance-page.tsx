import {
  ArrowLeft,
  ArrowsLeftRight,
  DotsThreeVertical,
  Export,
  FolderOpen,
  MonitorPlay,
  Play,
} from "@phosphor-icons/react";
import { Button, IconButton, Menu, Skeleton, TabPanel, Tabs, Tooltip } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ContentBrowser } from "@/features/content/content-browser";
import { ExportDialog } from "@/features/content/export-dialog";
import { ModCenter } from "@/features/modcenter/mod-center";
import { VersionDialog } from "@/features/modcenter/version-dialog";
import { api, type ContentKind } from "@/lib/api";
import { formatDuration, formatRelative } from "@/lib/format";
import { InstanceIcon } from "@/shell/instance-icon";
import { settingsChanged, useDrafts } from "@/stores/drafts";
import { isOpening, useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { type InstanceTab, instanceTab } from "./instance-tab";
import { OverviewTab } from "./overview/overview-tab";
import { createDesktopShortcut } from "./shortcut";
import { LogsTab } from "./tabs/logs-tab";
import { ModsTab } from "./tabs/mods-tab";
import { PacksTab } from "./tabs/packs-tab";
import { ScreenshotsTab } from "./tabs/screenshots-tab";
import { ServersTab } from "./tabs/servers-tab";
import { SettingsTab } from "./tabs/settings-tab";
import { WorldsTab } from "./tabs/worlds-tab";

export type { InstanceTab } from "./instance-tab";

const browseKinds: ContentKind[] = ["mod", "resourcePack", "shaderPack"];
/** The tab the browser of each kind returns to. */
const tabOfKind: Record<ContentKind, InstanceTab> = {
  mod: "mods",
  resourcePack: "resourcePacks",
  shaderPack: "shaders",
};

export function InstancePage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { instanceId } = useParams({ from: "/library/$instanceId" });
  const search = useSearch({ from: "/library/$instanceId" });
  const browse = browseKinds.find((k) => k === search.browse);
  const [exporting, setExporting] = useState(false);
  const [versionsOpen, setVersionsOpen] = useState(false);
  const { data, isPending } = useQuery({
    queryKey: ["instance", instanceId],
    queryFn: () => api.instance(instanceId),
  });
  const selectInstance = usePrefs((s) => s.selectInstance);
  const play = useLaunch((s) => s.play);
  // Hazırlanıyor (files), Açılıyor (until the game's window is on screen), Oyunda.
  const launchKind = useLaunch((s) => {
    if (s.instanceId !== instanceId || s.phase.kind === "error") return "idle";
    if (s.phase.kind === "working") return "preparing";
    return isOpening(s.phase) ? "opening" : s.phase.kind;
  });
  // Unsaved settings and unapplied mod choices wait in their tabs (stores/drafts.ts): the tab
  // shows a small marker so they are not forgotten.
  const settingsDraft = useDrafts((s) => s.settings[instanceId]);
  const modsPending = useDrafts((s) => Boolean(s.mods[instanceId]));

  const setTab = (next: InstanceTab) =>
    void navigate({
      to: "/library/$instanceId",
      params: { instanceId },
      search: { tab: next },
      replace: true,
    });
  const openBrowser = (kind: ContentKind) =>
    void navigate({
      to: "/library/$instanceId",
      params: { instanceId },
      search: { tab: tabOfKind[kind], browse: kind },
    });

  if (isPending || !data) {
    // Shaped like the page: back link, the header (icon, name, meta, actions), the tab row and
    // the overview's two columns (its card, its side panel).
    return (
      <div className="flex min-h-0 flex-1 flex-col px-page pt-2" aria-busy>
        <Skeleton className="mb-2 h-[26px] w-24" />
        <div className="flex items-center gap-4">
          <Skeleton className="size-14 shrink-0" />
          <div className="min-w-0 flex-1 space-y-2">
            <Skeleton className="h-6 w-56" />
            <Skeleton className="h-3.5 w-72" />
          </div>
          <Skeleton className="h-8 w-64 shrink-0" />
        </div>
        <Skeleton className="mt-3.5 h-[34px] w-full max-w-[880px]" />
        <div className="mt-4 grid max-w-[980px] items-start gap-x-6 md:grid-cols-[minmax(0,1fr)_clamp(264px,32%,320px)]">
          <Skeleton className="h-[83px] rounded-[var(--radius-panel)]" />
          <Skeleton className="hidden h-[245px] rounded-[var(--radius-panel)] md:block" />
        </div>
      </div>
    );
  }

  const loader = data.loader === "vanilla" ? "Minecraft" : t(`loaders.${data.loader}`);
  // The tab a link asked for, when this instance has it (else the overview).
  const tab = instanceTab(search.tab, data.loader === "vanilla");

  return (
    <div className="flex min-h-0 flex-1 flex-col px-page pt-2">
      <Link
        to="/library"
        className="-ml-1.5 mb-2 flex w-fit items-center gap-1.5 rounded-md px-1.5 py-1 text-[12.5px] text-ash-300 transition-colors hover:text-bone-50"
      >
        <ArrowLeft size={14} /> {t("instance.back")}
      </Link>
      <header className="flex items-center gap-4">
        <InstanceIcon icon={data.icon} size={56} />
        <div className="min-w-0 flex-1">
          <h1 className="page-title truncate">{data.name}</h1>
          {/* Said the way the command bar says it ("… oynandı", "Son oynama …"): bare figures
              side by side read as one number. The loader's own version is on "Genel". "Resmî
              paket" leads the line: a fact about the instance, not a pill beside its name
              (projects-taste, SMALL11). */}
          <p className="mt-0.5 flex flex-wrap gap-x-3 text-[12.5px] text-ash-300 tabular-nums">
            {data.managed ? <span>{t("instances.managed")}</span> : null}
            <span>
              {loader} {data.mcVersion}
            </span>
            {data.playTimeSecs > 0 ? (
              <span>{t("play.playTime", { time: formatDuration(data.playTimeSecs) })}</span>
            ) : null}
            <span>
              {data.lastPlayedAt
                ? t("play.lastPlayed", { when: formatRelative(data.lastPlayedAt) })
                : t("play.neverPlayed")}
            </span>
          </p>
        </div>
        <div className="flex shrink-0 gap-1.5">
          <Tooltip content={t("modCenter.version.change")} side="bottom">
            <Button
              variant="ghost"
              icon={<ArrowsLeftRight size={16} />}
              aria-label={t("modCenter.version.changeFor", { version: data.mcVersion })}
              onClick={() => setVersionsOpen(true)}
            >
              {data.mcVersion}
            </Button>
          </Tooltip>
          <Button
            variant="ghost"
            icon={<FolderOpen size={16} />}
            onClick={() => void api.openInstanceFolder(data.id)}
          >
            {t("instance.openFolder")}
          </Button>
          <Menu
            trigger={
              <IconButton
                label={t("library.card.more")}
                icon={<DotsThreeVertical size={18} weight="bold" />}
              />
            }
            items={[
              {
                label: t("modpack.export.menu"),
                icon: <Export size={16} />,
                onSelect: () => setExporting(true),
              },
              {
                label: t("shortcut.action"),
                icon: <MonitorPlay size={16} />,
                onSelect: () => void createDesktopShortcut(data),
              },
            ]}
          />
          {/* Follows this instance's launch like OYNA does (another one's launch: "Oyna" says
              that only one game runs at a time). */}
          <Button
            variant="primary"
            icon={<Play size={15} weight="fill" />}
            disabled={launchKind !== "idle"}
            onClick={() => {
              selectInstance(data.id);
              void play(data.id);
            }}
          >
            {launchKind === "running"
              ? t("play.running")
              : launchKind === "opening"
                ? t("playOpen.opening")
                : launchKind === "preparing"
                  ? t("playOpen.preparing")
                  : t("instance.play")}
          </Button>
        </div>
      </header>

      {browse ? (
        <ContentBrowser instance={data} kind={browse} onBack={() => setTab(tabOfKind[browse])} />
      ) : (
        <Tabs
          value={tab}
          onValueChange={setTab}
          label={data.name}
          className="mt-3.5"
          items={[
            { value: "overview", label: t("instance.tabs.overview") },
            ...(data.loader === "vanilla"
              ? []
              : [
                  {
                    value: "modcenter" as InstanceTab,
                    label: t("modCenter.tab"),
                    marker: modsPending ? t("instance.tabs.pendingMarker") : undefined,
                  },
                ]),
            {
              value: "mods",
              label: t("instance.tabs.mods"),
              count: data.loader === "vanilla" ? undefined : data.mods.length || undefined,
            },
            {
              value: "resourcePacks",
              label: t("instance.tabs.resourcePacks"),
              count: data.resourcePacks.length || undefined,
            },
            {
              value: "shaders",
              label: t("instance.tabs.shaders"),
              count: data.shaderPacks.length || undefined,
            },
            {
              value: "worlds",
              label: t("instance.tabs.worlds"),
              count: data.worlds.length || undefined,
            },
            { value: "screenshots", label: t("instance.tabs.screenshots") },
            {
              value: "servers",
              label: t("instance.tabs.servers"),
              count: data.servers.length || undefined,
            },
            { value: "logs", label: t("instance.tabs.logs") },
            {
              value: "settings",
              label: t("instance.tabs.settings"),
              marker:
                settingsDraft && settingsChanged(settingsDraft, data.settings)
                  ? t("instance.tabs.unsavedMarker")
                  : undefined,
            },
          ]}
        >
          {/* Play time and the last session are the header's; the tab says where to play next
              and what the instance is (overview/overview-tab.tsx). */}
          <TabPanel value="overview" className="pb-8">
            <OverviewTab instance={data} onOpenTab={setTab} onBrowse={openBrowser} />
          </TabPanel>
          <TabPanel
            value="modcenter"
            className="flex flex-col pb-0 [mask-image:linear-gradient(to_bottom,black_calc(100%-10px),transparent)]"
          >
            <ModCenter instance={data} onSwitchVersion={() => setVersionsOpen(true)} />
          </TabPanel>
          <TabPanel value="mods" className="pb-8">
            <ModsTab
              instance={data}
              onBrowse={() => openBrowser("mod")}
              onOpenModCenter={() => setTab("modcenter")}
            />
          </TabPanel>
          <TabPanel value="resourcePacks" className="pb-8">
            <PacksTab
              instanceId={data.id}
              kind="resourcePack"
              packs={data.resourcePacks}
              onBrowse={() => openBrowser("resourcePack")}
            />
          </TabPanel>
          <TabPanel value="shaders" className="pb-8">
            <PacksTab
              instanceId={data.id}
              kind="shaderPack"
              packs={data.shaderPacks}
              needsLoader={data.loader === "vanilla"}
              onBrowse={() => openBrowser("shaderPack")}
            />
          </TabPanel>
          <TabPanel value="worlds" className="pb-8">
            <WorldsTab instance={data} />
          </TabPanel>
          <TabPanel value="screenshots" className="pb-8">
            <ScreenshotsTab instance={data} />
          </TabPanel>
          <TabPanel value="servers" className="pb-8">
            <ServersTab instance={data} />
          </TabPanel>
          <TabPanel value="logs" className="overflow-hidden pb-6">
            <LogsTab instance={data} />
          </TabPanel>
          {/* No bottom padding: the save bar's anchor sticks to the panel's bottom edge. */}
          <TabPanel value="settings" className="flex flex-col pb-0">
            <SettingsTab instance={data} />
          </TabPanel>
        </Tabs>
      )}
      <ExportDialog
        instance={exporting ? { id: data.id, name: data.name } : null}
        onClose={() => setExporting(false)}
      />
      <VersionDialog
        instance={data}
        open={versionsOpen}
        onClose={() => setVersionsOpen(false)}
        onOpenModCenter={data.loader === "vanilla" ? undefined : () => setTab("modcenter")}
      />
    </div>
  );
}
