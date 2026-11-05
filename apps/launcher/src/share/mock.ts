// The preview's screen sharing (SHARE): the share commands change the voice mock's state as the
// launcher would (a share goes live after a moment, "İzle" puts the share on the stage), and the
// picker gets a made-up list. Nothing leaves the page; there are no streams (the stage shows a
// still picture, `mockFrame`).
import { mockVoice, type VoiceMock } from "@/lib/api/voice-mock";
import type { VoiceState } from "@/lib/api/voice-types";
import type { ShareApi } from "./api";
import { MOCK_SOURCES } from "./mock-scenarios";
import {
  HIDDEN_SHARE_VIEW,
  type ShareNotice,
  type ShareRefusal,
  type ShareSource,
  type VoiceShareView,
} from "./types";

export function createShareMock(
  voice: VoiceMock,
  timers = true,
): ShareApi & {
  emitNotice(notice: ShareNotice): void;
} {
  const listeners = new Set<(notice: ShareNotice) => void>();
  const view = (state: VoiceState): VoiceShareView => state.share ?? HIDDEN_SHARE_VIEW;
  const change = (patch: (share: VoiceShareView) => VoiceShareView) =>
    voice.patch((state) => ({ ...state, share: patch(view(state)) }));
  const later = (ms: number, work: () => void) => {
    if (timers) setTimeout(work, ms);
  };
  const kindOf = (source: ShareSource) => source.kind;
  return {
    sources: async () => MOCK_SOURCES,
    start: async (source, label): Promise<ShareRefusal | null> => {
      const share = view(voice.current());
      if (!share.on) return "off";
      if (!share.canShare) return "unsupported";
      if (!share.allowed) return "notAllowed";
      if (!share.open) return "budget";
      if (share.mine.length >= share.mineMax) return "full";
      const n = [1, 2, 3].find((i) => !share.mine.some((m) => m.n === i)) ?? 1;
      change((s) => ({
        ...s,
        mine: [
          ...s.mine,
          {
            n,
            kind: kindOf(source),
            label: source.kind === "game" ? "" : label,
            phase: "starting",
            audio: source.kind !== "screen",
            preset: "normal",
            fps: 30,
            watchers: [],
            since: Math.floor(Date.now() / 1000),
          },
        ],
      }));
      later(1200, () =>
        change((s) => ({
          ...s,
          mine: s.mine.map((m) => (m.n === n ? { ...m, phase: "live" } : m)),
        })),
      );
      later(4000, () =>
        change((s) => ({
          ...s,
          mine: s.mine.map((m) => (m.n === n ? { ...m, watchers: ["301"] } : m)),
        })),
      );
      return null;
    },
    stop: async (n) => change((s) => ({ ...s, mine: s.mine.filter((m) => m.n !== n) })),
    watch: async (person, n) => {
      const share = view(voice.current());
      if (!share.live.some((l) => l.person === person && l.n === n)) return "gone";
      if (share.watching.length >= 2) return "watchMax";
      change((s) => ({
        ...s,
        watching: [...s.watching, { person, n, layer: "f", phase: "connecting" }],
      }));
      later(700, () =>
        change((s) => ({
          ...s,
          watching: s.watching.map((w) =>
            w.person === person && w.n === n ? { ...w, phase: "live" } : w,
          ),
        })),
      );
      return null;
    },
    unwatch: async (person, n) =>
      change((s) => ({
        ...s,
        stillAsking: false,
        watching: s.watching.filter((w) => !(w.person === person && w.n === n)),
      })),
    layer: async (person, n, layer) =>
      change((s) => ({
        ...s,
        watching: s.watching.map((w) => (w.person === person && w.n === n ? { ...w, layer } : w)),
      })),
    attach: async () => {},
    report: async () => {},
    visible: async () => {},
    still: async () => change((s) => ({ ...s, stillAsking: false })),
    popout: async (open) => change((s) => ({ ...s, surface: open ? "popout" : "main" })),
    onTop: async () => {},
    fullscreen: async () => {},
    streamVolume: async (person, volume, muted) =>
      change((s) => ({ ...s, volumes: { ...s.volumes, [person]: { volume, muted } } })),
    setSharers: async (who) => {
      change((s) => ({ ...s, shareBy: who }));
      return true;
    },
    unshare: async (person, n) => {
      change((s) => ({ ...s, live: s.live.filter((l) => !(l.person === person && l.n === n)) }));
      return true;
    },
    onNotice: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    emitNotice: (notice) => {
      for (const listener of listeners) listener(notice);
    },
  };
}

export const mockShare = createShareMock(mockVoice);
