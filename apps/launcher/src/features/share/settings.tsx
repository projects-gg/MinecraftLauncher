// Settings → Sesli sohbet → "Ekran paylaşımı" (SHARE; share.md §3): what a new share starts with.
// "Kalite" and "Sesi de paylaş" on the surface; the rest under "Gelişmiş", closed. The picker shows
// the same choices and saves them here too. Saved through the settings page's own `change`, so a
// change of another voice setting never takes an older copy of these back.
import { CaretDown } from "@phosphor-icons/react";
import { cn, Segmented, Switch, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Row } from "@/features/settings/row";
import { DEFAULT_SHARE_SETTINGS, type SharePreset, type VoiceShareSettings } from "@/share/types";

const PRESETS: readonly SharePreset[] = ["low", "normal", "high"];

export function ShareSettingsRows({
  value,
  onChange,
}: {
  value: VoiceShareSettings | undefined;
  onChange: (share: VoiceShareSettings) => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const [open, setOpen] = useState(false);
  const share = { ...DEFAULT_SHARE_SETTINGS, ...value };
  const set = <K extends keyof VoiceShareSettings>(key: K, next: VoiceShareSettings[K]) =>
    onChange({ ...share, [key]: next });
  return (
    <>
      <h3 className="mt-5 mb-0.5 text-[12.5px] font-semibold text-ash-300">
        {t("share.settings.title")}
      </h3>
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
        <AnimatePresence initial={false}>
          {open ? (
            <motion.div
              key="advanced"
              initial={{ opacity: 0, y: reduce ? 0 : -4 }}
              animate={{
                opacity: 1,
                y: 0,
                transition: { duration: 0.22, ease: [0.16, 1, 0.3, 1] },
              }}
              exit={{ opacity: 0, transition: { duration: 0.14 } }}
            >
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
    </>
  );
}
