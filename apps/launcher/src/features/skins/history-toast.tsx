// The toast after a skin left the history: its picture, what happened, "Geri al", and a thin line
// that drains over the undo window (paused while the toasts are hovered, as Sonner pauses).
import { ArrowCounterClockwise } from "@phosphor-icons/react";
import { toast } from "@pl/ui";
import { useEffect, useRef } from "react";
import i18n from "@/lib/i18n";

/** How long "Geri al" stays offered (the backend keeps the entry for five minutes). */
export const HISTORY_UNDO_MS = 6000;

function DrainLine() {
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const line = ref.current;
    if (!line || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const animation = line.animate([{ transform: "scaleX(1)" }, { transform: "scaleX(0)" }], {
      duration: HISTORY_UNDO_MS,
      easing: "linear",
      fill: "forwards",
    });
    const toaster = line.closest("[data-sonner-toaster]");
    const pause = () => animation.pause();
    const resume = () => {
      if (!document.hidden) animation.play();
    };
    const visibility = () => (document.hidden ? pause() : resume());
    toaster?.addEventListener("mouseenter", pause);
    toaster?.addEventListener("mouseleave", resume);
    document.addEventListener("visibilitychange", visibility);
    return () => {
      animation.cancel();
      toaster?.removeEventListener("mouseenter", pause);
      toaster?.removeEventListener("mouseleave", resume);
      document.removeEventListener("visibilitychange", visibility);
    };
  }, []);
  return (
    <span
      ref={ref}
      aria-hidden
      className="absolute right-3.5 bottom-1.5 left-3.5 h-[2px] origin-left rounded-full bg-gold-400/70"
    />
  );
}

function RemovedToast({ thumbUrl, onUndo }: { thumbUrl: string | null; onUndo: () => void }) {
  return (
    <div className="flex w-[312px] items-center gap-3 font-normal">
      <span
        aria-hidden
        className="flex size-8 shrink-0 items-start justify-center overflow-hidden rounded-[var(--radius-head)] bg-obsidian-950/60 ring-1 ring-white/10"
      >
        {thumbUrl ? (
          // Head and shoulders of the 16×32 picture, two screen pixels per skin pixel.
          <img
            src={thumbUrl}
            alt=""
            width={32}
            height={64}
            draggable={false}
            className="max-w-none opacity-80 [image-rendering:pixelated]"
          />
        ) : null}
      </span>
      <p className="min-w-0 flex-1 text-[13.5px] leading-snug font-semibold break-words text-bone-50">
        {i18n.t("skins.history.removed")}
      </p>
      <button
        type="button"
        onClick={onUndo}
        className="pointer-events-auto -my-1 flex h-11 shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-3 text-[13px] font-semibold text-gold-300 transition-[background-color,transform,scale] duration-150 hover:bg-gold-500/12 focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none active:scale-[0.97]"
      >
        <ArrowCounterClockwise size={16} weight="bold" aria-hidden />
        {i18n.t("skins.history.undo")}
      </button>
      <DrainLine />
    </div>
  );
}

/** Shows the undo toast of a removal from the history; returns its id. */
export function showRemovedToast(thumbUrl: string | null, onUndo: () => void): string | number {
  return toast.custom(() => <RemovedToast thumbUrl={thumbUrl} onUndo={onUndo} />, {
    duration: HISTORY_UNDO_MS,
  });
}
