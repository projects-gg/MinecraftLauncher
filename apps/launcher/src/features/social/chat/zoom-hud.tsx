// "Yazı boyutu %110  Sıfırla": a quiet line under the composer's field while the player changes the
// chat's text size (CHAT-PROD), in the line that otherwise holds why a message cannot go (that
// reason wins). It covers no message and moves nothing. It shows at once with the new size (keys
// pressed many times in a row never animate), stays while the pointer or the keyboard is on it,
// and fades away (opacity only) a moment after the last change. "Sıfırla" takes the size back to
// 100 % (Ctrl+0 does the same) and is not there at 100 %; the words before it never move. Screen
// readers hear the size from the conversation (announce), not from here.
import { AnimatePresence, motion } from "motion/react";
import { type Ref, useEffect, useImperativeHandle, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { percentParts } from "@/lib/format";
import { useChatTextSize } from "./text-size";
import { DEFAULT_TEXT_SIZE } from "./text-zoom";

/** How long the line stays after the last change (ms). */
const SHOWN_MS = 1_600;

export interface ZoomHudHandle {
  /** The size changed (or was asked to change at its end): the line shows. */
  show: () => void;
}

/**
 * A live percentage (design.md: Tektur digits), the sign in the UI font (modcenter's Percent). It
 * is as wide as the widest size ("%150"), so "Sıfırla" after it stays put while the size changes.
 */
function Level({ value }: { value: number }) {
  const { figure, signFirst } = percentParts(value);
  const sign = <span className="font-sans font-medium">%</span>;
  return (
    <span className="inline-block min-w-[2.9em] font-display text-[12.5px] font-semibold text-gold-300 tabular-nums">
      {signFirst ? sign : null}
      {figure}
      {signFirst ? null : sign}
    </span>
  );
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
  return (
    <AnimatePresence>
      {shown ? (
        <motion.span
          key="hud"
          className="flex h-[18px] items-center gap-2"
          initial={false}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0, transition: { duration: 0.18 } }}
          onPointerEnter={() => hold(true)}
          onPointerLeave={() => hold(false)}
          onFocus={() => hold(true)}
          onBlur={() => hold(false)}
        >
          <span aria-hidden className="flex items-baseline gap-1.5 whitespace-nowrap text-ash-300">
            {t("chatProd.zoom.label")}
            <Level value={size} />
          </span>
          {size === DEFAULT_TEXT_SIZE ? null : (
            <button
              type="button"
              aria-label={t("chatProd.zoom.resetLabel")}
              onClick={onReset}
              // A press keeps the focus where it was (the field, usually).
              onMouseDown={(event) => event.preventDefault()}
              className="group/reset relative inline-flex h-[18px] shrink-0 cursor-pointer items-center outline-none transition-[transform,scale] duration-150 ease-out active:scale-[0.97]"
            >
              <span
                aria-hidden
                className="chamfer-ring absolute bg-gold-300 opacity-0 transition-opacity duration-150 [--cut:4px] group-focus-visible/reset:opacity-100"
              />
              <span className="chamfer relative inline-flex h-[18px] items-center bg-gold-500/12 px-1.5 text-[11.5px] font-semibold text-gold-200 transition-colors duration-150 [--cut:4px] group-hover/reset:bg-gold-500/20">
                {t("chatProd.zoom.reset")}
              </span>
            </button>
          )}
        </motion.span>
      ) : null}
    </AnimatePresence>
  );
}
