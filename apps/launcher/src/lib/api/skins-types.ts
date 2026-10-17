// Skins and capes of Microsoft accounts (Settings → Hesaplar → Görünüm). UI-facing shapes; the
// Tauri adapter (skins-tauri.ts) maps the generated bindings onto them.

/** Arm model of a skin: classic (4 px arms) or slim (3 px arms). */
export type SkinVariant = "classic" | "slim";

export interface ActiveSkin {
  /** Texture URL (textures.minecraft.net in the app). */
  url: string;
  variant: SkinVariant;
  /** One of Minecraft's default skins rather than an upload. */
  isDefault: boolean;
}

export interface CapeOption {
  id: string;
  /** Mojang's name for it, e.g. "Migrator". */
  name: string;
  /** Texture URL, usable in `<img>` and CSS backgrounds (not in WebGL: use `texture`). */
  url: string;
  active: boolean;
}

export interface SkinAppearance {
  accountId: string;
  username: string;
  skin: ActiveSkin | null;
  capes: CapeOption[];
  /** Minecraft's servers could not be reached: the saved copy, possibly out of date. */
  stale: boolean;
}

/** A skin PNG the player picked or dropped, checked and ready for the preview. */
export interface SkinFile {
  /** Handed back to `upload` (a file path in the app). */
  path: string;
  name: string;
  /** PNG data URL for the instant preview. */
  dataUrl: string;
  width: number;
  height: number;
}

export type SkinFileDropEvent =
  | { type: "over" }
  | { type: "leave" }
  | { type: "drop"; paths: string[] };

/** A skin the account wore while the launcher knew it (kept on this computer only). */
export interface SkinHistoryEntry {
  /** Key for the history calls; never shown. */
  id: string;
  /** Texture URL for the big preview (`texture` loads it). */
  url: string;
  /** The arm model it was last worn with, and is worn with again. */
  variant: SkinVariant;
  /** When the launcher first saw it (ISO). */
  firstSeen: string;
  /** The last time it was seen worn (ISO). */
  lastWorn: string;
  favorite: boolean;
  /** The whole player from the front (a small PNG), once drawn; `prepareHistory` makes it. */
  thumbUrl: string | null;
}

export interface SkinHistory {
  /** Favourites first, then the most recently worn. */
  entries: SkinHistoryEntry[];
  /** `id` of the skin the account wears now, when the history has it. */
  currentId: string | null;
}

/** `api.skins`: everything the appearance panel asks of the backend. */
export interface SkinsApi {
  /** Fresh from Minecraft's servers, or the saved copy (`stale`) when they are unreachable. */
  appearance(accountId: string): Promise<SkinAppearance>;
  /** A skin or cape texture as a data URL the 3D preview can use. */
  texture(url: string): Promise<string>;
  /** Native file picker for a PNG; `null` when the player closes it. */
  pickFile(): Promise<string | null>;
  /** Reads and checks a skin file (64×64 or 64×32 PNG) for the preview. */
  readFile(path: string): Promise<SkinFile>;
  upload(accountId: string, path: string, variant: SkinVariant): Promise<SkinAppearance>;
  /** Changes the arm model of the current skin, keeping its texture. */
  setVariant(accountId: string, variant: SkinVariant): Promise<SkinAppearance>;
  /** Shows an owned cape, or hides the cape with `null`. */
  setCape(accountId: string, capeId: string | null): Promise<SkinAppearance>;
  /** Back to Minecraft's default skin. */
  reset(accountId: string): Promise<SkinAppearance>;
  /** Files dragged onto the window. Returns an unsubscribe function. */
  onFileDrop(listener: (event: SkinFileDropEvent) => void): () => void;

  /** The skins the account wore ("Önceki skinler"). */
  history(accountId: string): Promise<SkinHistory>;
  /** Fetches and draws the pictures still missing; resolves with how many were made. */
  prepareHistory(accountId: string): Promise<number>;
  /** Puts a skin of the history back on. */
  wearFromHistory(accountId: string, entryId: string): Promise<SkinAppearance>;
  setHistoryFavorite(accountId: string, entryId: string, favorite: boolean): Promise<void>;
  /** Removes a skin from the history; `restoreToHistory` brings it back for a few minutes. */
  removeFromHistory(accountId: string, entryId: string): Promise<void>;
  restoreToHistory(accountId: string, entryId: string): Promise<void>;
}
