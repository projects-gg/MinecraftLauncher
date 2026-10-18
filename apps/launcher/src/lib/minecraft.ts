// Minecraft's own updates from the news feed (docs/content-format.md → "Minecraft news"): calm
// filler that only takes places Projects news leaves empty, never "new", in the interface's
// language when the bot wrote an English version.
import type { MinecraftNewsItem } from "@/lib/api";
import type { UiLanguage } from "@/lib/i18n";

export interface MinecraftText {
  title: string;
  summary: string;
  body: string;
}

/** The texts to show: English ones in an English interface (a missing part falls back). */
export function minecraftText(item: MinecraftNewsItem, language: UiLanguage): MinecraftText {
  const en = language === "en" ? item.en : undefined;
  return {
    title: en?.title ?? item.title,
    summary: en?.summary ?? item.summary,
    body: en?.body ?? item.body,
  };
}

/** Entries still shown at `now`: one that expired while the launcher was open just leaves. */
export function liveMinecraft(items: MinecraftNewsItem[] | undefined, now: number) {
  return (items ?? []).filter((item) => Date.parse(item.expiresAt) > now);
}

/** Rows the home column has. */
export const NEWS_COLUMN_ROWS = 4;

/** The Minecraft entries that fill the news column's rows Projects news leaves empty. */
export function columnFill<T>(projectsRows: number, minecraft: T[], rows = NEWS_COLUMN_ROWS): T[] {
  return minecraft.slice(0, Math.max(0, rows - projectsRows));
}
