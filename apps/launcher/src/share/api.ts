// `shareApi` (SHARE): the app's share commands, or the browser preview's mock. The state itself
// comes with voice chat's (`VoiceState.share`); this is what the page asks and what it hears that
// voice chat does not carry (toasts, the watching page's channel).
import { isTauri } from "@/lib/platform";
import { mockShare } from "./mock";
import { tauriShare } from "./tauri";
import type {
  ShareBy,
  ShareLayer,
  ShareNotice,
  ShareRefusal,
  ShareSource,
  ShareSources,
  VoiceShareView,
  WatchCmd,
  WatchReport,
} from "./types";

/** Everything the page asks of screen sharing. Listeners return their stop function. */
export interface ShareApi {
  /** The picker's list (thumbnails made now). */
  sources(): Promise<ShareSources>;
  /** "Paylaş": null when it started. */
  start(source: ShareSource, label: string): Promise<ShareRefusal | null>;
  stop(n: number): Promise<void>;
  /** "İzle": null when asked for. */
  watch(person: string, n: number): Promise<ShareRefusal | null>;
  unwatch(person: string, n: number): Promise<void>;
  layer(person: string, n: number, layer: ShareLayer): Promise<void>;
  /** This page watches: the launcher's commands for its peer connection come here. */
  attach(onCommand: (cmd: WatchCmd) => void): Promise<void>;
  report(report: WatchReport): Promise<void>;
  /** This page went out of sight, or came back. */
  visible(visible: boolean): Promise<void>;
  /** "Evet" on "Hâlâ izliyor musun?". */
  still(): Promise<void>;
  /** "Ayrı pencerede aç", or back. */
  popout(open: boolean): Promise<void>;
  onTop(on: boolean): Promise<void>;
  fullscreen(on: boolean): Promise<void>;
  streamVolume(person: string, volume: number, muted: boolean): Promise<void>;
  /** "Kim paylaşabilir" (a friend room's leader); true when done. */
  setSharers(who: ShareBy): Promise<boolean>;
  /** A leader's or moderator's "Paylaşımı durdur" on someone else's share; true when done. */
  unshare(person: string, n: number): Promise<boolean>;
  onNotice(listener: (notice: ShareNotice) => void): () => void;
}

export const shareApi: ShareApi = isTauri() ? tauriShare : mockShare;

/** The view, or nothing of sharing (an older launcher, no room). */
export function shareOf(state: { share?: VoiceShareView }): VoiceShareView | null {
  return state.share?.on ? state.share : null;
}
