// "Sınırlara git" (the owner, 28 Oct 2026: "harcama sınırı olduğu için soracak gibi bir şey var ya
// AFK+ sayfasında o uyarının yanına Sınırlara Git diye buton olsun"): beside a line that says AFK+
// asks, or asked, or waits because of a limit, a quiet action to that limit in "Ayarlar" (its field
// in view and focused). Never gold: the gold beside it is the card's or the panel's own action.
import { cn } from "@pl/ui";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { QuietLink } from "./fields";

/** A line, with "Sınırlara git" after its words while `onGo` is given (under them when long). */
export function LimitLine({
  onGo,
  className,
  children,
}: {
  /** Goes to the limit the line is about; null: the line alone. */
  onGo: (() => void) | null;
  className?: string;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  if (onGo === null) return <p className={className}>{children}</p>;
  return (
    <p className={cn("flex flex-wrap items-baseline gap-x-2 gap-y-0.5", className)}>
      <span className="min-w-0">{children}</span>
      <QuietLink tone="bone" onClick={onGo}>
        {t("afkPlus.settings.goToLimits")}
      </QuietLink>
    </p>
  );
}
