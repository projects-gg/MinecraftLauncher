// Emoji and stickers in chat (CHAT2, docs: chat2.md §3, §8). This module is light: the picker's
// panel, its data and the embedded emoji font load on the picker's first use.
export { bigEmojiCount, codePoints, insertText } from "./insert";
export { EmojiPickerButton, preloadEmojiPicker } from "./picker-button";
export type { PickerTab } from "./picker-layout";
export { RECENT_EMOJI, RECENT_STICKERS } from "./recents";
export { Sticker, UnknownSticker } from "./sticker";
export { useEmojiPrefs } from "./store";
export { type SkinTone, withTone } from "./tone";
