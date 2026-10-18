import { Cube } from "@phosphor-icons/react";
import { Chip, cn } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { MinecraftNewsItem } from "@/lib/api";
import { type MinecraftText, minecraftText } from "@/lib/minecraft";

/**
 * The source mark of a Minecraft update, in place of a news category: quieter than the category
 * chips (dimmer fill and text, a small block glyph, never Mojang's logo), so the entry reads as
 * filler from elsewhere, not as Projects news.
 */
export function MinecraftChip({ className }: { className?: string }) {
  const { t } = useTranslation();
  return (
    <Chip className={cn("bg-white/7 text-ash-300", className)}>
      <Cube size={12} aria-hidden className="shrink-0" />
      {t("minecraft.label")}
    </Chip>
  );
}

/** The entry's texts in the interface's language, following a live language change. */
export function useMinecraftText(item: MinecraftNewsItem): MinecraftText {
  const { i18n } = useTranslation();
  return minecraftText(item, i18n.language === "en" ? "en" : "tr");
}
