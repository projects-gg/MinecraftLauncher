// The skin tone (CHAT2 §3.1): one button beside the search, a raised hand in the chosen tone.
// Pressing it lays the six hands over the search row (they slide out from the button and fade,
// 180 ms; a fade only under reduced motion); a pick, Esc or a press outside folds them back. The
// row is a radio group: the arrows move, Enter or Space picks, the focus starts on the chosen one
// and returns to the button.
import { cn, enter, leave, Tooltip, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { SKIN_TONES, type SkinTone, withTone } from "./tone";

const HAND = { emoji: "✋", tones: "rule" as const };
const HAND_BASES = new Set([0x270b]);
const TONE_KEYS = ["default", "light", "mediumLight", "medium", "mediumDark", "dark"] as const;
/** The hands' cell, the button's size. */
const SIZE = 34;

export function TonePicker({ tone, onTone }: { tone: SkinTone; onTone: (tone: SkinTone) => void }) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const row = useRef<HTMLDivElement>(null);
  const [focused, setFocused] = useState<SkinTone>(tone);
  const toneName = (value: SkinTone) => t(`chat2.tone.options.${TONE_KEYS[value]}`);

  // Esc and a press outside fold the row; Esc never reaches the picker (it stays open).
  useEffect(() => {
    if (!open) return;
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
      button.current?.focus({ preventScroll: true });
    };
    const onPointer = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!row.current?.contains(target) && !button.current?.contains(target)) setOpen(false);
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onPointer, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onPointer, true);
    };
  }, [open]);

  // The focus goes to the chosen hand when the row opens.
  useEffect(() => {
    if (!open) return;
    setFocused(tone);
    const frame = requestAnimationFrame(() => {
      row.current
        ?.querySelector<HTMLButtonElement>(`[data-tone="${tone}"]`)
        ?.focus({ preventScroll: true });
    });
    return () => cancelAnimationFrame(frame);
  }, [open, tone]);

  const pick = (value: SkinTone) => {
    onTone(value);
    setOpen(false);
    button.current?.focus({ preventScroll: true });
  };

  const onRowKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const delta =
      event.key === "ArrowRight" || event.key === "ArrowDown"
        ? 1
        : event.key === "ArrowLeft" || event.key === "ArrowUp"
          ? -1
          : 0;
    if (event.key === "Home" || event.key === "End" || delta) {
      event.preventDefault();
      const next = (
        event.key === "Home" ? 0 : event.key === "End" ? 5 : (focused + delta + 6) % 6
      ) as SkinTone;
      setFocused(next);
      row.current?.querySelector<HTMLButtonElement>(`[data-tone="${next}"]`)?.focus();
    }
  };

  return (
    <div className="relative shrink-0">
      <Tooltip content={t("chat2.tone.label")} side="top">
        <button
          ref={button}
          type="button"
          aria-controls={open ? "emoji-tone-row" : undefined}
          aria-expanded={open}
          aria-label={t("chat2.tone.current", { tone: toneName(tone) })}
          onClick={() => setOpen((value) => !value)}
          className={cn(
            "chamfer relative flex cursor-pointer items-center justify-center bg-obsidian-800/80 [--cut:6px]",
            "transition-[background-color,transform,scale] duration-150 ease-out hover:bg-obsidian-700 active:scale-[0.94]",
            "focus-visible:outline-none",
            open && "bg-obsidian-700",
          )}
          style={{ width: SIZE, height: SIZE }}
        >
          <span aria-hidden className="picker-ring-static" />
          <span aria-hidden className="emoji-glyph text-[19px]">
            {withTone(HAND, tone, HAND_BASES)}
          </span>
        </button>
      </Tooltip>
      <AnimatePresence>
        {open ? (
          <motion.div
            ref={row}
            id="emoji-tone-row"
            role="radiogroup"
            aria-label={t("chat2.tone.label")}
            onKeyDown={onRowKey}
            className="absolute top-0 right-0 z-10 flex rounded-[var(--radius-panel)] bg-obsidian-850 p-0 shadow-[0_10px_30px_rgb(34_6_6/0.45)] ring-1 ring-white/10"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.18) }}
            exit={{ opacity: 0, transition: leave(0.14) }}
          >
            {SKIN_TONES.map((value) => {
              // Each hand comes from the button's place (the row's right end).
              const from = (5 - value) * SIZE;
              return (
                <motion.button
                  key={value}
                  type="button"
                  role="radio"
                  aria-checked={value === tone}
                  aria-label={toneName(value)}
                  data-tone={value}
                  tabIndex={value === focused ? 0 : -1}
                  onClick={() => pick(value)}
                  className={cn(
                    "chamfer relative flex cursor-pointer items-center justify-center [--cut:6px]",
                    "transition-[background-color,scale] duration-150 hover:bg-white/8 active:scale-[0.92] focus-visible:outline-none",
                    value === tone && "bg-white/10",
                  )}
                  style={{ width: SIZE, height: SIZE }}
                  initial={{ opacity: 0, transform: reduce ? "none" : `translateX(${from}px)` }}
                  animate={{
                    opacity: 1,
                    transform: "translateX(0px)",
                    transition: { ...enter(0.2), delay: reduce ? 0 : (5 - value) * 0.012 },
                  }}
                  exit={{
                    opacity: 0,
                    transform: reduce ? "none" : `translateX(${from * 0.6}px)`,
                    transition: leave(0.14),
                  }}
                >
                  <span aria-hidden className="picker-ring-static" />
                  {value === tone ? (
                    <span
                      aria-hidden
                      className="absolute bottom-[3px] left-1/2 h-[2px] w-3 -translate-x-1/2 rounded-full bg-gold-400"
                    />
                  ) : null}
                  <span aria-hidden className="emoji-glyph text-[19px]">
                    {withTone(HAND, value, HAND_BASES)}
                  </span>
                </motion.button>
              );
            })}
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}
