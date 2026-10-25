import { Cube } from "@phosphor-icons/react";
import { cn } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { MinecraftNewsItem } from "@/lib/api";
import { type MinecraftText, minecraftText } from "@/lib/minecraft";

/**
 * The source of a Minecraft update, in place of a news category: a small block glyph and the word,
 * in the meta line's own size and colour (never Mojang's logo), so the entry reads as filler from
 * elsewhere, not as Projects news.
 */
export function MinecraftLabel({ className }: { className?: string }) {
  const { t } = useTranslation();
  return (
    <span className={cn("inline-flex shrink-0 items-center gap-1 font-medium", className)}>
      <Cube size={12} aria-hidden className="shrink-0 opacity-80" />
      {t("minecraft.label")}
    </span>
  );
}

/** The entry's texts in the interface's language, following a live language change. */
export function useMinecraftText(item: MinecraftNewsItem): MinecraftText {
  const { i18n } = useTranslation();
  return minecraftText(item, i18n.language === "en" ? "en" : "tr");
}
