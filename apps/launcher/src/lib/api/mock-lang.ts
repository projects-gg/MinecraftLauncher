import { currentLanguage } from "@/lib/i18n";

/**
 * The Turkish or the English text of a pair, like the backend's `pl_lang::pick`: for the mocks'
 * stand-ins of texts the backend words in the player's language (command errors and the like).
 * The interface's language is read when the text is made, so a live language switch applies.
 */
export function pick(tr: string, en: string): string {
  return currentLanguage() === "en" ? en : tr;
}
