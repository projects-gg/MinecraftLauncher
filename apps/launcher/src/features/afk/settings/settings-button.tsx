// The settings button of an AFK card, desktop or 7/24 Bulut (SHEET-UX): a small gold dot while the
// account's panel was closed with edits in it (they wait in `drafts.ts`), as a tab holding a draft
// carries one. The dot fades; nothing moves.
import { GearSix } from "@phosphor-icons/react";
import { cn, IconButton } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { useHasDraft } from "./drafts";

export function SettingsButton({
  kind,
  id,
  label,
  onClick,
}: {
  kind: "desktop" | "cloud";
  id: string;
  label: string;
  onClick: () => void;
}) {
  const { t } = useTranslation();
  const unsaved = useHasDraft(kind, id);
  return (
    <span className="relative inline-flex">
      <IconButton
        label={unsaved ? t("afkPanel.settingsUnsaved") : label}
        icon={<GearSix size={17} />}
        size="sm"
        onClick={onClick}
      />
      <span
        aria-hidden
        className={cn(
          "pointer-events-none absolute top-0.5 right-0.5 size-1.5 rounded-full bg-gold-400 transition-opacity duration-150",
          unsaved ? "opacity-100" : "opacity-0",
        )}
      />
    </span>
  );
}
