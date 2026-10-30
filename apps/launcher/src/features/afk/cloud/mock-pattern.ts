// "ProBot AI ile kalıp kur" for browser previews (the service's "Pattern helper"): a plain phrase of
// one to four words answers at once as the service's does (no model); a description gets a pattern
// from its own words. `&ai=off` hides the helper (the overview's `aiPatterns`); `&ai=busy`,
// `&ai=unclear` or `&ai=down` make it refuse that way, in the launcher command's words.
import { CommandError } from "@/lib/api/tauri";
import i18n from "@/lib/i18n";
import type { PatternSuggestion } from "./api";

/** Words that describe messages rather than quote one (the service keeps its own longer list). */
const DESCRIBING = new Set([
  "biri",
  "birisi",
  "bir",
  "her",
  "ya",
  "da",
  "de",
  "veya",
  "yada",
  "ile",
  "ve",
  "mesaj",
  "mesajda",
  "when",
  "someone",
  "anyone",
  "or",
  "and",
  "if",
  "any",
  "a",
  "the",
  "message",
]);
/** Beginnings of describing words (Turkish suffixes vary: yazınca, yazarsa, yazan). */
const DESCRIBING_STEMS = ["yaz", "geç", "söyl", "sor", "iste", "write", "say", "ask", "type"];

function describing(word: string): boolean {
  const lower = word.toLocaleLowerCase("tr");
  return DESCRIBING.has(lower) || DESCRIBING_STEMS.some((stem) => lower.startsWith(stem));
}

function param(key: string): string | null {
  try {
    return new URLSearchParams(window.location.search).get(key);
  } catch {
    return null;
  }
}

/** The overview's `aiPatterns` in previews. */
export function mockAiPatterns(): boolean {
  return param("ai") !== "off";
}

const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

function refuse(code: string, tr: string, en: string): never {
  const english = i18n.language.startsWith("en");
  throw new CommandError({ code, message: english ? en : tr, details: `mock ${code}` });
}

export async function mockSuggestPattern(text: string, fresh: boolean): Promise<PatternSuggestion> {
  const english = i18n.language.startsWith("en");
  const words = text.split(" ").filter(Boolean);
  const failure = param("ai");
  if (words.length <= 4 && text.length <= 40 && !words.some(describing) && failure !== "unclear") {
    await delay(220);
    return {
      regex: false,
      match: text,
      summary: english
        ? `Replies when a message has "${text}" in it.`
        : `Mesajda "${text}" geçerse yanıt verir.`,
      matches: [text, text.charAt(0).toLocaleUpperCase("tr") + text.slice(1)],
      misses: [english ? "hello everyone" : "herkese selam"],
      source: "plain",
    };
  }
  await delay(fresh ? 1100 : 900);
  if (failure === "busy") {
    refuse(
      "BULUT-YZ-YOGUN",
      "ProBot AI şu an çok yoğun. Biraz sonra tekrar dene.",
      "ProBot AI is busy right now. Try again in a moment.",
    );
  }
  if (failure === "down") {
    refuse(
      "BULUT-YZ-KAPALI",
      "ProBot AI şu an yardım edemiyor. Düz bir söz yazabilir ya da kalıbı kendin yazabilirsin.",
      "ProBot AI can't help right now. Use a plain phrase or write the pattern yourself.",
    );
  }
  const quoted = words.filter((word) => word.length >= 2 && !describing(word)).slice(0, 2);
  const [first, second] = quoted;
  if (failure === "unclear" || first === undefined) {
    refuse(
      "BULUT-KALIP",
      "ProBot AI bundan bir kalıp çıkaramadı. Başka sözlerle anlatmayı dene.",
      "ProBot AI couldn't make a pattern from that. Try saying it another way.",
    );
  }
  const alternatives = second === undefined ? escape(first) : `(${escape(first)}|${escape(second)})`;
  const named = second === undefined ? `"${first}"` : `"${first}"${english ? " or " : " ya da "}"${second}"`;
  return {
    regex: true,
    match: `(?i)\\b${alternatives}\\b`,
    summary: english
      ? `Replies when a message has ${named} as a word of its own.`
      : `Mesajda ${named} ayrı bir söz olarak geçerse yanıt verir.`,
    matches: english
      ? [`${first} please`, ...(second === undefined ? [] : [`can you ${second}`])]
      : [`${first} atar mısın`, ...(second === undefined ? [] : [`beni ${second}`])],
    misses: [english ? `${first}x is here` : `${first}x geldim`],
    source: "ai",
  };
}
