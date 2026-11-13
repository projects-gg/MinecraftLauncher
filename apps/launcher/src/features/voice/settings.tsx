// Ayarlar → Sesli sohbet (VOICE-L3; voice.md §5.2): the one switch first ("Kapalıyken kimse seni
// arayamaz."), then what most players touch: the microphone and its test (a live level bar while
// it runs: the player hears themselves, nothing goes anywhere), the output, when the microphone is
// on ("Konuşunca" / "Bas-konuş" with its key), noise suppression, the join sounds and the two
// shortcuts (with Discord's keys offered once at the top, VOICE-KEYS); the rest one level down
// under "Gelişmiş". Everything under the switch fades away while it
// is off. A change is saved at once (a slider after its drag settles); the launcher's answer is
// what stays (it holds values to their ranges and a key to one shortcut). The voice engine runs
// for the devices and the level bar only while this page is open.
//
// VOICE-CALL: "Ses seviyesi" under the output (everyone at once, 0–200 %, to the engine's top),
// "Sesli sohbet sesleri" in the place of "Katılma ve ayrılma sesleri" (the same choice), "Aramadan
// önce sor", and the microphone's meter as the call's (meter.tsx): in the test row, and as the
// sensitivity itself, whose line sits on the meter (bars past it go out: Discord's and Steam's way,
// so the line and what lights agree; it moved from "how much it hears" to "how loud to speak").
// SQOL-U3: "Mesaj Önizlemelerini Göster" (on): the room chat's previews over the stream's stage.
import { CaretDown, Warning } from "@phosphor-icons/react";
import {
  Button,
  cn,
  Segmented,
  Select,
  Skeleton,
  Slider,
  SwapLabel,
  Switch,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { Slider as RadixSlider } from "radix-ui";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Row } from "@/features/settings/row";
import { ShareSettingsRows } from "@/features/share/settings";
import { enter, leave } from "@/features/social/motion";
import { voiceApi } from "@/lib/api/voice-api";
import {
  DEFAULT_VOICE_SETTINGS,
  type VoiceDevice,
  type VoiceKey,
  type VoiceSettings,
  type VoiceShortcut,
  type VoiceTalkMode,
} from "@/lib/api/voice-types";
import { formatPercent } from "@/lib/format";
import i18n from "@/lib/i18n";
import { askDiscord, DiscordInline, useDiscordAsk } from "./discord-offer";
import { KeyPicker, useKeyLine } from "./key-picker";
import { FULL_RANGE, VoiceMeter } from "./meter";
import { keptVoicePrefs, meterThreshold, useVoicePrefs } from "./prefs";
import { peekOf } from "./room-chat/settings";
import { useVoiceUi } from "./store";

/** A slider's value is saved this long after it stops moving. */
const SAVE_AFTER_MS = 250;
/** "Bu tuş seçilemez." stays this long under its row. */
const REFUSED_MS = 4000;
/** "Hassasiyet" when the player first turns "Otomatik" off. */
const MANUAL_SENSITIVITY = 30;
/** The rows' glide under Discord's card (the drawer's spring, as the room pill's). */
const glide = { type: "spring", stiffness: 480, damping: 40 } as const;

type Change = (patch: Partial<VoiceSettings>, later?: boolean) => void;

/** The settings, kept here while the page is open and saved through the launcher. */
function useVoiceSettings(): [VoiceSettings | null, Change] {
  const [settings, setSettings] = useState<VoiceSettings | null>(null);
  const current = useRef<VoiceSettings | null>(null);
  const pending = useRef<VoiceSettings | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const flush = useCallback(() => {
    clearTimeout(timer.current);
    const next = pending.current;
    pending.current = null;
    if (!next) return;
    void voiceApi
      .saveSettings(next)
      .then((kept) => {
        // A newer change waits: it wins over this answer.
        if (pending.current) return;
        current.current = kept;
        setSettings(kept);
        // The meters and the room panel read them too (VOICE-CALL).
        keptVoicePrefs(kept);
      })
      .catch(() => toast.error(i18n.t("voice.refused.failed")));
  }, []);

  useEffect(() => {
    let alive = true;
    void voiceApi
      .settings()
      .then((loaded) => {
        if (!alive) return;
        current.current = loaded;
        setSettings(loaded);
      })
      .catch(() => {
        if (!alive) return;
        current.current = DEFAULT_VOICE_SETTINGS;
        setSettings(DEFAULT_VOICE_SETTINGS);
      });
    return () => {
      alive = false;
      flush();
    };
  }, [flush]);

  // Discord's keys copied (the card here, or its toast): the launcher's answer is what stays.
  useEffect(
    () =>
      useDiscordAsk.subscribe((now, before) => {
        if (!now.copied || now.copied === before.copied || pending.current) return;
        current.current = now.copied;
        setSettings(now.copied);
      }),
    [],
  );
  // "Ses seviyesi" set from the room panel meanwhile (VOICE-CALL): the page shows it and keeps it.
  useEffect(
    () =>
      useVoicePrefs.subscribe((now) => {
        const there = now.settings;
        const here = current.current;
        if (!there || !here || pending.current || there.outputVolume === here.outputVolume) return;
        current.current = { ...here, outputVolume: there.outputVolume };
        setSettings(current.current);
      }),
    [],
  );

  const change = useCallback<Change>(
    (patch, later = false) => {
      if (!current.current) return;
      const next = { ...current.current, ...patch };
      current.current = next;
      pending.current = next;
      setSettings(next);
      clearTimeout(timer.current);
      if (later) timer.current = setTimeout(flush, SAVE_AFTER_MS);
      else flush();
    },
    [flush],
  );
  return [settings, change];
}

/**
 * The microphone's level while the engine hears it (the call's meter, long: VOICE-CALL), drawn
 * straight from each frame with no render; jade where it would go out. A hand-set sensitivity
 * shows as its step.
 */
function LevelBar({ threshold }: { threshold: number | null }) {
  const { t } = useTranslation();
  return (
    <span role="img" aria-label={t("voice.settings.level")} className="flex w-28">
      <VoiceMeter
        source="settings"
        bars={24}
        range={FULL_RANGE}
        threshold={threshold}
        className="w-full justify-between"
      />
    </span>
  );
}

/**
 * "Hassasiyet" on the meter itself (VOICE-CALL; Discord's and Steam's way): the bone line is how
 * loud the player must be, the bars past it light jade as they speak. Left of the line nothing
 * goes out; dragging it left hears quieter sounds ("Çok"), right only louder ones ("Az"). The
 * launcher keeps the threshold (0: everything goes) as before.
 */
function SensitivityMeter({
  value,
  onChange,
}: {
  value: number;
  onChange: (value: number, later: boolean) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  return (
    <div className="flex flex-col gap-2 py-3" data-voice-sensitivity="">
      <span id={id} className="text-[13px] font-medium text-bone-100">
        {t("voice.settings.sensitivity")}
      </span>
      <RadixSlider.Root
        value={[value]}
        min={0}
        max={100}
        step={1}
        onValueChange={([v]) => v !== undefined && onChange(v, true)}
        onValueCommit={([v]) => v !== undefined && onChange(v, false)}
        className="relative flex h-6 w-full cursor-pointer touch-none items-center select-none"
      >
        <RadixSlider.Track className="relative flex h-full grow items-center">
          <VoiceMeter
            source="settings"
            bars={48}
            range={FULL_RANGE}
            threshold={value / 100}
            className="w-full justify-between"
            barClassName="h-[14px] w-[3px]"
          />
        </RadixSlider.Track>
        <RadixSlider.Thumb
          aria-labelledby={id}
          aria-valuetext={formatPercent(value)}
          className="block h-6 w-[4px] rounded-full bg-bone-50 shadow-[0_0_0_2px_rgb(18_16_22/0.85)] outline-none transition-[transform,scale] duration-150 hover:scale-y-110 focus-visible:ring-2 focus-visible:ring-gold-300"
        />
      </RadixSlider.Root>
      {/* The meter's own axis, how loud: a voice right of the line goes out. */}
      <div className="flex justify-between text-[11px] text-ash-400">
        <span>{t("voice.level.quiet")}</span>
        <span>{t("voice.level.loud")}</span>
      </div>
    </div>
  );
}

function DeviceRow({
  title,
  devices,
  value,
  onChange,
}: {
  title: string;
  devices: VoiceDevice[];
  value: string | null;
  onChange: (id: string | null) => void;
}) {
  const { t } = useTranslation();
  // A device that is gone (unplugged) plays through the default: the select says so.
  const shown = value && devices.some((d) => d.id === value) ? value : SYSTEM_DEVICE;
  return (
    <Row title={title}>
      <div className="w-64">
        <Select<string>
          value={shown}
          onValueChange={(id) => onChange(id === SYSTEM_DEVICE ? null : id)}
          label={title}
          options={[
            { value: SYSTEM_DEVICE, label: t("voice.settings.defaultDevice") },
            // WebRTC lists the system's own "default" and "communications" too: the first row
            // says that, so they stay out.
            ...devices
              .filter((d) => d.id && d.id !== "default" && d.id !== "communications")
              .map((d) => ({ value: d.id, label: d.label })),
          ]}
        />
      </div>
    </Row>
  );
}

/** The select's value for "the system's default" (a select cannot hold an empty value). */
const SYSTEM_DEVICE = "system-default";

function KeyRow({
  title,
  shortcut,
  value,
  onPick,
  needed = false,
}: {
  title: string;
  shortcut: VoiceShortcut;
  value: VoiceKey | null;
  onPick: (key: VoiceKey | null) => void;
  /** Push-to-talk without a key never opens the microphone: the row says so. */
  needed?: boolean;
}) {
  const { t } = useTranslation();
  const clash = useKeyLine(value, shortcut);
  const [refused, setRefused] = useState<string | null>(null);
  useEffect(() => {
    if (!refused) return;
    const timer = window.setTimeout(() => setRefused(null), REFUSED_MS);
    return () => window.clearTimeout(timer);
  }, [refused]);
  const line = refused ?? clash ?? (needed && !value ? t("voice.settings.pttNeedsKey") : null);
  return (
    <Row
      title={title}
      // A real state, so the small gold triangle before a bone sentence (design.md §2).
      help={
        line ? (
          <span className="inline-flex items-start gap-1.5 text-bone-100">
            <Warning
              size={13}
              weight="fill"
              aria-hidden
              className="mt-[2px] shrink-0 text-gold-400"
            />
            {line}
          </span>
        ) : undefined
      }
    >
      <KeyPicker
        value={value}
        shortcut={shortcut}
        label={title}
        onPick={(key) => {
          setRefused(null);
          onPick(key);
        }}
        onRefused={setRefused}
      />
    </Row>
  );
}

function MicTest({ threshold }: { threshold: number | null }) {
  const { t } = useTranslation();
  const testing = useVoiceUi((s) => s.state.testing);
  return (
    <Row title={t("voice.settings.test")} help={t("voice.settings.testHelp")}>
      <div className="flex items-center gap-3">
        <LevelBar threshold={threshold} />
        <Button size="sm" variant="secondary" onClick={() => void voiceApi.testMic(!testing)}>
          <SwapLabel
            labels={[t("voice.settings.testStart"), t("voice.settings.testStop")]}
            active={testing ? 1 : 0}
          />
        </Button>
      </div>
    </Row>
  );
}

function Advanced({ settings, change }: { settings: VoiceSettings; change: Change }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const auto = settings.sensitivity === null;
  return (
    <div className="mt-2 border-t border-white/8 pt-1">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((on) => !on)}
        className="flex h-9 cursor-pointer items-center gap-1.5 rounded-md text-[13px] font-medium text-ash-300 outline-none transition-colors duration-150 hover:text-bone-50 focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        {t("voice.settings.advanced")}
        <CaretDown
          size={13}
          weight="bold"
          aria-hidden
          className={cn("transition-[rotate,color] duration-200", open && "rotate-180")}
        />
      </button>
      <AnimatePresence initial={false}>
        {open ? (
          <motion.div
            key="advanced"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.22) }}
            exit={{ opacity: 0, transition: leave(0.16) }}
          >
            <Row title={t("voice.settings.sensitivityAuto")}>
              <Switch
                checked={auto}
                onCheckedChange={(on) => change({ sensitivity: on ? null : MANUAL_SENSITIVITY })}
                label={t("voice.settings.sensitivityAuto")}
                hideLabel
              />
            </Row>
            {/* The launcher keeps a threshold (0: everything goes), drawn on the meter. */}
            {auto ? null : (
              <SensitivityMeter
                value={settings.sensitivity ?? MANUAL_SENSITIVITY}
                onChange={(sensitivity, later) => change({ sensitivity }, later)}
              />
            )}
            <Row title={t("voice.settings.echo")}>
              <Switch
                checked={settings.echoCancellation}
                onCheckedChange={(on) => change({ echoCancellation: on })}
                label={t("voice.settings.echo")}
                hideLabel
              />
            </Row>
            <Row title={t("voice.settings.autoGain")}>
              <Switch
                checked={settings.autoGain}
                onCheckedChange={(on) => change({ autoGain: on })}
                label={t("voice.settings.autoGain")}
                hideLabel
              />
            </Row>
            {settings.autoGain ? null : (
              <div className="py-3">
                <Slider
                  label={t("voice.settings.micVolume")}
                  value={settings.micVolume}
                  min={0}
                  max={200}
                  step={5}
                  onValueChange={(v) => change({ micVolume: v }, true)}
                  format={formatPercent}
                />
              </div>
            )}
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

export function VoiceSettingsSection() {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const [settings, change] = useVoiceSettings();
  const devices = useVoiceUi((s) => s.state.devices);
  // "Ses seviyesi" stops at the engine's top (200 %, or 100 % through `<audio>`).
  const volumeMax = useVoiceUi((s) => s.state.volumeMax ?? 200);
  // OVL-NEXT: in a push-to-talk-only room the push-to-talk key matters in "Konuşunca" too.
  const pttRoom = useVoiceUi((s) => s.state.room?.ptt === true);
  // SHARE: "Ekran paylaşımı" only while the service offers sharing; its sharing choices only where
  // this computer can share (SQOL-U1: the watching ones everywhere).
  const shareOffered = useVoiceUi((s) => Boolean(s.state.share?.service));
  const canShare = useVoiceUi((s) => Boolean(s.state.share?.canShare));

  // The engine runs for the devices and the level only while the page is open; a test the player
  // left running ends with it.
  useEffect(() => {
    void voiceApi.settingsOpen(true).catch(() => {});
    return () => {
      if (useVoiceUi.getState().state.testing) void voiceApi.testMic(false).catch(() => {});
      void voiceApi.settingsOpen(false).catch(() => {});
    };
  }, []);

  // The first visit may offer Discord's keys here, in place (VOICE-KEYS).
  useEffect(() => {
    useDiscordAsk.setState((s) => ({ inline: s.inline + 1 }));
    void askDiscord("settings");
    return () => useDiscordAsk.setState((s) => ({ inline: s.inline - 1 }));
  }, []);

  if (!settings) {
    return (
      <div aria-hidden className="space-y-4 pt-4">
        <Skeleton className="h-5 w-48" />
        <Skeleton className="h-5 w-64" />
        <Skeleton className="h-5 w-40" />
      </div>
    );
  }

  return (
    <>
      <Row title={t("voice.settings.enabled")} help={t("voice.settings.enabledHelp")}>
        <Switch
          checked={settings.enabled}
          onCheckedChange={(on) => change({ enabled: on })}
          label={t("voice.settings.enabled")}
          hideLabel
        />
      </Row>
      <AnimatePresence initial={false}>
        {settings.enabled ? (
          <motion.div
            key="voice"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.22) }}
            exit={{ opacity: 0, transition: leave(0.16) }}
          >
            <DiscordInline />
            {/* The rows glide (transform) as Discord's card comes and goes above them. */}
            <motion.div layout={layout ? "position" : false} transition={{ layout: glide }}>
              <DeviceRow
                title={t("voice.settings.mic")}
                devices={devices.inputs}
                value={settings.inputDevice}
                onChange={(inputDevice) => change({ inputDevice })}
              />
              <MicTest threshold={meterThreshold(settings)} />
              <DeviceRow
                title={t("voice.settings.output")}
                devices={devices.outputs}
                value={settings.outputDevice}
                onChange={(outputDevice) => change({ outputDevice })}
              />
              {/* Everyone at once (VOICE-CALL), on top of each person's own volume. */}
              <div className="py-3">
                <Slider
                  label={t("voice.level.output")}
                  value={Math.min(settings.outputVolume, volumeMax)}
                  min={0}
                  max={volumeMax}
                  step={5}
                  onValueChange={(outputVolume) => change({ outputVolume }, true)}
                  format={formatPercent}
                />
              </div>
              <Row title={t("voice.settings.mode")}>
                <Segmented<VoiceTalkMode>
                  value={settings.mode}
                  onValueChange={(mode) => change({ mode })}
                  label={t("voice.settings.mode")}
                  options={[
                    { value: "voice", label: t("voice.settings.modeVoice") },
                    { value: "pushToTalk", label: t("voice.settings.modePtt") },
                  ]}
                />
              </Row>
              {settings.mode === "pushToTalk" || pttRoom ? (
                <KeyRow
                  title={t("voice.settings.pttKey")}
                  shortcut="pushToTalk"
                  value={settings.pushToTalkKey}
                  onPick={(pushToTalkKey) => change({ pushToTalkKey })}
                  needed
                />
              ) : null}
              <Row title={t("voice.settings.noise")}>
                <Switch
                  checked={settings.noiseSuppression}
                  onCheckedChange={(on) => change({ noiseSuppression: on })}
                  label={t("voice.settings.noise")}
                  hideLabel
                />
              </Row>
              {/* Above "Kısayollar": under it the switch read as one more shortcut. The same
                  choice "Katılma ve ayrılma sesleri" was, for all of voice chat's sounds now. */}
              <Row title={t("voice.sounds.title")}>
                <Switch
                  checked={settings.sounds}
                  onCheckedChange={(on) => change({ sounds: on })}
                  label={t("voice.sounds.title")}
                  hideLabel
                />
              </Row>
              <Row title={t("voice.call.askFirst")}>
                <Switch
                  checked={settings.askBeforeCall}
                  onCheckedChange={(on) => change({ askBeforeCall: on })}
                  label={t("voice.call.askFirst")}
                  hideLabel
                />
              </Row>
              {/* SQOL-U3: the room chat's previews over the stream's stage. */}
              <Row title={t("roomChat.settings.peek")}>
                <Switch
                  checked={peekOf(settings)}
                  onCheckedChange={(on) => change({ peek: on })}
                  label={t("roomChat.settings.peek")}
                  hideLabel
                />
              </Row>
              <h3 className="mt-5 mb-0.5 text-[12.5px] font-semibold text-ash-300">
                {t("voice.settings.shortcuts")}
              </h3>
              <KeyRow
                title={t("voice.settings.muteKey")}
                shortcut="mute"
                value={settings.muteKey}
                onPick={(muteKey) => change({ muteKey })}
              />
              <KeyRow
                title={t("voice.settings.deafenKey")}
                shortcut="deafen"
                value={settings.deafenKey}
                onPick={(deafenKey) => change({ deafenKey })}
              />
              <Advanced settings={settings} change={change} />
              {/* SHARE: "Ekran paylaşımı", what a new share starts with (only while offered). */}
              {shareOffered ? (
                <ShareSettingsRows
                  value={settings.share}
                  onChange={(share) => change({ share })}
                  canShare={canShare}
                />
              ) : null}
            </motion.div>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </>
  );
}
