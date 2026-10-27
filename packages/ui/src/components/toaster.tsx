import { Toaster as Sonner } from "sonner";

/**
 * Toasts are for transient confirmations only; errors that need action stay inline. They sit on
 * the page's grid (SMALL10b): their right edge on the page gutter's (`--spacing-page`, where the
 * news column, the cards and the panels end) and as wide as the news column (`--spacing-news`), so
 * on Home a toast lies exactly over the column's foot instead of overhanging it.
 */
export function Toaster() {
  return (
    <Sonner
      position="bottom-right"
      // The gutter where the app defines one (the launcher's pages), else 20 px (Studio).
      offset={{ bottom: 88, right: "var(--spacing-page, 20px)" }}
      gap={10}
      toastOptions={{
        unstyled: true,
        classNames: {
          // Not `glass-strong`: see `toast-surface` in styles.css.
          toast:
            "toast-surface flex w-news items-start gap-3 rounded-[var(--radius-panel)] p-3 text-[13px] text-bone-100",
          // `text-balance` (design.md §3, as headings): a toast's two or three short lines come out
          // even, never one word alone on the last.
          title: "font-semibold text-balance text-bone-50",
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
          // Something that could not be done now (BETA-GATE: "couldn't check"), as inline warnings mark it.
          warning: "[&_[data-icon]]:text-gold-300",
        },
      }}
    />
  );
}

export { toast } from "sonner";
