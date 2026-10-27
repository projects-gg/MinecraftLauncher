import { Lightning, Mountains, Scales, SlidersHorizontal } from "@phosphor-icons/react";
import { Segmented } from "@pl/ui";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { type GraphicsChoice, graphicsChoices } from "./settings-form";

// Güzel is the far, full view (Settings → Grafik ve FPS draws the same mountains); no sparkle
// imagery (projects-taste).
const graphicsIcons: Record<GraphicsChoice, ReactNode> = {
  smooth: <Lightning size={15} />,
  balanced: <Scales size={15} />,
  quality: <Mountains size={15} />,
  custom: <SlidersHorizontal size={15} />,
};

/**
 * The graphics preset control: Akıcı, Dengeli, Güzel, and "Özel" while the file has values of its
 * own (`saved`). The instance settings tab (a draft, saved from its bar) and Settings → Grafik ve
 * FPS (applied at once) draw this same one.
 */
export function GraphicsPicker({
  saved,
  value,
  onValueChange,
  disabled,
  describedBy,
  className,
}: {
  /** What the file has: decides whether "Özel" is offered. */
  saved: GraphicsChoice;
  /** What the control shows chosen. */
  value: GraphicsChoice;
  onValueChange: (value: GraphicsChoice) => void;
  disabled?: boolean;
  describedBy?: string;
  className?: string;
}) {
  const { t } = useTranslation();
  return (
    <Segmented<GraphicsChoice>
      className={className}
      value={value}
      onValueChange={onValueChange}
      label={t("graphics.title")}
      disabled={disabled}
      describedBy={describedBy}
      options={graphicsChoices(saved).map((choice) => ({
        value: choice,
        label: choice === "custom" ? t("graphics.custom") : t(`graphics.preset.${choice}`),
        icon: graphicsIcons[choice],
      }))}
    />
  );
}
