// The preview's screen sharing (SHARE): the share commands change the voice mock's state as the
// launcher would (a share goes live after a moment, "İzle" puts the share on the stage), and the
// picker gets a made-up list. Nothing leaves the page; there are no streams (the stage shows a
// still picture, `mockFrame`). SQOL (sqol-r.md "API"): a live share switches (answered with a
// notice, as the launcher does), changes and pauses in place; its card's small picture and its
// numbers come as the launcher's events would (once when asked: no timers keep running).
import { mockVoice, type VoiceMock } from "@/lib/api/voice-mock";
import type { VoiceState } from "@/lib/api/voice-types";
import type { ShareApi } from "./api";
import { MOCK_SOURCES, mockOwnFrame, mockStats, mockThumb } from "./mock-scenarios";
import { param } from "./param";
import {
  HIDDEN_SHARE_VIEW,
  type ShareChange,
  type ShareNotice,
  type SharePreset,
  type SharePreviewFrame,
  type ShareRefusal,
  type ShareSource,
  type ShareStats,
  type ShareThumb,
  type VoiceMyShare,
  type VoiceShareView,
} from "./types";

/** How the preview's launcher differs from a plain one. */
export interface ShareMockOptions {
  /** "Ekranı Değiştir" fails after it was asked (the old source goes on). */
  switchFails?: boolean;
  /** Every raise (quality, frame rate, the sound) is refused: the rollback shows. */
  refuseRaise?: boolean;
}

const RANK: Record<SharePreset, number> = { low: 0, normal: 1, high: 2 };

const sameSource = (a: ShareSource | null | undefined, b: ShareSource) =>
  !!a && a.kind === b.kind && ("id" in a ? a.id : "") === ("id" in b ? b.id : "");

/** A change that asks for more than the share has now. */
function raises(mine: VoiceMyShare, change: ShareChange): boolean {
  return (
    (change.preset !== undefined && RANK[change.preset] > RANK[mine.preset]) ||
    (change.fps !== undefined && change.fps > mine.fps) ||
    (change.audio === true && !mine.audio)
  );
}

export function createShareMock(
  voice: VoiceMock,
  timers = true,
  options: ShareMockOptions = {},
): ShareApi & {
  emitNotice(notice: ShareNotice): void;
  emitStats(stats: ShareStats): void;
  emitPreview(frame: SharePreviewFrame): void;
} {
  const listeners = new Set<(notice: ShareNotice) => void>();
  const previewListeners = new Set<(frame: SharePreviewFrame) => void>();
  const statsListeners = new Set<(stats: ShareStats) => void>();
  const previewing = new Set<number>();
  const view = (state: VoiceState): VoiceShareView => state.share ?? HIDDEN_SHARE_VIEW;
  const change = (patch: (share: VoiceShareView) => VoiceShareView) =>
    voice.patch((state) => ({ ...state, share: patch(view(state)) }));
  const later = (ms: number, work: () => void) => {
    if (timers) setTimeout(work, ms);
  };
  // What the launcher says on its own a moment later (at once without timers: the tests').
  const soon = (ms: number, work: () => void) => {
    if (timers) setTimeout(work, ms);
    else work();
  };
  const kindOf = (source: ShareSource) => source.kind;
  const mineOf = (n: number) => view(voice.current()).mine.find((m) => m.n === n);
  const mutedForAll = () => voice.current().room?.me.mutedByModerator === true;
  const patchMine = (n: number, patch: (mine: VoiceMyShare) => VoiceMyShare) =>
    change((s) => ({ ...s, mine: s.mine.map((m) => (m.n === n ? patch(m) : m)) }));
  const emitNotice = (notice: ShareNotice) => {
    for (const listener of listeners) listener(notice);
  };
  const emitStats = (stats: ShareStats) => {
    for (const listener of statsListeners) listener(stats);
  };
  const emitPreview = (frame: SharePreviewFrame) => {
    for (const listener of previewListeners) listener(frame);
  };
  // What the launcher would say now about one share (its picture while asked, its numbers).
  const tell = (n: number) => {
    const mine = mineOf(n);
    if (!mine) return;
    emitStats(mockStats(mine, (view(voice.current()).hwMax ?? "high") === "high"));
    if (previewing.has(n) && mine.st !== "hold") emitPreview({ n, dataUrl: mockOwnFrame(mine) });
  };
  return {
    sources: async () => MOCK_SOURCES,
    start: async (source, label): Promise<ShareRefusal | null> => {
      const share = view(voice.current());
      if (!share.on) return "off";
      if (!share.canShare) return "unsupported";
      // A moderator's mute for everyone: the room lets no new share start.
      if (!share.allowed || mutedForAll()) return "notAllowed";
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
            st: "live",
            hold: null,
            source,
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
    stop: async (n) => {
      previewing.delete(n);
      change((s) => ({ ...s, mine: s.mine.filter((m) => m.n !== n) }));
    },
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
    // SHARE-LIVE: as the launcher answers after its request (a moment; `thumb=slow` 1.5 s).
    thumb: (room, user) => {
      const force = param("thumb");
      const answer = mockThumb(voice.current(), room, user, force);
      if (!timers) return Promise.resolve(answer);
      return new Promise<ShareThumb>((resolve) =>
        setTimeout(() => resolve(answer), force === "slow" ? 1500 : 220),
      );
    },
    onNotice: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    emitNotice,
    // SQOL: a live share changes in place, as the launcher would answer (sqol-r.md "API").
    switchSource: async (n, source, label) => {
      const share = view(voice.current());
      const mine = mineOf(n);
      if (!mine) return "gone";
      if (!share.edit) return "off";
      if (share.mine.some((m) => m.n !== n && sameSource(m.source, source))) return "inUse";
      if (sameSource(mine.source, source)) return null;
      // The publisher answers a moment later: the old source goes on, or the new one shows.
      soon(400, () => {
        if (options.switchFails) {
          emitNotice({ kind: "switchFailed", n });
          return;
        }
        patchMine(n, (m) => ({
          ...m,
          kind: source.kind,
          label: source.kind === "game" ? "" : label,
          source,
          st: "live",
          hold: null,
          holdUntil: null,
          th: Math.floor(Date.now() / 1000),
        }));
        emitNotice({
          kind: "switched",
          n,
          src: source.kind,
          label: source.kind === "game" ? "" : label,
        });
        later(60, () => tell(n));
      });
      return null;
    },
    update: async (n, asked) => {
      const share = view(voice.current());
      const mine = mineOf(n);
      if (!mine) return "gone";
      if (!share.edit) return "off";
      // A moderator's mute for everyone: the room refuses what it checks (not the launcher's own).
      const roomAsked =
        asked.preset !== undefined || asked.fps !== undefined || asked.audio !== undefined;
      if (roomAsked && mutedForAll()) return "notAllowed";
      if (asked.preset !== undefined && RANK[asked.preset] > RANK[share.maxPreset]) return "budget";
      if (options.refuseRaise && raises(mine, asked)) return "budget";
      // This computer's ceiling holds (the state says what it took); 60 frames only at "Yüksek".
      const top = share.hwMax ?? "high";
      const wanted = asked.preset ?? mine.preset;
      const preset = RANK[wanted] > RANK[top] ? top : wanted;
      const rate = asked.fps ?? mine.fps;
      const fps = preset !== "high" && rate > 30 ? 30 : rate;
      patchMine(n, (m) => ({ ...m, ...asked, preset, fps }));
      later(60, () => tell(n));
      return null;
    },
    hold: async (n, on) => {
      const share = view(voice.current());
      const mine = mineOf(n);
      if (!mine) return "gone";
      if (!share.edit) return "off";
      // Nothing to go on with while the source is gone.
      if (!on && (mine.hold === "closed" || mine.hold === "gameClosed")) return "gone";
      patchMine(n, (m) => ({ ...m, st: on ? "hold" : "live", hold: on ? "paused" : null }));
      if (!on) later(60, () => tell(n));
      return null;
    },
    preview: async (n, on) => {
      if (!on) {
        previewing.delete(n);
        return;
      }
      previewing.add(n);
      later(120, () => tell(n));
    },
    onPreview: (listener) => {
      previewListeners.add(listener);
      return () => previewListeners.delete(listener);
    },
    onStats: (listener) => {
      statsListeners.add(listener);
      // The launcher says it every few seconds: the preview says it once, soon.
      later(80, () => {
        for (const mine of view(voice.current()).mine) tell(mine.n);
      });
      return () => statsListeners.delete(listener);
    },
    emitStats,
    emitPreview,
  };
}

export const mockShare = createShareMock(mockVoice, true, {
  switchFails: param("voice") === "shareSwitchFail",
  refuseRaise: param("edit") === "refuse",
});
