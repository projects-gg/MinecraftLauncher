// Ayarlar → Sesli sohbet (VOICE-L3; voice.md §5.2): the one switch first ("Kapalıyken kimse seni
// arayamaz."), then what most players touch: the microphone and its test (a live level bar while
// it runs: the player hears themselves, nothing goes anywhere), the output, when the microphone is
// on ("Konuşunca" / "Bas-konuş" with its key), noise suppression, the two shortcuts and the join
// sounds; the rest one level down under "Gelişmiş". Everything under the switch fades away while it
// is off. A change is saved at once (a slider after its drag settles); the launcher's answer is
// what stays (it holds values to their ranges and a key to one shortcut). The voice engine runs
// for the devices and the level bar only while this page is open.
import { CaretDown } from "@phosphor-icons/react";
import { Button, cn, Segmented, Select, Skeleton, Slider, SwapLabel, Switch, toast } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Row } from "@/features/settings/row";
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
import { KeyPicker, useKeyLine } from "./key-picker";
import { useVoiceUi } from "./store";

/** A slider's value is saved this long after it stops moving. */
const SAVE_AFTER_MS = 250;
/** "Bu tuş seçilemez." stays this long under its row. */
const REFUSED_MS = 4000;
/** "Hassasiyet" when the player first turns "Otomatik" off. */
const MANUAL_SENSITIVITY = 30;

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

/** The microphone's level while the engine hears it: drawn straight from each frame, no render. */
function LevelBar() {
  const { t } = useTranslation();
  const fill = useRef<HTMLSpanElement>(null);
  const testing = useVoiceUi((s) => s.state.testing);
  useEffect(
    () =>
      voiceApi.onLevel(({ level, open }) => {
        const element = fill.current;
        if (!element) return;
        element.style.transform = `scaleX(${Math.max(0, Math.min(1, level))})`;
        element.toggleAttribute("data-open", open);
      }),
    [],
  );
  useEffect(() => {
    if (!testing && fill.current) fill.current.style.transform = "scaleX(0)";
  }, [testing]);
  return (
    <span
      role="img"
      aria-label={t("voice.settings.level")}
      className="relative block h-1.5 w-28 overflow-hidden rounded-full bg-white/10"
    >
      {/* The fill follows the frames on the compositor; above the sensitivity it turns jade. */}
      <span
        ref={fill}
        className="absolute inset-0 origin-left bg-bone-100/70 transition-[transform,background-color] duration-75 ease-linear data-[open]:bg-jade-400"
        style={{ transform: "scaleX(0)" }}
      />
    </span>
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
  const shown = value && devices.some((d) => d.id === value) ? value : "";
  return (
    <Row title={title}>
      <div className="w-64">
        <Select<string>
          value={shown}
          onValueChange={(id) => onChange(id || null)}
          label={title}
          options={[
            { value: "", label: t("voice.settings.defaultDevice") },
            ...devices.map((d) => ({ value: d.id, label: d.label })),
          ]}
        />
      </div>
    </Row>
  );
}

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
    <Row title={title} help={line ?? undefined}>
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

function MicTest() {
  const { t } = useTranslation();
  const testing = useVoiceUi((s) => s.state.testing);
  return (
    <Row title={t("voice.settings.test")} help={t("voice.settings.testHelp")}>
      <div className="flex items-center gap-3">
        <LevelBar />
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
            {/* The launcher keeps a threshold (0: everything goes); the slider says how much it
                hears, so right is more sensitive. */}
            {auto ? null : (
              <div className="py-3">
                <Slider
                  label={t("voice.settings.sensitivity")}
                  value={100 - (settings.sensitivity ?? MANUAL_SENSITIVITY)}
                  min={0}
                  max={100}
                  onValueChange={(v) => change({ sensitivity: 100 - v }, true)}
                  format={formatPercent}
                />
              </div>
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
  const [settings, change] = useVoiceSettings();
  const devices = useVoiceUi((s) => s.state.devices);

  // The engine runs for the devices and the level only while the page is open; a test the player
  // left running ends with it.
  useEffect(() => {
    void voiceApi.settingsOpen(true).catch(() => {});
    return () => {
      if (useVoiceUi.getState().state.testing) void voiceApi.testMic(false).catch(() => {});
      void voiceApi.settingsOpen(false).catch(() => {});
    };
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
            <DeviceRow
              title={t("voice.settings.mic")}
              devices={devices.inputs}
              value={settings.inputDevice}
              onChange={(inputDevice) => change({ inputDevice })}
            />
            <MicTest />
            <DeviceRow
              title={t("voice.settings.output")}
              devices={devices.outputs}
              value={settings.outputDevice}
              onChange={(outputDevice) => change({ outputDevice })}
            />
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
            {settings.mode === "pushToTalk" ? (
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
            <Row title={t("voice.settings.sounds")}>
              <Switch
                checked={settings.sounds}
                onCheckedChange={(on) => change({ sounds: on })}
                label={t("voice.settings.sounds")}
                hideLabel
              />
            </Row>
            <Advanced settings={settings} change={change} />
          </motion.div>
        ) : null}
      </AnimatePresence>
    </>
  );
}
