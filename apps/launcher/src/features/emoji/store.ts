// The picker's memory on this computer (CHAT2 §3.1): the recent emoji and stickers and the skin
// tone. Not in the settings backup: the recents change with every message.
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { cleanList, RECENT_EMOJI, RECENT_STICKERS, remember } from "./recents";
import { bareEmoji, isSkinTone, type SkinTone } from "./tone";

interface EmojiPrefsState {
  /** Bare emoji (no skin tone), newest first. */
  recentEmoji: string[];
  /** Sticker references ("pl1/gg"), newest first. */
  recentStickers: string[];
  tone: SkinTone;
  /** An emoji went into a message (or a reaction). */
  usedEmoji: (emoji: string) => void;
  usedSticker: (ref: string) => void;
  setTone: (tone: SkinTone) => void;
}

export const useEmojiPrefs = create<EmojiPrefsState>()(
  persist(
    (set, get) => ({
      recentEmoji: [],
      recentStickers: [],
      tone: 0,
      usedEmoji: (emoji) =>
        set({ recentEmoji: remember(get().recentEmoji, bareEmoji(emoji), RECENT_EMOJI) }),
      usedSticker: (ref) =>
        set({ recentStickers: remember(get().recentStickers, ref, RECENT_STICKERS) }),
      setTone: (tone) => set({ tone }),
    }),
    {
      name: "pl.emoji",
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: ({ recentEmoji, recentStickers, tone }) => ({
        recentEmoji,
        recentStickers,
        tone,
      }),
      // What was stored is checked, never trusted: a broken entry falls back to the default.
      merge: (stored, current) => {
        const value = (stored ?? {}) as Record<string, unknown>;
        return {
          ...current,
          recentEmoji: cleanList(value.recentEmoji, RECENT_EMOJI),
          recentStickers: cleanList(value.recentStickers, RECENT_STICKERS),
          tone: isSkinTone(value.tone) ? value.tone : 0,
        };
      },
    },
  ),
);
