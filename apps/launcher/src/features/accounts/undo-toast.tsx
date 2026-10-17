// The toast after a removal: who left, what that means, "Geri al", and a thin line that drains
// over the undo window (paused while the toasts are hovered, as Sonner pauses its timer).
import { ArrowCounterClockwise } from "@phosphor-icons/react";
import { toast } from "@pl/ui";
import { useEffect, useRef } from "react";
import type { AccountSummary } from "@/lib/api";
import i18n from "@/lib/i18n";
import { AccountFace } from "./account-face";
import { UNDO_MS } from "./removal-queue";

function DrainLine() {
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const line = ref.current;
    if (!line || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const animation = line.animate([{ transform: "scaleX(1)" }, { transform: "scaleX(0)" }], {
      duration: UNDO_MS,
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

function UndoToast({
  account,
  title,
  undoLabel,
  onUndo,
}: {
  account: AccountSummary;
  title: string;
  undoLabel: string;
  onUndo: () => void;
}) {
  return (
    <div className="flex w-[312px] items-center gap-3 font-normal">
      <AccountFace account={account} size={32} className="opacity-80" />
      <p className="min-w-0 flex-1 text-[13.5px] leading-snug font-semibold break-words text-bone-50">
        {title}
      </p>
      <button
        type="button"
        onClick={onUndo}
        className="pointer-events-auto -my-1 flex h-11 shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-3 text-[13px] font-semibold text-gold-300 transition-[background-color,transform] duration-150 hover:bg-gold-500/12 focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none active:scale-[0.97]"
      >
        <ArrowCounterClockwise size={16} weight="bold" aria-hidden />
        {undoLabel}
      </button>
      <DrainLine />
    </div>
  );
}

/** Shows the undo toast of a removal; returns its id (for dismissing it on undo). */
export function showUndoToast(
  account: AccountSummary,
  handlers: { onUndo: () => void; onClose: () => void },
): string | number {
  const t = i18n.t.bind(i18n);
  const microsoft = account.kind === "microsoft";
  return toast.custom(
    () => (
      <UndoToast
        account={account}
        title={
          microsoft
            ? t("accounts.center.removedMicrosoft", { name: account.username })
            : t("accounts.center.removed", { name: account.username })
        }
        undoLabel={t("accounts.center.undo")}
        onUndo={handlers.onUndo}
      />
    ),
    { duration: UNDO_MS, onAutoClose: handlers.onClose, onDismiss: handlers.onClose },
  );
}
