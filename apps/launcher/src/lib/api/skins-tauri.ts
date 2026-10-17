// Tauri implementation of `api.skins`: maps the generated bindings onto the UI shapes.
import { open } from "@tauri-apps/plugin-dialog";
import { type AppearanceView, type CmdError, commands, type SkinHistoryView } from "@/bindings";
import type { SkinAppearance, SkinHistory, SkinsApi } from "./skins-types";

/** A failed skin command: friendly `message`, plus `code`/`details` for "copy details". */
export class SkinCommandError extends Error {
  readonly code: string;
  readonly details: string;
  constructor(error: CmdError) {
    super(error.message);
    this.code = error.code;
    this.details = error.details;
  }
}

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function ok<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new SkinCommandError(result.error);
  return result.data;
}

function appearance(v: AppearanceView): SkinAppearance {
  return {
    accountId: v.accountId,
    username: v.username,
    skin: v.skin ? { url: v.skin.url, variant: v.skin.variant, isDefault: v.skin.isDefault } : null,
    capes: v.capes.map((c) => ({ id: c.id, name: c.name, url: c.url, active: c.active })),
    stale: v.stale,
  };
}

function history(v: SkinHistoryView): SkinHistory {
  return {
    entries: v.entries.map((e) => ({
      id: e.id,
      url: e.url,
      variant: e.variant,
      firstSeen: e.firstSeen,
      lastWorn: e.lastWorn,
      favorite: e.favorite,
      thumbUrl: e.thumbUrl ?? null,
    })),
    currentId: v.currentId ?? null,
  };
}

export const tauriSkins: SkinsApi = {
  async appearance(accountId) {
    return appearance(await ok(commands.skinAppearance(accountId)));
  },
  texture: (url) => ok(commands.skinTexture(url)),
  async pickFile() {
    const chosen = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Skin (PNG)", extensions: ["png"] }],
    });
    return typeof chosen === "string" ? chosen : null;
  },
  async readFile(path) {
    const file = await ok(commands.skinFilePreview(path));
    return {
      path: file.path,
      name: file.name,
      dataUrl: file.dataUrl,
      width: file.width,
      height: file.height,
    };
  },
  async upload(accountId, path, variant) {
    return appearance(await ok(commands.skinUpload(accountId, path, variant)));
  },
  async setVariant(accountId, variant) {
    return appearance(await ok(commands.skinSetVariant(accountId, variant)));
  },
  async setCape(accountId, capeId) {
    return appearance(await ok(commands.skinSetCape(accountId, capeId)));
  },
  async reset(accountId) {
    return appearance(await ok(commands.skinReset(accountId)));
  },
  async history(accountId) {
    return history(await ok(commands.skinHistory(accountId)));
  },
  prepareHistory: (accountId) => ok(commands.skinHistoryPrepare(accountId)),
  async wearFromHistory(accountId, entryId) {
    return appearance(await ok(commands.skinHistoryWear(accountId, entryId)));
  },
  async setHistoryFavorite(accountId, entryId, favorite) {
    await ok(commands.skinHistorySetFavorite(accountId, entryId, favorite));
  },
  async removeFromHistory(accountId, entryId) {
    await ok(commands.skinHistoryRemove(accountId, entryId));
  },
  async restoreToHistory(accountId, entryId) {
    await ok(commands.skinHistoryRestore(accountId, entryId));
  },
};
