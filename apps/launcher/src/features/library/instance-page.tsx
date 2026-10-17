import {
  ArrowLeft,
  ArrowsLeftRight,
  DotsThreeVertical,
  Export,
  FolderOpen,
  MonitorPlay,
  Play,
} from "@phosphor-icons/react";
import { Button, Chip, IconButton, Menu, Skeleton, TabPanel, Tabs, Tooltip } from "@pl/ui";
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
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { ModpackCard } from "./modpack-update";
import { createDesktopShortcut } from "./shortcut";
import { LogsTab } from "./tabs/logs-tab";
import { ModsTab } from "./tabs/mods-tab";
import { PacksTab } from "./tabs/packs-tab";
import { ScreenshotsTab } from "./tabs/screenshots-tab";
import { ServersTab } from "./tabs/servers-tab";
import { SettingsTab } from "./tabs/settings-tab";
import { WorldsTab } from "./tabs/worlds-tab";

export type InstanceTab =
  | "overview"
  | "modcenter"
  | "mods"
  | "resourcePacks"
  | "shaders"
  | "worlds"
  | "screenshots"
  | "servers"
  | "logs"
  | "settings";

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
  const tab = (search.tab ?? "overview") as InstanceTab;
  const browse = browseKinds.find((k) => k === search.browse);
  const [exporting, setExporting] = useState(false);
  const [versionsOpen, setVersionsOpen] = useState(false);
  const { data, isPending } = useQuery({
    queryKey: ["instance", instanceId],
    queryFn: () => api.instance(instanceId),
  });
  const selectInstance = usePrefs((s) => s.selectInstance);
  const play = useLaunch((s) => s.play);
  const launchKind = useLaunch((s) =>
    s.instanceId === instanceId && s.phase.kind !== "error" ? s.phase.kind : "idle",
  );
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
    // Shaped like the page: back link, the header (icon, name, meta, actions), the tab row.
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
        <div className="mt-5 grid max-w-[880px] grid-cols-4 gap-3">
          {[0, 1, 2, 3].map((i) => (
            <Skeleton key={i} className="h-[72px] rounded-[var(--radius-panel)]" />
          ))}
        </div>
      </div>
    );
  }

  const loader = data.loader === "vanilla" ? "Minecraft" : t(`loaders.${data.loader}`);
  const enabledMods = data.mods.filter((m) => m.enabled).length;

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
          <div className="flex items-center gap-2.5">
            <h1 className="page-title truncate">{data.name}</h1>
            {data.managed ? <Chip tone="gold">{t("instances.managed")}</Chip> : null}
          </div>
          {/* Said the way the command bar says it ("… oynandı", "Son oynama …"): bare figures
              side by side read as one number. The loader's own version is on "Genel". */}
          <p className="mt-0.5 flex flex-wrap gap-x-3 text-[12.5px] text-ash-300 tabular-nums">
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
              : launchKind === "idle"
                ? t("instance.play")
                : t("play.launching")}
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
          <TabPanel value="overview" className="pb-8">
            <div className="grid max-w-[880px] grid-cols-4 gap-3">
              {[
                [
                  t("instance.overview.playTime"),
                  data.playTimeSecs > 0
                    ? formatDuration(data.playTimeSecs)
                    : t("instance.overview.noPlayTime"),
                ],
                [
                  t("instance.overview.lastPlayed"),
                  data.lastPlayedAt ? formatRelative(data.lastPlayedAt) : t("play.neverPlayed"),
                ],
                [t("instance.overview.version"), data.mcVersion],
                [
                  t("instance.overview.loader"),
                  data.loader === "vanilla"
                    ? t("loaders.vanilla")
                    : `${loader} ${data.loaderVersion ?? ""}`,
                ],
              ].map(([label, value]) => (
                <div key={label} className="glass rounded-[var(--radius-panel)] px-4 py-3.5">
                  <p className="text-[12px] text-ash-400">{label}</p>
                  <p className="mt-1 truncate font-display text-[18px] font-semibold text-bone-50">
                    {value}
                  </p>
                </div>
              ))}
            </div>
            <ModpackCard instance={data} />
            {data.loader !== "vanilla" ? (
              <p className="mt-5 text-[13px] text-ash-300">
                {t("instance.mods.summary", { enabled: enabledMods, total: data.mods.length })}
              </p>
            ) : null}
            {data.managed ? (
              <p className="mt-3 max-w-[70ch] text-[13.5px] leading-relaxed text-ash-300">
                {t("instance.overview.managedNote")}
              </p>
            ) : null}
            {data.settings.notes ? (
              <div className="mt-6 max-w-[70ch]">
                <h3 className="font-display text-[15px] font-semibold text-bone-50">
                  {t("instance.overview.notes")}
                </h3>
                <p
                  className="mt-1.5 text-[13.5px] whitespace-pre-wrap text-ash-300"
                  data-selectable
                >
                  {data.settings.notes}
                </p>
              </div>
            ) : null}
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
