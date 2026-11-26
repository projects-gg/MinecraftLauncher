import type { trCustomVer } from "./tr-custom-ver";

// CUSTOM-VER: the English side of tr-custom-ver.ts (same keys).
export const enCustomVer: typeof trCustomVer = {
  filter: "Custom",
  base: "Minecraft {{minecraft}} with {{loader}}.",
  baseVanilla: "Minecraft {{minecraft}}.",
  empty: "No custom versions yet.",
  failed: "Couldn't read the custom versions.",
  add: {
    title: "How to Add a Version",
    body: "Open OptiFine or Forge and paste the data folder's path as the folder. The version you install shows up on the left.",
    copy: "Copy Path",
    copied: "Folder path copied.",
    open: "Open Folder",
  },
  fact: "Custom Version",
};
