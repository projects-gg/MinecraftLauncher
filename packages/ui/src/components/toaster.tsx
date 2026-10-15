import { Toaster as Sonner } from "sonner";

/** Toasts are for transient confirmations only; errors that need action stay inline. */
export function Toaster() {
  return (
    <Sonner
      position="bottom-right"
      offset={{ bottom: 104, right: 24 }}
      gap={10}
      toastOptions={{
        unstyled: true,
        classNames: {
          toast:
            "glass-strong flex w-[340px] items-start gap-3 rounded-[var(--radius-panel)] p-3.5 text-[13px] text-bone-100 shadow-[0_18px_48px_rgb(34_6_6/0.5)]",
          title: "font-semibold text-bone-50",
          description: "mt-0.5 text-ash-300",
          actionButton:
            "ml-auto shrink-0 rounded-md bg-gold-500 px-2.5 py-1 text-[12px] font-semibold text-gold-ink",
          cancelButton: "ml-auto shrink-0 rounded-md bg-white/8 px-2.5 py-1 text-[12px] text-bone-100",
          error: "[&_[data-title]]:text-flare-400",
          success: "[&_[data-title]]:text-jade-400",
        },
      }}
    />
  );
}

export { toast } from "sonner";
