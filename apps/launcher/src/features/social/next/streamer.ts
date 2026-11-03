// SOC-NEXT §3.4: "Yayıncı modu". Off, on, or while a streaming program runs (the default: OBS
// Studio, Streamlabs, XSplit, Twitch Studio; looked for when the launcher opens, when its window
// shows and when a game starts, never on a timer). While it is on, what a stream must not show
// hides: the player's friend code, its QR and link, crews' codes and invite links, the server
// addresses in presence lines and calls ("Başka sunucuda"), and the words of messages in
// notifications (the sender and "Yeni mesaj" stay). A hidden value shows "••••" and "Göster"
// (ten seconds). The title bar's mark says it is on; a press turns it off (this session, when
// it came by itself).
import { create } from "zustand";
import { socNextOn, useSocNext } from "./flag";
import { type StreamerMode, useSocNextPrefs } from "./prefs";

/** A shown value hides again after this long. */
export const REVEAL_MS = 10_000;
/** What stands for a hidden value. */
export const HIDDEN = "••••";

/** On for this mode, with or without a streaming program (`dismissed`: the player turned the
 * automatic one off this session). */
export function streamerActive(mode: StreamerMode, detected: boolean, dismissed = false): boolean {
  if (mode === "on") return true;
  return mode === "auto" && detected && !dismissed;
}

interface StreamerState {
  /** A streaming program ran at the last look. */
  detected: boolean;
  /** The automatic one was turned off from the title bar (until the launcher starts again). */
  dismissed: boolean;
  setDetected: (detected: boolean) => void;
  dismiss: () => void;
}

export const useStreamer = create<StreamerState>()((set) => ({
  detected: false,
  dismissed: false,
  setDetected: (detected) => set((s) => (s.detected === detected ? s : { detected })),
  dismiss: () => set({ dismissed: true }),
}));

/** Streamer mode is on now (React). */
export function useStreamerOn(): boolean {
  const mode = useSocNextPrefs((s) => s.streamer);
  const detected = useStreamer((s) => s.detected);
  const dismissed = useStreamer((s) => s.dismissed);
  const on = useSocNext();
  return on && streamerActive(mode, detected, dismissed);
}

/** Streamer mode is on now (outside React: notifications, presence lines). */
export function streamerOnNow(): boolean {
  if (!socNextOn()) return false;
  const { detected, dismissed } = useStreamer.getState();
  return streamerActive(useSocNextPrefs.getState().streamer, detected, dismissed);
}

/** A server's address as a line may show it now: hidden (undefined) under streamer mode. */
export function shownHost(host: string | undefined): string | undefined {
  return host && streamerOnNow() ? undefined : host;
}
