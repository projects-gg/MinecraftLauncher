import {
  AppWindow,
  ArrowsClockwise,
  ChatsCircle,
  Coffee,
  DownloadSimple,
  GameController,
  Gear,
  HardDrives,
  Info,
  Lifebuoy,
  Mountains,
  PaintBrush,
  SidebarSimple,
  UserCircle,
  UsersThree,
} from "@phosphor-icons/react";
import {
  Button,
  cn,
  easeOutExpo,
  Field,
  Input,
  SEGMENTED_GLIDE_MS,
  Segmented,
  Select,
  Slider,
  Switch,
  toast,
  useInteractionMotion,
  useMotionPrefs,
  useReduceMotion,
  useSystemReducedMotion,
} from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import type { TFunction } from "i18next";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useMemo } from "react";
import { useTranslation } from "react-i18next";
import logo from "@/assets/brand/logo-96.webp";
import { HelpSection } from "@/features/help/help-panel";
import { platformName } from "@/features/home/community-links";
import { EmojiRow, OverlayRow } from "@/features/overlay/settings-row";
import { FriendsSettingsSection } from "@/features/social/settings-section";
import { useSocialUi } from "@/features/social/store";
import { UpdateChannelRow } from "@/features/updates/channel-row";
import { UpdateSettingsAction } from "@/features/updates/update-settings";
import { api } from "@/lib/api";
import { applyStartOnBoot } from "@/lib/autostart";
import { formatMemory, formatPercent } from "@/lib/format";
import {
  automaticLanguage,
  LANGUAGE_NAMES,
  type LanguageChoice,
  loadEnglish,
  setLanguage,
} from "@/lib/i18n";
import { openLink } from "@/lib/links";
import { detectOs } from "@/lib/platform";
import { playSound } from "@/lib/sound";
import { type Place, usePlace, usePlacement } from "@/stores/placement";
import { type CloseBehavior, type GameStartBehavior, usePrefs } from "@/stores/prefs";
import { AccountsSection } from "./accounts-section";
import { DataSection } from "./data-section";
import { GraphicsSection } from "./graphics-section";
import { JavaSection } from "./java-section";
import { Row, SettingArrival, SwapLine, useSettingLanding } from "./row";

const sections = [
  { id: "general", icon: Gear },
  // "Grafik ve FPS": second, where players looking for FPS find it first (HOME1).
  { id: "graphics", icon: Mountains },
  { id: "appearance", icon: PaintBrush },
  { id: "game", icon: GameController },
  { id: "java", icon: Coffee },
  { id: "accounts", icon: UserCircle },
  // Only while the friends feature is on.
  { id: "friends", icon: UsersThree },
  { id: "downloads", icon: DownloadSimple },
  { id: "updates", icon: ArrowsClockwise },
  { id: "discord", icon: ChatsCircle },
  { id: "data", icon: HardDrives },
  // "Sorun yardımcısı" (HELP1): at the end, where help sits in most apps, before Hakkında.
  { id: "help", icon: Lifebuoy },
  { id: "about", icon: Info },
] as const;

type SectionId = (typeof sections)[number]["id"];

/**
 * A section's name: "Grafik ve FPS" and "Sorun yardımcısı" keep their words in their own modules
 * (locales/tr-fps.ts, tr-help.ts).
 */
function sectionTitle(t: TFunction, id: SectionId): string {
  if (id === "graphics") return t("fps.title");
  if (id === "help") return t("help.title");
  return t(`settings.sections.${id}`);
}

/**
 * "Otomatik" says what it gives on this computer (Turkish in Turkey or on a Turkish system,
 * English otherwise); the languages are named in their own language. The pick answers at once:
 * the plate glides to it, and as it arrives the window cross-fades to the new words
 * (lib/i18n.ts; under reduced motion the plate jumps and the fade starts right away). English
 * starts loading as soon as the pointer or the focus reaches the control.
 */
function LanguageRow() {
  const { t } = useTranslation();
  const choice = usePrefs((s) => s.language);
  const set = usePrefs((s) => s.set);
  const glide = useInteractionMotion();
  const automatic = useMemo(() => automaticLanguage(), []);
  const warm = () => void loadEnglish().catch(() => {});
  return (
    <Row title={t("settings.general.language")}>
      <Segmented<LanguageChoice>
        value={choice}
        onValueChange={(v) => {
          set("language", v);
          void setLanguage(v, { settle: glide ? SEGMENTED_GLIDE_MS : 0 });
        }}
        onIntent={warm}
        label={t("settings.general.language")}
        options={[
          {
            value: "auto",
            label: t("settings.general.languageAuto"),
            hint: `(${LANGUAGE_NAMES[automatic]})`,
            hintLang: automatic,
          },
          { value: "tr", label: LANGUAGE_NAMES.tr, lang: "tr" },
          { value: "en", label: LANGUAGE_NAMES.en, lang: "en" },
        ]}
      />
    </Row>
  );
}

/**
 * What the close button does (TRAY1): quit unless a game or AFK accounts keep the launcher (a job
 * the player started finishes first), or always stay in the tray, for friends' notifications. Next
 * to "Oyun açılınca launcher", worded the same way. macOS has no such row: its red button only
 * hides the window (shell::close_requested).
 */
function CloseBehaviorRow() {
  const { t } = useTranslation();
  const choice = usePrefs((s) => s.closeBehavior);
  const set = usePrefs((s) => s.set);
  const mac = useMemo(() => detectOs() === "macos", []);
  if (mac) return null;
  return (
    <Row title={t("tray.closeSetting")}>
      <div className="w-64">
        <Select<CloseBehavior>
          value={choice}
          onValueChange={(v) => set("closeBehavior", v)}
          label={t("tray.closeSetting")}
          options={[
            { value: "quit", label: t("tray.closeQuit") },
            { value: "tray", label: t("tray.closeTray") },
          ]}
        />
      </div>
    </Row>
  );
}

function General() {
  const { t } = useTranslation();
  const p = usePrefs();
  return (
    <>
      <LanguageRow />
      {/* "Kapansın" really quits (SMALL7): its line says what that gives up; the others keep the
          AFK rule. Both share one cell, so nothing moves when the choice changes. */}
      <Row
        title={t("settings.general.onGameStart")}
        help={
          <SwapLine
            lines={[t("settings.general.onGameStartHelp"), t("small7.gameStart.closeHelp")]}
            active={p.onGameStart === "close" ? 1 : 0}
          />
        }
      >
        {/* Wide enough for "Simge durumuna küçülsün" on one line. */}
        <div className="w-64">
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
      <CloseBehaviorRow />
      {/* Next to the two rows about the launcher during a game: "Kapansın" above is what stops it. */}
      <OverlayRow />
      {/* The same mod's emoji picker in the game's chat (CHAT-EMOJI), right under its switch. */}
      <EmojiRow />
      <Row title={t("shotCopy.setting")} help={t("shotCopy.settingHelp")}>
        <Switch
          checked={p.copyScreenshots}
          onCheckedChange={(v) => {
            p.set("copyScreenshots", v);
            // A game that runs now follows the switch at once; the next launch brings it anyway.
            void api.setScreenshotCopy(v).catch(() => {});
          }}
          label={t("shotCopy.setting")}
          hideLabel
        />
      </Row>
      <Row title={t("settings.general.startOnBoot")}>
        <Switch
          checked={p.startOnBoot}
          onCheckedChange={(v) => {
            p.set("startOnBoot", v);
            applyStartOnBoot(v).catch(() => {
              p.set("startOnBoot", !v);
              toast.error(t("settings.general.startOnBootFailed"));
            });
          }}
          label={t("settings.general.startOnBoot")}
          hideLabel
        />
      </Row>
      <Row title={t("settings.general.notifications")}>
        <Switch
          checked={p.desktopNotifications}
          onCheckedChange={(v) => p.set("desktopNotifications", v)}
          label={t("settings.general.notifications")}
          hideLabel
        />
      </Row>
    </>
  );
}

/** Lets the player hear the new volume while dragging, at most a few times a second. */
let lastPreview = 0;
function previewVolume() {
  const now = Date.now();
  if (now - lastPreview < 180) return;
  lastPreview = now;
  playSound("success");
}

/**
 * Where the friends button sits: the title bar (its home) or the side menu. The same choice as
 * the button's right-click menu; the button moves while the plate glides.
 */
function FriendsButtonRow() {
  const { t } = useTranslation();
  const place = usePlace("friends");
  const move = usePlacement((s) => s.move);
  return (
    <Row title={t("placement.friendsButton")}>
      <Segmented<Place>
        value={place}
        onValueChange={(v) => move("friends", v)}
        label={t("placement.friendsButton")}
        // The same marks as the button's right-click menu.
        options={[
          {
            value: "titlebar",
            label: t("placement.titleBar"),
            icon: <AppWindow size={15} aria-hidden />,
          },
          {
            value: "rail",
            label: t("placement.rail"),
            icon: <SidebarSimple size={15} aria-hidden />,
          },
        ]}
      />
    </Row>
  );
}

/** The system's name where its settings are meant ("Windows ayarlarında açık."). */
const SYSTEM_NAMES = { windows: "Windows", macos: "macOS", linux: "Linux" } as const;

/**
 * "Animasyonları azalt" (BETA-FB): fades only, as the system's own reduced motion gives (@pl/ui
 * reduced-motion). Performance mode and the system's setting hold it on, and the line under it
 * says which (a switch held on without a reason read as broken; TASTE-SWEEP); otherwise the line
 * says what it does, apart from Performance mode above it. One cell, so nothing below moves.
 */
function ReduceMotionRow() {
  const { t } = useTranslation();
  const p = usePrefs();
  const system = useSystemReducedMotion();
  const os = useMemo(() => detectOs(), []);
  const kept = system || p.performanceMode;
  return (
    <Row
      title={t("motion.title")}
      help={
        <SwapLine
          lines={[
            t("motion.help"),
            t("motion.perfOn"),
            t("motion.systemOn", { system: SYSTEM_NAMES[os] }),
          ]}
          active={system ? 2 : p.performanceMode ? 1 : 0}
        />
      }
      setting="reduce-motion"
    >
      <Switch
        checked={p.reduceMotion || kept}
        onCheckedChange={(v) => p.set("reduceMotion", v)}
        label={t("motion.title")}
        hideLabel
        disabled={kept}
      />
    </Row>
  );
}

/**
 * The embers on Home are ambient motion: Performance mode and reduced motion (the system's, or
 * "Animasyonları azalt") stop them, so the switch shows them off and fixed then, with the reason
 * under it (it showed them on in gold while Home had none; TASTE-SWEEP).
 */
function ParticlesRow() {
  const { t } = useTranslation();
  const p = usePrefs();
  const reduced = useReduceMotion();
  const held = p.performanceMode || reduced;
  return (
    <Row
      title={t("settings.appearance.particles")}
      help={
        p.performanceMode ? t("motion.embersPerf") : reduced ? t("motion.embersReduced") : undefined
      }
    >
      <Switch
        checked={p.particles && !held}
        onCheckedChange={(v) => p.set("particles", v)}
        label={t("settings.appearance.particles")}
        hideLabel
        disabled={held}
      />
    </Row>
  );
}

function Appearance() {
  const { t } = useTranslation();
  const p = usePrefs();
  const friendsOn = useSocialUi((s) => s.view.enabled);
  return (
    <>
      <Row
        title={t("settings.appearance.performance")}
        help={t("settings.appearance.performanceHelp")}
        setting="performance"
      >
        <Switch
          checked={p.performanceMode}
          onCheckedChange={p.setPerformanceMode}
          label={t("settings.appearance.performance")}
          hideLabel
        />
      </Row>
      <ReduceMotionRow />
      <ParticlesRow />
      <Row title={t("settings.appearance.sounds")}>
        <Switch
          checked={p.sounds}
          onCheckedChange={(v) => p.set("sounds", v)}
          label={t("settings.appearance.sounds")}
          hideLabel
        />
      </Row>
      {/* Always there (dimmed while sounds are off), so nothing below jumps. */}
      <div className={cn("py-4", friendsOn && "border-b border-white/6")}>
        <Slider
          label={t("settings.appearance.volume")}
          value={p.soundVolume}
          onValueChange={(v) => {
            p.set("soundVolume", v);
            previewVolume();
          }}
          min={0}
          max={100}
          step={5}
          format={formatPercent}
          disabled={!p.sounds}
        />
      </div>
      {friendsOn ? <FriendsButtonRow /> : null}
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
        <Switch
          checked={p.verifyOnLaunch}
          onCheckedChange={(v) => p.set("verifyOnLaunch", v)}
          label={t("settings.downloads.verify")}
          hideLabel
        />
      </Row>
      <Row title={t("prepare.setting")} help={t("prepare.settingHelp")}>
        <Switch
          checked={p.backgroundPrepare}
          onCheckedChange={(v) => p.set("backgroundPrepare", v)}
          label={t("prepare.setting")}
          hideLabel
        />
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
      {/* BETA-GATE: Beta asks first; a no says why under the title. */}
      <UpdateChannelRow />
      <Row title={t("settings.updates.autoDownload")}>
        <Switch
          checked={p.autoDownloadUpdates}
          onCheckedChange={(v) => p.set("autoDownloadUpdates", v)}
          label={t("settings.updates.autoDownload")}
          hideLabel
        />
      </Row>
      <Row
        title={
          info
            ? t("settings.updates.current", { version: info.version })
            : t("settings.about.version")
        }
      >
        <UpdateSettingsAction />
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
        <Switch
          checked={p.discordEnabled}
          onCheckedChange={(v) => p.set("discordEnabled", v)}
          label={t("settings.discord.enabled")}
          hideLabel
        />
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
            {/* What Discord shows while playing, with "Bağlandığım sunucuyu göster" as it is. */}
            <div className="text-[13px] leading-snug">
              <p className="font-semibold text-white">Projects Launcher</p>
              <p className="text-[#dbdee1]">
                {p.discordShowServer
                  ? t("settings.discord.previewDetails")
                  : t("settings.discord.previewDetailsPlain")}
              </p>
              <p className="text-[#b5bac1] tabular-nums">
                {p.discordShowServer
                  ? t("settings.discord.previewStateParty")
                  : t("settings.discord.previewState")}
              </p>
              <p className="text-[#b5bac1] tabular-nums">{t("settings.discord.previewElapsed")}</p>
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
}

/** Data folder, storage and cleanup, support bundle, moving the folder, v1 import. */
function Data() {
  return <DataSection />;
}

/** "Windows, 64 bit" / "Windows, 64-bit" instead of "windows x86_64". */
function platformLabel(t: TFunction, os: string, arch: string): string {
  const system =
    { windows: "Windows", macos: "macOS", linux: "Linux" }[os] ??
    os.charAt(0).toLocaleUpperCase("en") + os.slice(1);
  const bits = {
    x86_64: t("settings.about.bits", { bits: 64 }),
    aarch64: "ARM",
    x86: t("settings.about.bits", { bits: 32 }),
  }[arch];
  return bits ? `${system}, ${bits}` : system;
}

function About() {
  const { t } = useTranslation();
  const { data: info } = useQuery({ queryKey: ["app-info"], queryFn: () => api.appInfo() });
  // Studio's links (from the home content, cached with it); the built-in addresses stand in.
  const { data: home } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  const links = home?.links;
  const linkButtons = [
    { label: t("settings.about.website"), url: links?.website ?? "https://projects.gg" },
    { label: t("settings.about.discord"), url: links?.discord ?? "https://projects.gg/discord" },
    ...(links?.store ? [{ label: t("settings.about.store"), url: links.store }] : []),
    ...(links?.support ? [{ label: t("settings.about.support"), url: links.support }] : []),
    // Wiki, forum and the social accounts, as Home's community row shows them.
    ...(links?.community ?? [])
      .filter((link) => link.platform !== "discord")
      .map((link) => ({ label: platformName(link.platform), url: link.url })),
    ...(links?.custom ?? []),
  ];
  return (
    <div>
      <div className="flex items-center gap-4 pb-5">
        <img src={logo} alt="" className="size-16" />
        <div>
          <p className="font-display text-[22px] font-semibold text-bone-50">Projects Launcher</p>
          <p className="flex gap-3 text-ash-300 tabular-nums" data-selectable>
            <span>
              {t("settings.about.version")} {info?.version}
            </span>
            {info ? <span>{platformLabel(t, info.os, info.arch)}</span> : null}
          </p>
        </div>
      </div>
      <div className="flex flex-wrap gap-2.5">
        {linkButtons.map((link) => (
          <Button
            key={`${link.label} ${link.url}`}
            variant="secondary"
            onClick={() => openLink(link.url)}
          >
            {link.label}
          </Button>
        ))}
        <Button
          variant="ghost"
          onClick={() => void api.openUrl("https://github.com/projects-gg/MinecraftLauncher")}
        >
          {t("settings.about.source")}
        </Button>
      </div>
      <p className="mt-6 text-[12.5px] text-ash-400">{t("settings.about.credits")}</p>
      {/* Mojang's usage guidelines ask for it; the owner wants it quiet (18 Oct 2026). */}
      <p className="mt-1 text-[12.5px] text-ash-400">{t("minecraft.notAffiliated")}</p>
    </div>
  );
}

/** Defaults applied to instances created from now on. */
function GameDefaults() {
  const { t } = useTranslation();
  const p = usePrefs();
  const { data: info } = useQuery({ queryKey: ["app-info"], queryFn: () => api.appInfo() });
  const total = info?.systemMemoryMb ?? 8192;
  // Leave at least 1 GB to the system, like the per-instance limits.
  const ceiling = Math.max(2048, Math.floor((total - 1024) / 512) * 512);
  const auto = p.defaultMaxMemoryMb === null;
  // The slider fades in place when "Otomatik" goes off; the rows below glide (not in Performance
  // mode or under reduced motion, where they simply move).
  const glides = useMotionPrefs().layout;
  return (
    <>
      {/* The same control as an instance's memory, so "Otomatik" reads the same everywhere. */}
      <Row
        title={t("settings.game.memory")}
        help={auto ? t("settings.game.memoryAutoHelp") : t("settings.game.memoryHelp")}
      >
        <Segmented
          value={auto ? "auto" : "custom"}
          onValueChange={(v) =>
            p.set("defaultMaxMemoryMb", v === "auto" ? null : Math.min(4096, ceiling))
          }
          label={t("settings.game.memory")}
          options={[
            { value: "auto", label: t("instance.settings.memoryAuto") },
            { value: "custom", label: t("instance.settings.memoryCustom") },
          ]}
        />
      </Row>
      <AnimatePresence initial={false}>
        {auto ? null : (
          <motion.div
            key="max-memory"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: { duration: 0.2 } }}
            exit={{ opacity: 0, transition: { duration: 0.12 } }}
            className="border-b border-white/6 py-4"
          >
            <Slider
              label={t("settings.game.maxMemory")}
              value={Math.min(p.defaultMaxMemoryMb ?? 4096, ceiling)}
              onValueChange={(v) => p.set("defaultMaxMemoryMb", v)}
              min={1024}
              max={ceiling}
              step={512}
              format={formatMemory}
            />
          </motion.div>
        )}
      </AnimatePresence>
      <motion.div
        layout={glides ? "position" : false}
        transition={{ layout: { duration: 0.3, ease: easeOutExpo } }}
      >
        <Row title={t("settings.game.showSnapshots")}>
          <Switch
            checked={p.showSnapshots}
            onCheckedChange={(v) => p.set("showSnapshots", v)}
            label={t("settings.game.showSnapshots")}
            hideLabel
          />
        </Row>
        <div className="py-4">
          <Field
            label={t("settings.game.jvmArgs")}
            help={t("settings.game.jvmArgsHelp")}
            htmlFor="default-jvm-args"
          >
            <Input
              id="default-jvm-args"
              value={p.defaultJvmArgs}
              onChange={(e) => p.set("defaultJvmArgs", e.target.value)}
              spellCheck={false}
              autoComplete="off"
            />
          </Field>
        </div>
      </motion.div>
    </>
  );
}

export function SettingsPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const search = useSearch({ strict: false }) as { section?: string; focus?: string };
  const friendsOn = useSocialUi((s) => s.view.enabled);
  const shown = sections.filter((s) => s.id !== "friends" || friendsOn);
  const active = (shown.find((s) => s.id === search.section)?.id ?? "general") as SectionId;
  const glide = useMotionPrefs().layout;
  // A row asked for by name (the command palette's settings rows): the page lands on it.
  const arrival = useSettingLanding(search.focus, () => {
    void navigate({ to: "/settings", search: { section: active }, replace: true });
  });

  const body: Record<SectionId, ReactNode> = {
    general: <General />,
    graphics: <GraphicsSection />,
    appearance: <Appearance />,
    game: <GameDefaults />,
    java: <JavaSection />,
    accounts: <AccountsSection />,
    friends: <FriendsSettingsSection />,
    downloads: <Downloads />,
    updates: <Updates />,
    discord: <Discord />,
    data: <Data />,
    help: <HelpSection />,
    about: <About />,
  };

  return (
    <div className="flex min-h-0 flex-1 gap-6 px-page pt-3 pb-5">
      <nav aria-label={t("settings.title")} className="w-[184px] shrink-0">
        <h1 className="page-title mb-4">{t("settings.title")}</h1>
        <ul className="space-y-0.5">
          {shown.map((s) => {
            const Icon = s.icon;
            const on = s.id === active;
            return (
              <li key={s.id}>
                <button
                  type="button"
                  aria-current={on ? "page" : undefined}
                  onClick={() =>
                    void navigate({ to: "/settings", search: { section: s.id }, replace: true })
                  }
                  className={cn(
                    "relative isolate flex w-full cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-[7px] text-left text-[13px] font-medium transition-[color,background-color,transform,scale] duration-150 ease-out active:scale-[0.98]",
                    on ? "text-bone-50" : "text-ash-300 hover:bg-white/5 hover:text-bone-50",
                  )}
                >
                  {/* The highlight glides to the chosen section (in place in Performance mode). */}
                  {on ? (
                    <motion.span
                      aria-hidden
                      layoutId={glide ? "settings-section" : undefined}
                      className="absolute inset-0 -z-10 rounded-md bg-white/8"
                      transition={{ type: "spring", stiffness: 480, damping: 40 }}
                    />
                  ) : null}
                  <Icon
                    size={17}
                    weight={on ? "duotone" : "regular"}
                    className={on ? "text-gold-300" : undefined}
                  />
                  {sectionTitle(t, s.id)}
                </button>
              </li>
            );
          })}
        </ul>
      </nav>
      <section className="glass min-h-0 max-w-[740px] flex-1 overflow-y-auto rounded-[var(--radius-panel)] px-6 py-4">
        {/* A new section rises into place; the panel itself stays. */}
        <div key={active} className="panel-in">
          <h2 className="mb-1 font-display text-section font-semibold text-bone-50">
            {sectionTitle(t, active)}
          </h2>
          <SettingArrival.Provider value={arrival}>{body[active]}</SettingArrival.Provider>
        </div>
      </section>
    </div>
  );
}
