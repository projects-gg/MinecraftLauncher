import heroArt from "@/assets/art/hero-dragon-1.webp";
import i18n from "@/lib/i18n";
import type { HomeContent } from "./types";

/**
 * Home before anything is published or cached (a first start while the content servers can't be
 * reached): the bundled art and a greeting, with no news, events or banners; the news column says
 * "Henüz haber yok.". Never the preview's sample content: players would read its season, double XP
 * weekend and tournament countdown as real announcements.
 */
export function welcomeHome(): HomeContent {
  return {
    hero: {
      imageUrl: heroArt,
      title: i18n.t("home.welcomeTitle"),
      subtitle: i18n.t("home.welcomeSubtitle"),
      particles: "embers",
    },
    banners: [],
    events: [],
    news: [],
    offline: false,
  };
}
