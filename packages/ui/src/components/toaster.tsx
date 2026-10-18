import { Toaster as Sonner } from "sonner";

/** Toasts are for transient confirmations only; errors that need action stay inline. */
export function Toaster() {
  return (
    <Sonner
      position="bottom-right"
      offset={{ bottom: 88, right: 20 }}
      gap={10}
      toastOptions={{
        unstyled: true,
        classNames: {
          // Not `glass-strong`: see `toast-surface` in styles.css.
          toast:
            "toast-surface flex w-[320px] items-start gap-3 rounded-[var(--radius-panel)] p-3 text-[13px] text-bone-100",
          title: "font-semibold text-bone-50",
          description: "mt-0.5 text-ash-300",
          // `pointer-events-auto`: a modal sheet turns pointer events off outside itself, the
          // toasts included; their buttons must still work (the sheet ignores those clicks).
          actionButton:
            "pointer-events-auto ml-auto shrink-0 cursor-pointer rounded-md bg-gold-500 px-2.5 py-1 text-[12px] font-semibold text-gold-ink transition-[filter,transform,scale] duration-150 ease-out hover:brightness-110 active:scale-[0.97]",
          cancelButton:
            "pointer-events-auto ml-auto shrink-0 cursor-pointer rounded-md bg-white/8 px-2.5 py-1 text-[12px] text-bone-100 transition-[background-color,transform,scale] duration-150 ease-out hover:bg-white/12 active:scale-[0.97]",
          // The state is the icon's colour; the sentence stays legible bone (a whole line of green
          // or red text shouts).
          error: "[&_[data-icon]]:text-flare-400",
          success: "[&_[data-icon]]:text-jade-400",
        },
      }}
    />
  );
}

export { toast } from "sonner";
