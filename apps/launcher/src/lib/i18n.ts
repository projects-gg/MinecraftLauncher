import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { en } from "@/locales/en";
import { tr } from "@/locales/tr";

/**
 * English (`locales/en.ts`) is translated but hidden for now: the language picker is not shown
 * and the launcher always runs in Turkish. Set to `true` to offer English in Settings → Genel
 * once dates and numbers follow the chosen language (several formatters still use "tr-TR").
 */
export const ENGLISH_ENABLED = false;

void i18n.use(initReactI18next).init({
  resources: { tr: { translation: tr }, en: { translation: en } },
  lng: "tr",
  fallbackLng: "tr",
  interpolation: { escapeValue: false },
  returnNull: false,
});

export function setLanguage(lang: "tr" | "en") {
  const next = ENGLISH_ENABLED ? lang : "tr";
  void i18n.changeLanguage(next);
  document.documentElement.lang = next;
}

export default i18n;
