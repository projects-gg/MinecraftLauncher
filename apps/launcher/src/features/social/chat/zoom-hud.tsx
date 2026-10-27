// "Yazı boyutu %110  Sıfırla": a small plate at the top of a conversation while the player changes
// its text size (CHAT-PROD). It shows at once with the new size (keys pressed many times in a row
// never animate), stays while the pointer or the keyboard is on it, and fades away (opacity only)
// a moment after the last change. "Sıfırla" takes the size back to 100 % (Ctrl+0 does the same);
// at 100 % it keeps its place unseen, so the plate never changes width under the pointer. Screen
// readers hear the size from the conversation (announce), not from here.
import { Button } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type Ref, useEffect, useImperativeHandle, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatPercent } from "@/lib/format";
import { useChatTextSize } from "./text-size";
import { DEFAULT_TEXT_SIZE } from "./text-zoom";

/** How long the plate stays after the last change (ms). */
const SHOWN_MS = 1_600;

export interface ZoomHudHandle {
  /** The size changed (or was asked to change at its end): the plate shows. */
  show: () => void;
}

export function ZoomHud({ ref, onReset }: { ref: Ref<ZoomHudHandle>; onReset: () => void }) {
  const { t } = useTranslation();
  const size = useChatTextSize((s) => s.size);
  const [shown, setShown] = useState(false);
  const held = useRef(false);
  const timer = useRef<number | undefined>(undefined);
  const hideLater = () => {
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      if (!held.current) setShown(false);
    }, SHOWN_MS);
  };
  useImperativeHandle(ref, () => ({
    show: () => {
      setShown(true);
      hideLater();
    },
  }));
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const hold = (on: boolean) => {
    held.current = on;
    if (!on) hideLater();
  };
  const resettable = size !== DEFAULT_TEXT_SIZE;
  return (
    <AnimatePresence>
      {shown ? (
        <motion.div
          key="hud"
          className="absolute top-2 left-1/2 z-20 flex h-[34px] -translate-x-1/2 items-center gap-2.5 rounded-[var(--radius-panel)] bg-obsidian-850 pr-[3px] pl-3 shadow-[0_10px_30px_rgb(34_6_6/0.5)] ring-1 ring-white/10"
          initial={false}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0, transition: { duration: 0.18 } }}
          onPointerEnter={() => hold(true)}
          onPointerLeave={() => hold(false)}
          onFocus={() => hold(true)}
          onBlur={() => hold(false)}
        >
          <span aria-hidden className="flex items-baseline gap-2 whitespace-nowrap">
            <span className="text-[12.5px] text-ash-300">{t("chatProd.zoom.label")}</span>
            <span className="min-w-[4ch] font-display text-[14px] font-semibold text-gold-300 tabular-nums">
              {formatPercent(size)}
            </span>
          </span>
          <Button
            size="sm"
            variant="ghost"
            className="h-[28px] px-2.5"
            aria-label={t("chatProd.zoom.resetLabel")}
            tabIndex={resettable ? undefined : -1}
            aria-hidden={resettable ? undefined : true}
            style={resettable ? undefined : { visibility: "hidden" }}
            onClick={onReset}
          >
            {t("chatProd.zoom.reset")}
          </Button>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
