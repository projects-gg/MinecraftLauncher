// The chat's text size as this computer keeps it (CHAT-PROD; text-zoom.ts has the steps). A store
// of its own: a step of Ctrl + wheel wakes only what draws the size (the conversation's zoom, the
// field, the plate, "Yazı boyutu"), never every friends menu that reads the other friend
// preferences. Not in the settings backup: the size belongs to this screen and these eyes.
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { DEFAULT_TEXT_SIZE, type TextSize, textSizeOf } from "./text-zoom";

interface ChatTextSize {
  size: TextSize;
  setSize: (size: number) => void;
}

export const useChatTextSize = create<ChatTextSize>()(
  persist(
    (set) => ({
      size: DEFAULT_TEXT_SIZE,
      setSize: (size) => set({ size: textSizeOf(size) }),
    }),
    {
      name: "pl.chat-text",
      version: 1,
      // A hand-edited or damaged value reads as the nearest step.
      merge: (persisted, current) => ({
        ...current,
        size: textSizeOf((persisted as Partial<ChatTextSize> | undefined)?.size),
      }),
    },
  ),
);
