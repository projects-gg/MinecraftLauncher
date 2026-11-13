// Settings → Sesli sohbet → "Ekran Paylaşımı" (SHARE, SQOL-U1; share.md §3, sqol.md §1.1). First
// "İzlerken" (SQOL): "Yayın Kalitesi" for every share the player watches, and "Biri Konuşunca Yayın
// Sesini Kıs" (off by default; while on, "Kısma Miktarı" and "Ben Konuşunca da Kıs"); a computer that
// cannot share watches all the same and sees only these. Then "Paylaşırken", what a new share starts
// with: "Kalite" and "Sesi de Paylaş" on the surface, the rest under "Gelişmiş", closed. The picker
// shows the same sharing choices and saves them here too. Saved through the settings page's own
// `change`, so a change of another voice setting never takes an older copy of these back; a watching
// choice made from a stream's menu meanwhile shows here.
import { CaretDown } from "@phosphor-icons/react";
import { cn, Segmented, Slider, Switch, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Row } from "@/features/settings/row";
import { useVoicePrefs } from "@/features/voice/prefs";
import { formatPercent } from "@/lib/format";
import { DUCK_LEVEL_MAX, DUCK_LEVEL_MIN } from "@/share/stream-audio";
import {
  DEFAULT_SHARE_SETTINGS,
  type SharePreset,
  type ShareViewQuality,
  type VoiceShareSettings,
} from "@/share/types";

const PRESETS: readonly SharePreset[] = ["low", "normal", "high"];
const VIEW_QUALITIES: readonly ShareViewQuality[] = ["auto", "high", "saver"];
/** "Kısma Miktarı" is saved this long after the slider stops (as the page's other sliders). */
const SAVE_AFTER_MS = 250;

/** The watching choices a stream's menu can change (the launcher keeps them; `share_view_prefs`). */
const VIEW_KEYS = ["viewQuality", "duck", "duckLevel", "duckSelf"] as const;

/** A part's heading under "Ekran Paylaşımı". */
const partHeading = "mt-4 mb-0 text-[12px] font-medium text-ash-400";

/** A sub-part that opens under its switch: it fades where it is. */
function useFold() {
  const { reduce } = useMotionPrefs();
  return {
    initial: { opacity: 0, y: reduce ? 0 : -4 },
    animate: { opacity: 1, y: 0, transition: { duration: 0.22, ease: [0.16, 1, 0.3, 1] as const } },
    exit: { opacity: 0, transition: { duration: 0.14 } },
  };
}

type SetShare = <K extends keyof VoiceShareSettings>(key: K, next: VoiceShareSettings[K]) => void;

export function ShareSettingsRows({
  value,
  onChange,
  canShare = true,
}: {
  value: VoiceShareSettings | undefined;
  onChange: (share: VoiceShareSettings) => void;
  /** This computer can share (false: only "İzlerken"; watching works everywhere). */
  canShare?: boolean;
}) {
  const { t } = useTranslation();
  const fold = useFold();
  const share = { ...DEFAULT_SHARE_SETTINGS, ...value };
  const latest = useRef({ share, onChange });
  latest.current = { share, onChange };
  const set: SetShare = (key, next) => onChange({ ...share, [key]: next });

  // A choice made from a stream's menu while this page is open shows here (and this page's next
  // save keeps it, instead of its older copy).
  useEffect(
    () =>
      useVoicePrefs.subscribe((now, before) => {
        const there = { ...DEFAULT_SHARE_SETTINGS, ...now.settings?.share };
        const was = { ...DEFAULT_SHARE_SETTINGS, ...before.settings?.share };
        const here = latest.current.share;
        const moved = VIEW_KEYS.filter(
          (key) => there[key] !== was[key] && there[key] !== here[key],
        );
        if (!moved.length) return;
        const pick = <K extends (typeof VIEW_KEYS)[number]>(key: K): VoiceShareSettings[K] =>
          moved.includes(key) ? there[key] : here[key];
        latest.current.onChange({
          ...here,
          viewQuality: pick("viewQuality"),
          duck: pick("duck"),
          duckLevel: pick("duckLevel"),
          duckSelf: pick("duckSelf"),
        });
      }),
    [],
  );

  // "Kısma Miktarı" follows the thumb at once and is saved once it rests.
  const [level, setLevel] = useState(share.duckLevel);
  const levelTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => setLevel(share.duckLevel), [share.duckLevel]);
  useEffect(() => () => clearTimeout(levelTimer.current), []);

  return (
    <>
      <h3 className="mt-5 mb-0.5 text-[12.5px] font-semibold text-ash-300">
        {t("share.settings.title")}
      </h3>
      <h4 className={partHeading}>{t("shareView.settings.watching")}</h4>
      <Row title={t("shareView.settings.quality")}>
        <Segmented
          label={t("shareView.settings.quality")}
          value={share.viewQuality}
          onValueChange={(viewQuality) => set("viewQuality", viewQuality)}
          options={VIEW_QUALITIES.map((q) => ({ value: q, label: t(`shareView.quality.${q}`) }))}
        />
      </Row>
      <Row title={t("shareView.settings.duck")}>
        <Switch
          checked={share.duck}
          onCheckedChange={(on) => set("duck", on)}
          label={t("shareView.settings.duck")}
          hideLabel
        />
      </Row>
      <AnimatePresence initial={false}>
        {share.duck ? (
          <motion.div key="duck" {...fold}>
            <div className="border-b border-white/6 py-3">
              <Slider
                label={t("shareView.settings.duckLevel")}
                value={level}
                min={DUCK_LEVEL_MIN}
                max={DUCK_LEVEL_MAX}
                step={5}
                format={formatPercent}
                onValueChange={(next) => {
                  setLevel(next);
                  clearTimeout(levelTimer.current);
                  levelTimer.current = setTimeout(() => {
                    const now = latest.current;
                    now.onChange({ ...now.share, duckLevel: next });
                  }, SAVE_AFTER_MS);
                }}
              />
            </div>
            <Row title={t("shareView.settings.duckSelf")}>
              <Switch
                checked={share.duckSelf}
                onCheckedChange={(on) => set("duckSelf", on)}
                label={t("shareView.settings.duckSelf")}
                hideLabel
              />
            </Row>
          </motion.div>
        ) : null}
      </AnimatePresence>
      {canShare ? <SharingRows share={share} set={set} /> : null}
    </>
  );
}

/** "Paylaşırken": what a new share starts with. */
function SharingRows({ share, set }: { share: VoiceShareSettings; set: SetShare }) {
  const { t } = useTranslation();
  const fold = useFold();
  const [open, setOpen] = useState(false);
  const advancedId = useId();
  return (
    <>
      <h4 className={partHeading}>{t("shareView.settings.sharing")}</h4>
      <Row title={t("share.quality.label")}>
        <Segmented
          label={t("share.quality.label")}
          value={share.quality}
          onValueChange={(quality) => set("quality", quality)}
          options={PRESETS.map((p) => ({ value: p, label: t(`share.quality.${p}`) }))}
        />
      </Row>
      <Row title={t("share.audio.label")}>
        <Switch
          checked={share.audio}
          onCheckedChange={(on) => set("audio", on)}
          label={t("share.audio.label")}
          hideLabel
        />
      </Row>
      <div className="mt-1">
        <button
          type="button"
          aria-expanded={open}
          aria-controls={advancedId}
          onClick={() => setOpen((on) => !on)}
          className="flex h-9 cursor-pointer items-center gap-1.5 rounded-md text-[13px] font-medium text-ash-300 outline-none transition-colors duration-150 hover:text-bone-50 focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          {t("share.advanced.label")}
          <CaretDown
            size={13}
            weight="bold"
            aria-hidden
            className={cn("transition-[rotate,color] duration-200", open && "rotate-180")}
          />
        </button>
        {/* Always there (empty while closed), so the button's `aria-controls` names an element. */}
        <div id={advancedId}>
          <AnimatePresence initial={false}>
            {open ? (
              <motion.div key="advanced" {...fold}>
                <Row title={t("share.advanced.motion")}>
                  <Segmented
                    label={t("share.advanced.motion")}
                    value={share.motion}
                    onValueChange={(motion) => set("motion", motion)}
                    options={[
                      { value: "auto", label: t("share.advanced.motionAuto") },
                      { value: "clarity", label: t("share.advanced.clarity") },
                      { value: "motion", label: t("share.advanced.smooth") },
                    ]}
                  />
                </Row>
                <Row title={t("share.advanced.fps")}>
                  <Segmented
                    label={t("share.advanced.fps")}
                    value={share.fps === null ? "auto" : String(share.fps)}
                    onValueChange={(fps) => set("fps", fps === "auto" ? null : Number(fps))}
                    options={[
                      { value: "auto", label: t("share.advanced.fpsAuto") },
                      { value: "15", label: "15" },
                      { value: "30", label: "30" },
                      ...(share.quality === "high" ? [{ value: "60", label: "60" }] : []),
                    ]}
                  />
                </Row>
                <Row title={t("share.advanced.audioSource")}>
                  <Segmented
                    label={t("share.advanced.audioSource")}
                    value={share.audioSource}
                    disabled={!share.audio}
                    onValueChange={(source) => set("audioSource", source)}
                    options={[
                      { value: "app", label: t("share.advanced.audioApp") },
                      { value: "system", label: t("share.advanced.audioSystem") },
                    ]}
                  />
                </Row>
                <Row title={t("share.advanced.cursor")}>
                  <Switch
                    checked={share.cursor}
                    onCheckedChange={(on) => set("cursor", on)}
                    label={t("share.advanced.cursor")}
                    hideLabel
                  />
                </Row>
                <Row title={t("shareView.settings.thumbs")}>
                  <Switch
                    checked={share.thumbs !== false}
                    onCheckedChange={(on) => set("thumbs", on)}
                    label={t("shareView.settings.thumbs")}
                    hideLabel
                  />
                </Row>
                <Row title={t("share.advanced.hideLauncher")}>
                  <Switch
                    checked={share.hideLauncher}
                    onCheckedChange={(on) => set("hideLauncher", on)}
                    label={t("share.advanced.hideLauncher")}
                    hideLabel
                  />
                </Row>
                <Row title={t("share.advanced.quiet")}>
                  <Switch
                    checked={share.quiet}
                    onCheckedChange={(on) => set("quiet", on)}
                    label={t("share.advanced.quiet")}
                    hideLabel
                  />
                </Row>
              </motion.div>
            ) : null}
          </AnimatePresence>
        </div>
      </div>
    </>
  );
}
