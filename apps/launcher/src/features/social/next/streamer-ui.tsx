// SOC-NEXT §3.4 on screen: "Yayıncı modu" in the friends settings' privacy (Kapalı / Açık / "Yayın
// programı açıkken"), a hidden value ("••••" and "Göster" for ten seconds) wherever a code or an
// invite link shows, and the title bar's quiet mark while it is on (a press turns it off).
import { Broadcast } from "@phosphor-icons/react";
import { cn, IconButton, Select, toast } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { TEXT_LINK } from "@/lib/text-link";
import { enter, leave } from "../motion";
import { useSocNext } from "./flag";
import { type StreamerMode, useSocNextPrefs } from "./prefs";
import { HIDDEN, REVEAL_MS, useStreamer, useStreamerOn } from "./streamer";

const MODES: readonly StreamerMode[] = ["off", "on", "auto"];

/** The settings row (privacy). */
export function StreamerSetting() {
  const { t } = useTranslation();
  const next = useSocNext();
  const mode = useSocNextPrefs((s) => s.streamer);
  if (!next) return null;
  return (
    <div>
      <div className="flex items-center gap-3">
        <span className="min-w-0 flex-1 text-[13px] font-medium text-bone-100">
          {t("socNext.streamer.title")}
        </span>
        <Select
          className="w-[228px]"
          label={t("socNext.streamer.title")}
          value={mode}
          onValueChange={(value) => useSocNextPrefs.getState().setStreamer(value as StreamerMode)}
          options={MODES.map((value) => ({ value, label: t(`socNext.streamer.${value}`) }))}
        />
      </div>
      <p className="mt-1 text-[12px] leading-snug text-ash-400">{t("socNext.streamer.help")}</p>
    </div>
  );
}

/** Whether what a stream must not show hides now, and "Göster" (ten seconds). */
export function useReveal(): { hidden: boolean; reveal: () => void } {
  const on = useStreamerOn();
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (!shown) return;
    const timer = window.setTimeout(() => setShown(false), REVEAL_MS);
    return () => window.clearTimeout(timer);
  }, [shown]);
  useEffect(() => {
    if (!on) setShown(false);
  }, [on]);
  return { hidden: on && !shown, reveal: () => setShown(true) };
}

/** "••••" and "Göster" in a hidden value's place. */
export function HiddenValue({ onShow, className }: { onShow: () => void; className?: string }) {
  const { t } = useTranslation();
  return (
    <span className={cn("inline-flex items-center gap-2", className)}>
      <span aria-hidden className="tracking-[0.18em] text-ash-300">
        {HIDDEN}
      </span>
      <button
        type="button"
        onClick={onShow}
        className={cn(TEXT_LINK, "text-[12px] tracking-normal")}
      >
        {t("socNext.streamer.show")}
      </button>
    </span>
  );
}

/** A picture a stream must not show (a QR) in its own size, quiet; a press shows it. */
export function HiddenTile({ size, onShow }: { size: number; onShow: () => void }) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={onShow}
      aria-label={t("socNext.streamer.show")}
      className="flex shrink-0 cursor-pointer flex-col items-center justify-center gap-1.5 rounded-md bg-obsidian-950/60 text-[12px] text-ash-300 ring-1 ring-white/8 outline-none transition-[box-shadow,color,transform,scale] duration-150 hover:text-bone-100 hover:ring-white/16 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.98]"
      style={{ width: size, height: size }}
    >
      <Broadcast size={20} aria-hidden />
      {t("socNext.streamer.show")}
    </button>
  );
}

/**
 * A value a stream must not show (a code, an invite link): "••••" and "Göster" while streamer
 * mode is on; "Göster" shows it for ten seconds. Off, it is the value itself.
 */
export function Hidden({ children, className }: { children: ReactNode; className?: string }) {
  const { hidden, reveal } = useReveal();
  if (!hidden) return <>{children}</>;
  return <HiddenValue onShow={reveal} className={className} />;
}

/** The title bar's mark while streamer mode is on: a press turns it off (the automatic one for
 * this session, a chosen one for good). It fades in and out where it stands. */
export function StreamerMark() {
  const { t } = useTranslation();
  const on = useStreamerOn();
  const turnOff = () => {
    const prefs = useSocNextPrefs.getState();
    if (prefs.streamer === "on") prefs.setStreamer("off");
    else useStreamer.getState().dismiss();
    const words = t("socNext.streamer.turnedOff");
    announce(words);
    toast(words);
  };
  return (
    <AnimatePresence initial={false}>
      {on ? (
        <motion.span
          key="streamer"
          className="flex"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: enter(0.2) }}
          exit={{ opacity: 0, transition: leave(0.14) }}
        >
          <IconButton
            label={t("socNext.streamer.mark")}
            icon={<Broadcast size={16} />}
            size="sm"
            onClick={turnOff}
          />
        </motion.span>
      ) : null}
    </AnimatePresence>
  );
}
