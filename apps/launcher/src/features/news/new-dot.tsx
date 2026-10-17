import { cn } from "@pl/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/**
 * The small, still gold dot before the date of a news item the player has not seen yet (the
 * same mark as a tab's unsaved-changes dot). Opening the article fades it out; it keeps its room
 * until the list is shown again, so the date beside it never moves. Screen readers get the word
 * from the link (`NewLabel`), not from the dot.
 */
export function NewDot({ on, className }: { on: boolean; className?: string }) {
  const [kept, setKept] = useState(on);
  if (on && !kept) setKept(true);
  if (!kept) return null;
  return (
    <span
      aria-hidden
      className={cn(
        "size-1.5 shrink-0 rounded-full bg-gold-400 transition-opacity duration-200 ease-out",
        on ? "opacity-100" : "opacity-0",
        className,
      )}
    />
  );
}

/** ", yeni" at the end of a news link's text, for screen readers only. */
export function NewLabel({ on }: { on: boolean }) {
  const { t } = useTranslation();
  return on ? <span className="sr-only">, {t("reminders.newsNew")}</span> : null;
}
