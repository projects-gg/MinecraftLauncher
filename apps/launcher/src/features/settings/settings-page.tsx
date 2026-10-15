import {
  ArrowsClockwise,
  ChatsCircle,
  DownloadSimple,
  FolderOpen,
  GameController,
  Gear,
  HardDrives,
  Info,
  Lifebuoy,
  PaintBrush,
  Trash,
  UserCircle,
} from "@phosphor-icons/react";
import { Button, cn, Segmented, Select, Slider, Switch, toast } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import logo from "@/assets/brand/logo-96.webp";
import { api } from "@/lib/api";
import { setLanguage } from "@/lib/i18n";
import { type GameStartBehavior, usePrefs } from "@/stores/prefs";
import { AccountsSection } from "./accounts-section";

const sections = [
  { id: "general", icon: Gear },
  { id: "appearance", icon: PaintBrush },
  { id: "game", icon: GameController },
  { id: "accounts", icon: UserCircle },
  { id: "downloads", icon: DownloadSimple },
  { id: "updates", icon: ArrowsClockwise },
  { id: "discord", icon: ChatsCircle },
  { id: "data", icon: HardDrives },
  { id: "about", icon: Info },
] as const;

type SectionId = (typeof sections)[number]["id"];

export function Row({ title, help, children }: { title: string; help?: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-8 border-b border-white/6 py-4 last:border-b-0">
      <div className="min-w-0">
        <p className="text-[14px] font-medium text-bone-50">{title}</p>
        {help ? <p className="mt-0.5 max-w-[56ch] text-[12.5px] text-ash-400">{help}</p> : null}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function General() {
  const { t } = useTranslation();
  const p = usePrefs();
  return (
    <>
      <Row title={t("settings.general.language")}>
        <Segmented
          value={p.language}
          onValueChange={(v) => {
            p.set("language", v);
            setLanguage(v);
          }}
          label={t("settings.general.language")}
          options={[
            { value: "tr", label: "Türkçe" },
            { value: "en", label: "English" },
          ]}
        />
      </Row>
      <Row title={t("settings.general.onGameStart")} help={t("settings.general.onGameStartHelp")}>
        <div className="w-56">
          <Select<GameStartBehavior>
            value={p.onGameStart}
            onValueChange={(v) => p.set("onGameStart", v)}
            label={t("settings.general.onGameStart")}
            options={[
              { value: "keep", label: t("settings.general.keep") },
              { value: "minimize", label: t("settings.general.minimize") },
              { value: "tray", label: t("settings.general.tray") },
              { value: "close", label: t("settings.general.close") },
            ]}
          />
        </div>
      </Row>
      <Row title={t("settings.general.startOnBoot")} help={t("settings.general.startOnBootHelp")}>
        <Switch checked={p.startOnBoot} onCheckedChange={(v) => p.set("startOnBoot", v)} label={t("settings.general.startOnBoot")} hideLabel />
      </Row>
      <Row title={t("settings.general.closeToTray")}>
        <Switch checked={p.closeToTray} onCheckedChange={(v) => p.set("closeToTray", v)} label={t("settings.general.closeToTray")} hideLabel />
      </Row>
    </>
  );
}

function Appearance() {
  const { t } = useTranslation();
  const p = usePrefs();
  return (
    <>
      <Row title={t("settings.appearance.performance")} help={t("settings.appearance.performanceHelp")}>
        <Switch checked={p.performanceMode} onCheckedChange={p.setPerformanceMode} label={t("settings.appearance.performance")} hideLabel />
      </Row>
      <Row title={t("settings.appearance.particles")}>
        <Switch checked={p.particles} onCheckedChange={(v) => p.set("particles", v)} label={t("settings.appearance.particles")} hideLabel disabled={p.performanceMode} />
      </Row>
      <Row title={t("settings.appearance.sounds")}>
        <Switch checked={p.sounds} onCheckedChange={(v) => p.set("sounds", v)} label={t("settings.appearance.sounds")} hideLabel />
      </Row>
      {p.sounds ? (
        <div className="py-4">
          <Slider
            label={t("settings.appearance.volume")}
            value={p.soundVolume}
            onValueChange={(v) => p.set("soundVolume", v)}
            min={0}
            max={100}
            step={5}
            format={(v) => `%${v}`}
          />
        </div>
      ) : null}
    </>
  );
}

function Downloads() {
  const { t } = useTranslation();
  const p = usePrefs();
  return (
    <>
      <div className="border-b border-white/6 py-4">
        <Slider
          label={t("settings.downloads.concurrent")}
          value={p.concurrentDownloads}
          onValueChange={(v) => p.set("concurrentDownloads", v)}
          min={4}
          max={48}
          step={4}
        />
        <p className="mt-2 text-[12.5px] text-ash-400">{t("settings.downloads.concurrentHelp")}</p>
      </div>
      <Row title={t("settings.downloads.verify")} help={t("settings.downloads.verifyHelp")}>
        <Switch checked={p.verifyOnLaunch} onCheckedChange={(v) => p.set("verifyOnLaunch", v)} label={t("settings.downloads.verify")} hideLabel />
      </Row>
    </>
  );
}

function Updates() {
  const { t } = useTranslation();
  const p = usePrefs();
  const { data: info } = useQuery({ queryKey: ["app-info"], queryFn: () => api.appInfo() });
  return (
    <>
      <Row title={t("settings.updates.channel")} help={p.updateChannel === "beta" ? t("settings.updates.betaHelp") : undefined}>
        <Segmented
          value={p.updateChannel}
          onValueChange={(v) => p.set("updateChannel", v)}
          label={t("settings.updates.channel")}
          options={[
            { value: "stable", label: t("settings.updates.stable") },
            { value: "beta", label: t("settings.updates.beta") },
          ]}
        />
      </Row>
      <Row title={t("settings.updates.autoDownload")}>
        <Switch checked={p.autoDownloadUpdates} onCheckedChange={(v) => p.set("autoDownloadUpdates", v)} label={t("settings.updates.autoDownload")} hideLabel />
      </Row>
      <Row title={t("settings.updates.current", { version: info?.version ?? "" })}>
        <Button variant="secondary" icon={<ArrowsClockwise size={16} />} onClick={() => toast.success(t("settings.updates.upToDate"))}>
          {t("settings.updates.check")}
        </Button>
      </Row>
    </>
  );
}

function Discord() {
  const { t } = useTranslation();
  const p = usePrefs();
  return (
    <>
      <Row title={t("settings.discord.enabled")}>
        <Switch checked={p.discordEnabled} onCheckedChange={(v) => p.set("discordEnabled", v)} label={t("settings.discord.enabled")} hideLabel />
      </Row>
      <Row title={t("settings.discord.showServer")}>
        <Switch
          checked={p.discordShowServer}
          onCheckedChange={(v) => p.set("discordShowServer", v)}
          label={t("settings.discord.showServer")}
          hideLabel
          disabled={!p.discordEnabled}
        />
      </Row>
      {p.discordEnabled ? (
        <div className="mt-5">
          <p className="mb-2 text-[12px] text-ash-400">{t("settings.discord.preview")}</p>
          <div className="flex w-fit items-center gap-3 rounded-lg bg-[#232428] p-3 pr-6 ring-1 ring-white/5">
            <div className="relative">
              <img src={logo} alt="" className="size-16 rounded-lg bg-obsidian-900 p-1" />
              <span className="absolute -right-1 -bottom-1 flex size-6 items-center justify-center rounded-md bg-gold-700 text-[11px] font-bold text-gold-200 ring-2 ring-[#232428]">
                E
              </span>
            </div>
            <div className="text-[13px] leading-snug">
              <p className="font-semibold text-white">Projects Launcher</p>
              <p className="text-[#dbdee1]">{t("settings.discord.previewDetails")}</p>
              <p className="text-[#b5bac1]">{t("settings.discord.previewState")}</p>
              <p className="text-[#b5bac1] tabular-nums">00:42 geçti</p>
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
}

function Data() {
  const { t } = useTranslation();
  return (
    <>
      <Row title={t("settings.data.folder")} help={t("settings.data.folderHelp")}>
        <div className="flex gap-2">
          <Button variant="ghost" size="sm" icon={<FolderOpen size={15} />}>
            {t("settings.data.open")}
          </Button>
          <Button variant="secondary" size="sm">
            {t("settings.data.move")}
          </Button>
        </div>
      </Row>
      <Row title={t("settings.data.cache")} help={t("settings.data.cacheHelp")}>
        <Button variant="secondary" size="sm" icon={<Trash size={15} />}>
          {t("settings.data.clear")}
        </Button>
      </Row>
      <Row title={t("settings.data.legacy")} help={t("settings.data.legacyHelp")}>
        <Button variant="secondary" size="sm">
          {t("settings.data.import")}
        </Button>
      </Row>
      <Row title={t("settings.data.support")} help={t("settings.data.supportHelp")}>
        <Button variant="secondary" size="sm" icon={<Lifebuoy size={15} />}>
          {t("settings.data.support")}
        </Button>
      </Row>
    </>
  );
}

function About() {
  const { t } = useTranslation();
  const { data: info } = useQuery({ queryKey: ["app-info"], queryFn: () => api.appInfo() });
  return (
    <div>
      <div className="flex items-center gap-4 pb-5">
        <img src={logo} alt="" className="size-16" />
        <div>
          <p className="font-display text-[22px] font-semibold text-bone-50">Projects Launcher</p>
          <p className="text-ash-400">
            {t("settings.about.version")} {info?.version} ({info?.os} {info?.arch})
          </p>
        </div>
      </div>
      <div className="flex flex-wrap gap-2.5">
        <Button variant="secondary" onClick={() => void api.openUrl("https://projects.gg")}>
          {t("settings.about.website")}
        </Button>
        <Button variant="secondary" onClick={() => void api.openUrl("https://projects.gg/discord")}>
          {t("settings.about.discord")}
        </Button>
        <Button variant="ghost" onClick={() => void api.openUrl("https://github.com/projects-gg/MinecraftLauncher")}>
          {t("settings.about.source")}
        </Button>
      </div>
      <p className="mt-6 text-[12.5px] text-ash-400">{t("settings.about.credits")}</p>
    </div>
  );
}

function GameDefaults() {
  const { t } = useTranslation();
  return (
    <div className="py-2">
      <p className="text-[12.5px] text-ash-400">{t("settings.game.memoryHelp")}</p>
    </div>
  );
}

export function SettingsPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const search = useSearch({ strict: false }) as { section?: string };
  const active = (sections.find((s) => s.id === search.section)?.id ?? "general") as SectionId;

  const body: Record<SectionId, ReactNode> = {
    general: <General />,
    appearance: <Appearance />,
    game: <GameDefaults />,
    accounts: <AccountsSection />,
    downloads: <Downloads />,
    updates: <Updates />,
    discord: <Discord />,
    data: <Data />,
    about: <About />,
  };

  return (
    <div className="flex min-h-0 flex-1 gap-8 px-10 pt-4 pb-6">
      <nav aria-label={t("settings.title")} className="w-56 shrink-0">
        <h1 className="mb-5 font-display text-[32px] leading-tight font-semibold text-bone-50">{t("settings.title")}</h1>
        <ul className="space-y-0.5">
          {sections.map((s) => {
            const Icon = s.icon;
            const on = s.id === active;
            return (
              <li key={s.id}>
                <button
                  type="button"
                  aria-current={on ? "page" : undefined}
                  onClick={() => void navigate({ to: "/settings", search: { section: s.id }, replace: true })}
                  className={cn(
                    "flex w-full cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-left text-[13.5px] font-medium transition-colors",
                    on ? "bg-white/8 text-bone-50" : "text-ash-300 hover:bg-white/5 hover:text-bone-50",
                  )}
                >
                  <Icon size={18} weight={on ? "duotone" : "regular"} className={on ? "text-gold-300" : undefined} />
                  {t(`settings.sections.${s.id}`)}
                </button>
              </li>
            );
          })}
        </ul>
      </nav>
      <section className="glass min-h-0 max-w-[760px] flex-1 overflow-y-auto rounded-[var(--radius-panel)] px-7 py-5">
        <h2 className="mb-2 font-display text-[20px] font-semibold text-bone-50">{t(`settings.sections.${active}`)}</h2>
        {body[active]}
      </section>
    </div>
  );
}
