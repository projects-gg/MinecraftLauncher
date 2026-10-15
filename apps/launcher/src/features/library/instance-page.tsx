import { ArrowLeft, FolderOpen, Play } from "@phosphor-icons/react";
import { Button, Chip, Skeleton, TabPanel, Tabs } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { formatDuration, formatRelative } from "@/lib/format";
import { InstanceIcon } from "@/shell/instance-icon";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { ModsTab } from "./tabs/mods-tab";
import { PacksTab } from "./tabs/packs-tab";
import { ScreenshotsTab } from "./tabs/screenshots-tab";
import { ServersTab } from "./tabs/servers-tab";
import { SettingsTab } from "./tabs/settings-tab";
import { WorldsTab } from "./tabs/worlds-tab";

export type InstanceTab =
  | "overview"
  | "mods"
  | "resourcePacks"
  | "shaders"
  | "worlds"
  | "screenshots"
  | "servers"
  | "settings";

export function InstancePage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { instanceId } = useParams({ from: "/library/$instanceId" });
  const search = useSearch({ from: "/library/$instanceId" });
  const tab = (search.tab ?? "overview") as InstanceTab;
  const { data, isPending } = useQuery({ queryKey: ["instance", instanceId], queryFn: () => api.instance(instanceId) });
  const selectInstance = usePrefs((s) => s.selectInstance);
  const play = useLaunch((s) => s.play);

  const setTab = (next: InstanceTab) =>
    void navigate({ to: "/library/$instanceId", params: { instanceId }, search: { tab: next }, replace: true });

  if (isPending || !data) {
    return (
      <div className="px-10 pt-6">
        <Skeleton className="h-20 w-96" />
        <Skeleton className="mt-6 h-10 w-full" />
        <Skeleton className="mt-4 h-64 w-full" />
      </div>
    );
  }

  const loader = data.loader === "vanilla" ? "Minecraft" : t(`loaders.${data.loader}`);
  const enabledMods = data.mods.filter((m) => m.enabled).length;

  return (
    <div className="flex min-h-0 flex-1 flex-col px-10 pt-3">
      <Link
        to="/library"
        className="mb-3 flex w-fit items-center gap-1.5 rounded-md px-1.5 py-1 text-[13px] text-ash-400 transition-colors hover:text-bone-50"
      >
        <ArrowLeft size={14} /> {t("instance.back")}
      </Link>
      <header className="flex items-center gap-5">
        <InstanceIcon icon={data.icon} size={76} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2.5">
            <h1 className="truncate font-display text-[30px] leading-tight font-semibold text-bone-50">{data.name}</h1>
            {data.managed ? <Chip tone="gold">{t("instances.managed")}</Chip> : null}
          </div>
          <p className="mt-1 flex gap-4 text-[13px] text-ash-400">
            <span>
              {loader} {data.mcVersion}
              {data.loaderVersion ? ` (${data.loaderVersion})` : ""}
            </span>
            {data.playTimeSecs > 0 ? <span>{formatDuration(data.playTimeSecs)}</span> : null}
            {data.lastPlayedAt ? <span>{formatRelative(data.lastPlayedAt)}</span> : null}
          </p>
        </div>
        <div className="flex gap-2.5">
          <Button variant="ghost" icon={<FolderOpen size={17} />} onClick={() => void api.openInstanceFolder(data.id)}>
            {t("instance.openFolder")}
          </Button>
          <Button
            variant="primary"
            icon={<Play size={15} weight="fill" />}
            onClick={() => {
              selectInstance(data.id);
              void play(data.id);
            }}
          >
            {t("instance.play")}
          </Button>
        </div>
      </header>

      <Tabs
        value={tab}
        onValueChange={setTab}
        label={data.name}
        className="mt-6"
        items={[
          { value: "overview", label: t("instance.tabs.overview") },
          { value: "mods", label: t("instance.tabs.mods"), count: data.loader === "vanilla" ? undefined : data.mods.length },
          { value: "resourcePacks", label: t("instance.tabs.resourcePacks"), count: data.resourcePacks.length },
          { value: "shaders", label: t("instance.tabs.shaders"), count: data.shaderPacks.length },
          { value: "worlds", label: t("instance.tabs.worlds"), count: data.worlds.length },
          { value: "screenshots", label: t("instance.tabs.screenshots") },
          { value: "servers", label: t("instance.tabs.servers") },
          { value: "settings", label: t("instance.tabs.settings") },
        ]}
      >
        <TabPanel value="overview" className="pb-8">
          <div className="grid max-w-[880px] grid-cols-4 gap-3">
            {[
              [t("instance.overview.playTime"), data.playTimeSecs > 0 ? formatDuration(data.playTimeSecs) : "0 dk"],
              [t("instance.overview.lastPlayed"), data.lastPlayedAt ? formatRelative(data.lastPlayedAt) : t("library.card.never")],
              [t("instance.overview.version"), data.mcVersion],
              [
                t("instance.overview.loader"),
                data.loader === "vanilla" ? t("loaders.vanilla") : `${loader} ${data.loaderVersion ?? ""}`,
              ],
            ].map(([label, value]) => (
              <div key={label} className="glass rounded-[var(--radius-panel)] px-4 py-3.5">
                <p className="text-[12px] text-ash-400">{label}</p>
                <p className="mt-1 truncate font-display text-[18px] font-semibold text-bone-50">{value}</p>
              </div>
            ))}
          </div>
          {data.loader !== "vanilla" ? (
            <p className="mt-5 text-[13px] text-ash-300">
              {t("instance.mods.summary", { enabled: enabledMods, total: data.mods.length })}
            </p>
          ) : null}
          {data.managed ? (
            <p className="mt-3 max-w-[70ch] text-[13.5px] leading-relaxed text-ash-300">{t("instance.overview.managedNote")}</p>
          ) : null}
          {data.settings.notes ? (
            <div className="mt-6 max-w-[70ch]">
              <h3 className="font-display text-[15px] font-semibold text-bone-50">{t("instance.overview.notes")}</h3>
              <p className="mt-1.5 text-[13.5px] whitespace-pre-wrap text-ash-300" data-selectable>
                {data.settings.notes}
              </p>
            </div>
          ) : null}
        </TabPanel>
        <TabPanel value="mods" className="pb-8">
          <ModsTab instance={data} />
        </TabPanel>
        <TabPanel value="resourcePacks" className="pb-8">
          <PacksTab packs={data.resourcePacks} emptyText={t("instance.packs.emptyResource")} />
        </TabPanel>
        <TabPanel value="shaders" className="pb-8">
          <PacksTab
            packs={data.shaderPacks}
            emptyText={data.loader === "vanilla" ? t("instance.packs.shaderNeedsIris") : t("instance.packs.emptyShader")}
          />
        </TabPanel>
        <TabPanel value="worlds" className="pb-8">
          <WorldsTab worlds={data.worlds} />
        </TabPanel>
        <TabPanel value="screenshots" className="pb-8">
          <ScreenshotsTab shots={data.screenshots} />
        </TabPanel>
        <TabPanel value="servers" className="pb-8">
          <ServersTab servers={data.servers} />
        </TabPanel>
        <TabPanel value="settings" className="pb-8">
          <SettingsTab instance={data} />
        </TabPanel>
      </Tabs>
    </div>
  );
}
