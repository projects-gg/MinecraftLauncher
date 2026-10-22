// How a message reads in one line (the conversation list, a toast, a report's evidence): its text,
// or for a sticker (CHAT2 §8.4) "Çıkartma: GG" in the interface's language.
import { findSticker } from "@pl/stickers";

type T = (key: string, options?: Record<string, unknown>) => string;

/** A sticker's name in the language, or the plain word for one this build does not have. */
export function stickerName(ref: string, t: T, lang: string): string {
  const sticker = findSticker(ref);
  return sticker ? sticker.name[lang.startsWith("en") ? "en" : "tr"] : t("chat2.stickers.unknown");
}

/** The message's line: its text, or its sticker in words. */
export function messageLine(
  message: { text: string; sticker?: string },
  t: T,
  lang: string,
): string {
  if (!message.sticker) return message.text;
  return t("chat2.stickers.preview", { name: stickerName(message.sticker, t, lang) });
}
