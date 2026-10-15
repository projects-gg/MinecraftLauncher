import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { en } from "@/locales/en";
import { tr } from "@/locales/tr";

void i18n.use(initReactI18next).init({
  resources: { tr: { translation: tr }, en: { translation: en } },
  lng: "tr",
  fallbackLng: "tr",
  interpolation: { escapeValue: false },
  returnNull: false,
});

export function setLanguage(lang: "tr" | "en") {
  void i18n.changeLanguage(lang);
  document.documentElement.lang = lang;
}

export default i18n;
