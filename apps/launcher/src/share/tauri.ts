// The app's side of `shareApi` (SHARE): the launcher's `share_*` commands and its `share-notice`
// event. Called by name (the "Yayın" window has no generated bindings of its own to load, and the
// commands answer plain data); a refusal is an answer, a failed command rejects.
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ShareApi } from "./api";
import type { ShareNotice, ShareRefusal, ShareSources, ShareThumb, WatchCmd } from "./types";

const quiet = () => {
  // The launcher closed meanwhile, or the page is going away.
};

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
  onNotice: (listener) => {
    let stop: (() => void) | null = null;
    let gone = false;
    void listen<ShareNotice>("share-notice", (event) => listener(event.payload)).then(
      (unlisten) => {
        if (gone) unlisten();
        else stop = unlisten;
      },
    );
    return () => {
      gone = true;
      stop?.();
    };
  },
};
