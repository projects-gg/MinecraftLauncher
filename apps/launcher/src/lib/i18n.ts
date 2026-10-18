import i18n from "i18next";
import { flushSync } from "react-dom";
import { initReactI18next } from "react-i18next";
import { tr } from "@/locales/tr";

/** A language of the launcher's texts. */
export type UiLanguage = "tr" | "en";

/** Each language's name in its own language (the picker never translates them). */
export const LANGUAGE_NAMES: Record<UiLanguage, string> = { tr: "Türkçe", en: "English" };

/**
 * Settings → Genel → Dil: "Otomatik" (the default) or the player's own pick. The owner's rule
 * (18 Oct 2026): projects.gg is a Turkish server and English is there for players from abroad,
 * so the language is the player's pick; else Turkish in Turkey; else Turkish on a Turkish system;
 * else English. The backend applies the same rule (`pl_lang`).
 */
export type LanguageChoice = "auto" | UiLanguage;

/** What the computer says about the player (`pl_lang::Signals`). */
export interface LanguageSignals {
  /** The system's display language, a locale tag. */
  language?: string | null;
  /** The device's IANA time zone. */
  timeZone?: string | null;
  /** A home country the system names (Windows' home location, only when it is Turkey). */
  region?: string | null;
}

declare global {
  interface Window {
    /** Set by the backend before the page's first script runs (`language_plugin`). */
    __PL_SYSTEM__?: LanguageSignals;
  }
}

void i18n.use(initReactI18next).init({
  resources: { tr: { translation: tr } },
  lng: "tr",
  fallbackLng: "tr",
  interpolation: { escapeValue: false },
  returnNull: false,
});

/**
 * Time zones that count as Turkey: `Europe/Istanbul` with its old names, and `Asia/Famagusta`,
 * Northern Cyprus, where Turkish is the language people speak.
 */
const TURKEY_TIME_ZONES = ["europe/istanbul", "asia/istanbul", "turkey", "asia/famagusta"];

/** Turkish for a Turkish locale tag ("tr", "tr-TR", "tr_TR.UTF-8"), English for any other. */
export function languageOfTag(tag: string | null | undefined): UiLanguage {
  return tag
    ?.trim()
    .toLowerCase()
    .split(/[-_.@]/)[0] === "tr"
    ? "tr"
    : "en";
}

/** A Turkish time zone, or Turkey as Windows' home location. No network is asked. */
export function inTurkey(signals: LanguageSignals): boolean {
  const zone = signals.timeZone?.trim().toLowerCase() ?? "";
  return TURKEY_TIME_ZONES.includes(zone) || signals.region?.trim().toUpperCase() === "TR";
}

/** "Otomatik": Turkish in Turkey or on a Turkish system, English otherwise. */
export function automaticLanguage(signals: LanguageSignals = systemSignals()): UiLanguage {
  return inTurkey(signals) ? "tr" : languageOfTag(signals.language);
}

export function resolveLanguage(
  choice: LanguageChoice,
  signals: LanguageSignals = systemSignals(),
): UiLanguage {
  return choice === "auto" ? automaticLanguage(signals) : choice;
}

/**
 * This computer's signals. In the app the backend hands them over before any script runs (the
 * OS display language: WebView2's own language can differ from it); what it could not read, and
 * everything in a plain browser (previews, tests), comes from the page itself.
 */
export function systemSignals(): LanguageSignals {
  const fromBackend = typeof window === "undefined" ? undefined : window.__PL_SYSTEM__;
  let browserZone: string | undefined;
  try {
    browserZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  } catch {
    browserZone = undefined;
  }
  const browserLanguage =
    typeof navigator === "undefined" ? undefined : (navigator.languages?.[0] ?? navigator.language);
  return {
    language: fromBackend?.language || browserLanguage,
    timeZone: fromBackend?.timeZone || browserZone,
    region: fromBackend?.region ?? null,
  };
}

/** The language the texts are in now. */
export function currentLanguage(): UiLanguage {
  return i18n.language === "en" ? "en" : "tr";
}

/** English loads only when it is needed (most players read Turkish); missing keys fall back to Turkish. */
export async function loadEnglish() {
  if (i18n.hasResourceBundle("en", "translation")) return;
  const { en } = await import("@/locales/en");
  i18n.addResourceBundle("en", "translation", en);
}

/** `<html lang>`: CSS upper case ("i" → "İ" only in Turkish), hyphenation, screen readers. */
function applyDocumentLanguage(lang: UiLanguage) {
  if (typeof document !== "undefined") document.documentElement.lang = lang;
}

/**
 * Before the first render: the language with its texts loaded, so the first paint is already in
 * it (the window stays hidden until then). Turkish needs nothing loaded.
 */
export async function startLanguage(choice: LanguageChoice): Promise<UiLanguage> {
  const lang = resolveLanguage(choice);
  if (lang === "en") await loadEnglish();
  await i18n.changeLanguage(lang);
  applyDocumentLanguage(lang);
  return lang;
}

type StartTransition = (options: { update: () => void | Promise<void>; types?: string[] }) => {
  finished: Promise<void>;
  ready: Promise<void>;
};

/** The newest `setLanguage` call: a quicker second pick wins over a first one still loading. */
let latestSwitch = 0;

/**
 * Settings → Genel → Dil: switches live, without a reload. The window cross-fades to the new
 * texts (a view transition of type "language", styles.css) so the words do not snap; engines
 * without view transition types switch in place.
 */
export async function setLanguage(choice: LanguageChoice): Promise<UiLanguage> {
  const request = ++latestSwitch;
  const lang = resolveLanguage(choice);
  if (lang === "en") await loadEnglish();
  if (request !== latestSwitch) return currentLanguage();
  if (lang === currentLanguage()) return lang;
  const apply = () => {
    // Synchronously, so the transition's new picture already has every word in the new language.
    flushSync(() => {
      void i18n.changeLanguage(lang);
      applyDocumentLanguage(lang);
    });
  };
  const start =
    typeof document !== "undefined" &&
    typeof document.startViewTransition === "function" &&
    globalThis.CSS?.supports?.("selector(:active-view-transition-type(a))")
      ? (document.startViewTransition.bind(document) as unknown as StartTransition)
      : null;
  if (start) {
    const transition = start({ update: apply, types: ["language"] });
    transition.ready.catch(() => {});
    await transition.finished.catch(() => {});
  } else {
    apply();
  }
  return lang;
}

export default i18n;
