import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { tr } from "@/locales/tr";

/**
 * English (`locales/en.ts`) is translated but hidden for now: the language picker is not shown
 * and the launcher always runs in Turkish. Set to `true` to offer English in Settings → Genel
 * once dates and numbers follow the chosen language (several formatters still use "tr-TR").
 */
export const ENGLISH_ENABLED = false;

void i18n.use(initReactI18next).init({
  resources: { tr: { translation: tr } },
  lng: "tr",
  fallbackLng: "tr",
  interpolation: { escapeValue: false },
  returnNull: false,
});

/** English loads only when chosen (most players never do); missing keys fall back to Turkish. */
export async function loadEnglish() {
  if (i18n.hasResourceBundle("en", "translation")) return;
  const { en } = await import("@/locales/en");
  i18n.addResourceBundle("en", "translation", en);
}

export function setLanguage(lang: "tr" | "en") {
  const next = ENGLISH_ENABLED ? lang : "tr";
  document.documentElement.lang = next;
  if (next === "en") {
    void loadEnglish().then(() => i18n.changeLanguage("en"));
  } else {
    void i18n.changeLanguage(next);
  }
}

export default i18n;
