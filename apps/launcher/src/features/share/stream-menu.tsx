// The stream menu (SQOL-U1; sqol.md §1.1): one menu for a watched share, the same rows in the same
// order wherever it opens (a right click on the tile, Shift+F10 or the menu key on it, its "⋯", the
// corner picture): "Yayın Sesi" up to 200 % (a notch at 100 %, heard while it moves, kept per
// sharer), "Yayını Sessize Al", "Kalite", "Konuşulurken Yayını Kıs", full screen and the window,
// bringing one of two shares forward, "Yayın Bilgileri", "Yayını Rapor Et", "İzlemeyi Bırak"; then,
// in the launcher's window, the sharer's own person rows (FIX2: the line's own actions first). The
// "Yayın" window has no person rows and no report (no friend list, no report dialog there).
import {
  ArrowSquareIn,
  ArrowSquareOut,
  Columns,
  CornersIn,
  CornersOut,
  Flag,
  FrameCorners,
  Info,
  PushPin,
  SlidersHorizontal,
  SpeakerHigh,
  SpeakerSimpleSlash,
  Waveform,
  X,
} from "@phosphor-icons/react";
import type { MenuItem } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { formatPercent } from "@/lib/format";
import { DEFAULT_STREAM_SOUND, type StreamSound } from "@/share/stream-audio";
import { type ShareViewQuality, shareKey } from "@/share/types";
import { reportShare, unwatchShare } from "./actions";
import { isPopout, useShare, useShareUi } from "./store";
import { bringForward, keepOnTop, popOut, toggleFullscreen } from "./stream-actions";
import {
  moveStreamVolume,
  setViewPrefs,
  toggleStreamMute,
  useStreamEnv,
  useStreamLive,
} from "./stream-env";
import { type StreamRow, streamRows } from "./stream-view";

const QUALITIES: readonly ShareViewQuality[] = ["auto", "high", "saver"];

/** A person's "Yayın Sesi" as it plays now (a moving value first). */
export function useStreamSound(person: string): StreamSound {
  const saved = useShare().volumes[person] ?? DEFAULT_STREAM_SOUND;
  const live = useStreamLive((s) => s.volumes[person]);
  return live === undefined ? saved : { ...saved, volume: live };
}

/** "Yayın Sesi" as a menu row (also the first row of the tiles' sound menu). */
export function useVolumeRow(person: string): MenuItem {
  const { t } = useTranslation();
  const { volume } = useStreamSound(person);
  const max = useStreamEnv((s) => s.volumeMax);
  return {
    label: t("shareView.menu.volume"),
    icon: <SpeakerHigh size={16} />,
    onSelect: () => {},
    slider: {
      value: Math.min(volume, max),
      min: 0,
      max,
      step: 5,
      notch: max > 100 ? 100 : undefined,
      format: formatPercent,
      onChange: (next, final) => moveStreamVolume(person, next, final),
    },
  };
}

/** "Yayını Sessize Al" as a menu row. */
export function useMuteRow(person: string): MenuItem {
  const { t } = useTranslation();
  const { muted } = useStreamSound(person);
  return {
    label: t("shareView.menu.mute"),
    icon: <SpeakerSimpleSlash size={16} />,
    checked: muted,
    onSelect: () => toggleStreamMute(person),
  };
}

/** The small sound menu (a tile's and the corner's "Yayın Sesi" button): the first two rows. */
export function useSoundItems(person: string): MenuItem[] {
  return [useVolumeRow(person), useMuteRow(person)];
}

/** The stream's own rows, in the contract's order. */
function useOwnRows(person: string, n: number): Array<MenuItem | null> {
  const { t } = useTranslation();
  const share = useShare();
  const volume = useVolumeRow(person);
  const mute = useMuteRow(person);
  const quality = useStreamEnv((s) => s.viewQuality);
  const duck = useStreamEnv((s) => s.duck);
  const full = useShareUi((s) => s.full);
  const forward = useShareUi((s) => s.forward);
  const info = useShareUi((s) => s.info);
  const setInfo = useShareUi((s) => s.setInfo);
  const two = share.watching.length > 1;
  const key = shareKey(person, n);
  const plan = streamRows({ popout: isPopout(), two, forward: forward === key });
  const row = (id: StreamRow): MenuItem => {
    switch (id) {
      case "volume":
        return volume;
      case "mute":
        return mute;
      case "quality":
        return {
          label: t("shareView.menu.quality"),
          icon: <SlidersHorizontal size={16} />,
          onSelect: () => {},
          items: QUALITIES.map((choice) => ({
            label: t(`shareView.quality.${choice}`),
            checked: quality === choice,
            radio: true,
            onSelect: () => setViewPrefs({ viewQuality: choice }),
          })),
        };
      case "duck":
        return {
          label: t("shareView.menu.duck"),
          icon: <Waveform size={16} />,
          checked: duck,
          onSelect: () => setViewPrefs({ duck: !duck }),
        };
      case "fullscreen":
        return {
          label: full ? t("shareView.menu.exitFullscreen") : t("shareView.menu.fullscreen"),
          icon: full ? <CornersIn size={16} /> : <CornersOut size={16} />,
          onSelect: () => toggleFullscreen(two ? key : undefined),
        };
      case "popout":
        return {
          label: t("shareView.menu.popout"),
          icon: <ArrowSquareOut size={16} />,
          onSelect: () => popOut(true),
        };
      case "popin":
        return {
          label: t("shareView.menu.popin"),
          icon: <ArrowSquareIn size={16} />,
          onSelect: () => popOut(false),
        };
      case "onTop":
        return {
          label: t("shareView.menu.onTop"),
          icon: <PushPin size={16} />,
          checked: share.onTop,
          onSelect: () => keepOnTop(!share.onTop),
        };
      case "spotlight":
        return {
          label: t("shareView.menu.forward"),
          icon: <FrameCorners size={16} />,
          onSelect: () => bringForward(key),
        };
      case "sideBySide":
        return {
          label: t("shareView.menu.sideBySide"),
          icon: <Columns size={16} />,
          onSelect: () => bringForward(null),
        };
      case "info":
        return {
          label: t("shareView.menu.info"),
          icon: <Info size={16} />,
          checked: info,
          onSelect: () => setInfo(!info),
        };
      case "report":
        return {
          label: t("share.report.action"),
          icon: <Flag size={16} />,
          onSelect: () => void reportShare(person, n),
        };
      case "stop":
        return {
          label: t("share.stage.stop"),
          icon: <X size={16} />,
          onSelect: () => void unwatchShare(person, n),
        };
    }
  };
  return plan.map((id) => (id === null ? null : row(id)));
}

/** The sharer's person rows (`stream-person.tsx`, the launcher's window only). */
type PersonRows = (person: string) => Array<MenuItem | null>;

let usePersonRows: PersonRows = () => [];

/**
 * The launcher's window gives the menu the sharer's person rows (once, before anything renders):
 * the person menu and the friend list it reads stay out of the "Yayın" window.
 */
export function givePersonRows(rows: PersonRows) {
  usePersonRows = rows;
}

/**
 * The stream menu's rows for a share (`person`, `n`): its own, then the sharer's person rows in the
 * launcher's window (sqol.md §4: the cross-surface contract's `useStreamItems(u, n)`).
 */
export function useStreamItems(person: string, n: number): Array<MenuItem | null> {
  const own = useOwnRows(person, n);
  const people = usePersonRows(person);
  return people.length ? [...own, null, ...people] : own;
}
