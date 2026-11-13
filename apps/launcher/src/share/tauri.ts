// The app's side of `shareApi` (SHARE): the launcher's `share_*` commands and its `share-notice`
// event. Called by name (the "Yayın" window has no generated bindings of its own to load, and the
// commands answer plain data); a refusal is an answer, a failed command rejects.
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@/lib/api/listen";
import type { ShareApi } from "./api";
import type {
  ShareNotice,
  SharePreviewFrame,
  ShareRefusal,
  ShareSources,
  ShareStats,
  ShareThumb,
  WatchCmd,
} from "./types";

const quiet = () => {
  // The launcher closed meanwhile, or the page is going away.
};

/** One of the launcher's events for as long as the returned stop is not called. */
function subscribe<T>(event: string, listener: (payload: T) => void): () => void {
  let stop: (() => void) | null = null;
  let gone = false;
  void listen<T>(event, (e) => listener(e.payload)).then((unlisten) => {
    if (gone) unlisten();
    else stop = unlisten;
  });
  return () => {
    gone = true;
    stop?.();
  };
}

export const tauriShare: ShareApi = {
  sources: () => invoke<ShareSources>("share_sources"),
  start: (source, label) => invoke<ShareRefusal | null>("share_start", { source, label }),
  stop: (n) => invoke("share_stop", { n }),
  watch: (person, n) => invoke<ShareRefusal | null>("share_watch", { person, n }),
  unwatch: (person, n) => invoke("share_unwatch", { person, n }),
  layer: (person, n, layer) => invoke("share_watch_layer", { person, n, layer }),
  attach: async (onCommand) => {
    const channel = new Channel<WatchCmd>();
    channel.onmessage = onCommand;
    await invoke("share_watch_attach", { onCommand: channel });
  },
  report: (report) => invoke<null>("share_watch_report", { report }).then(quiet, quiet),
  visible: (visible) => invoke<null>("share_watch_visible", { visible }).then(quiet, quiet),
  still: () => invoke("share_watch_still"),
  popout: (open) => invoke("share_popout", { open }),
  onTop: (on) => invoke("share_popout_on_top", { on }),
  fullscreen: (on) => invoke("share_fullscreen", { on }),
  streamVolume: (person, volume, muted) => invoke("share_stream_volume", { person, volume, muted }),
  setSharers: async (who) => {
    const answer = await invoke<{ refused: string | null }>("share_set_sharers", { who });
    return answer.refused === null;
  },
  unshare: async (person, n) => {
    const answer = await invoke<{ refused: string | null }>("share_unshare", { person, n });
    return answer.refused === null;
  },
  // SHARE-LIVE: the launcher asks the friends service (the page never sees its token) and keeps
  // an answer 20 s; a launcher without the command answers nothing. Someone outside the player's
  // room has one share as far as the lists know: the first (LIVE-R's `share_thumb` takes `n` 1).
  thumb: (room, user, n) =>
    invoke<ShareThumb>("share_thumb", { room, user, n: n ?? 1 }).catch(
      (): ShareThumb => ({ kind: "none" }),
    ),
  onNotice: (listener) => subscribe<ShareNotice>("share-notice", listener),
  // SQOL (sqol.md §3). A command that answers nothing (`()`) resolves null: done.
  switchSource: (n, source, label) =>
    invoke<ShareRefusal | null>("share_switch", { n, source, label }).then((r) => r ?? null),
  update: (n, change) =>
    invoke<ShareRefusal | null>("share_update", { n, change }).then((r) => r ?? null),
  hold: (n, on) => invoke<ShareRefusal | null>("share_hold", { n, on }).then((r) => r ?? null),
  // Only a picture: a launcher that cannot make one leaves the card on the room's last one.
  preview: (n, on) => invoke<null>("share_preview", { n, on }).then(quiet, quiet),
  onPreview: (listener) => subscribe<SharePreviewFrame>("share-preview", listener),
  onStats: (listener) => subscribe<ShareStats>("share-stats", listener),
};
