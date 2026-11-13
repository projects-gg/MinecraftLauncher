// The player's own share's menu (SQOL, sqol.md §1.2): the same rows in the same order wherever the
// share shows (the card's "⋯" and right click, the title bar pill's share part, the stage's own
// tile): "Ekranı Değiştir…", "Kalite" ›, "Sesi de Paylaş", "Duraklat"/"Sürdür", "Gelişmiş" ›, a
// line, "Paylaşımı Durdur". A row that cannot be chosen now stays dim with one sentence why (this
// computer's encoder, the service's or the month's limit, no sound on this computer). A service
// that does not let a live share change leaves "Paylaşımı Durdur" alone.
import {
  GearSix,
  Pause,
  Play,
  SlidersHorizontal,
  SpeakerHigh,
  Stop,
  Swap,
} from "@phosphor-icons/react";
import type { MenuItem } from "@pl/ui";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { useVoicePrefs } from "@/features/voice/prefs";
import {
  DEFAULT_SHARE_SETTINGS,
  type ShareChange,
  type ShareMotion,
  type SharePreset,
  type VoiceMyShare,
  type VoiceShareSettings,
} from "@/share/types";
import { changeShare, holdShare, presetText, stopShare } from "./actions";
import { isLost, type OwnStatus, openSwitch, ownStatus, useOwnShares, useOwnUi } from "./own-store";
import { useShare } from "./store";

const PRESETS: readonly SharePreset[] = ["low", "normal", "high"];
const RANK: Record<SharePreset, number> = { low: 0, normal: 1, high: 2 };
const RATES = [15, 30, 60] as const;
const MOTIONS: readonly ShareMotion[] = ["auto", "clarity", "motion"];

/** What the rows need to know of one share. */
export interface OwnMenuInput {
  /** The share as the player sees it now (changes on their way shown). */
  mine: VoiceMyShare;
  status: OwnStatus;
  /** The service lets a live share change (`share.edit`). */
  edit: boolean;
  /** The highest quality the service and the month allow now. */
  maxPreset: SharePreset;
  /** This computer's highest quality ("normal" without a hardware encoder). */
  hwMax: SharePreset;
  /** This computer can share an app's sound. */
  canAudio: boolean;
  /** The player's kept choices (a share that does not say its cursor or picture follows them). */
  settings: VoiceShareSettings;
}

/** What the rows do. */
export interface OwnMenuActions {
  switchSource: (n: number) => void;
  change: (n: number, change: ShareChange) => void;
  hold: (n: number, on: boolean) => void;
  stop: (n: number) => void;
}

export const OWN_ACTIONS: OwnMenuActions = {
  switchSource: openSwitch,
  change: (n, change) => void changeShare(n, change),
  hold: (n, on) => void holdShare(n, on),
  stop: (n) => void stopShare(n),
};

const nothing = () => {};

/** A quality change; leaving "Yüksek" at 60 frames comes down to 30 with it. */
function presetChange(mine: VoiceMyShare, preset: SharePreset): ShareChange {
  return preset !== "high" && mine.fps > 30 ? { preset, fps: 30 } : { preset };
}

/** The rows of one share's menu (pure: the tests read them). */
export function ownShareItems(
  input: OwnMenuInput,
  t: TFunction,
  act: OwnMenuActions = OWN_ACTIONS,
): Array<MenuItem | null> {
  const { mine, status, edit, maxPreset, hwMax, canAudio, settings } = input;
  const { n } = mine;
  const stop: MenuItem = {
    label: t("share.own.menu.stop"),
    icon: <Stop size={16} />,
    onSelect: () => act.stop(n),
  };
  // Nothing changes before it is live, nor on a service that does not let it.
  if (!edit || status === "starting") return [stop];
  const quality: MenuItem = {
    label: t("share.quality.label"),
    icon: <SlidersHorizontal size={16} />,
    onSelect: nothing,
    items: PRESETS.map((preset) => {
      // This computer first (no limit lifts that), then the service's or the month's.
      const computer = RANK[preset] > RANK[hwMax];
      const capped = !computer && RANK[preset] > RANK[maxPreset];
      return {
        label: presetText(preset),
        radio: true,
        checked: mine.preset === preset,
        disabled: computer || capped,
        hint: computer
          ? t("share.own.computerCap", { quality: t(`share.quality.${hwMax}`) })
          : capped
            ? t("share.quality.capped")
            : undefined,
        onSelect: () => {
          if (mine.preset !== preset) act.change(n, presetChange(mine, preset));
        },
      };
    }),
  };
  const sound: MenuItem = {
    label: t("share.audio.label"),
    icon: <SpeakerHigh size={16} />,
    checked: canAudio && mine.audio,
    disabled: !canAudio,
    hint: canAudio ? undefined : t("share.audio.unsupported"),
    onSelect: () => act.change(n, { audio: !mine.audio }),
  };
  // A share whose source went away has nothing to pause: "Ekranı Değiştir…" is its way on.
  const pause: MenuItem[] = isLost(status)
    ? []
    : status === "paused"
      ? [
          {
            label: t("share.own.menu.resume"),
            icon: <Play size={16} />,
            onSelect: () => act.hold(n, false),
          },
        ]
      : [
          {
            label: t("share.own.menu.pause"),
            icon: <Pause size={16} />,
            onSelect: () => act.hold(n, true),
          },
        ];
  const cursor = mine.cursor ?? settings.cursor;
  const thumbs = mine.thumbs ?? settings.thumbs ?? true;
  const motion = mine.motion ?? settings.motion;
  const advanced: MenuItem = {
    label: t("share.advanced.label"),
    icon: <GearSix size={16} />,
    onSelect: nothing,
    items: [
      {
        label: t("share.advanced.fps"),
        onSelect: nothing,
        items: RATES.map((fps) => {
          const needsHigh = fps === 60 && mine.preset !== "high";
          return {
            label: t("share.own.fps", { fps }),
            radio: true,
            checked: mine.fps === fps,
            disabled: needsHigh,
            hint: needsHigh ? t("share.own.fpsNeedsHigh") : undefined,
            onSelect: () => {
              if (mine.fps !== fps) act.change(n, { fps });
            },
          };
        }),
      },
      {
        label: t("share.advanced.motion"),
        onSelect: nothing,
        items: MOTIONS.map((value) => ({
          label:
            value === "auto"
              ? t("share.advanced.motionAuto")
              : value === "clarity"
                ? t("share.advanced.clarity")
                : t("share.advanced.smooth"),
          radio: true,
          checked: motion === value,
          onSelect: () => {
            if (motion !== value) act.change(n, { motion: value });
          },
        })),
      },
      {
        label: t("share.advanced.cursor"),
        checked: cursor,
        onSelect: () => act.change(n, { cursor: !cursor }),
      },
      {
        label: t("share.advanced.thumbs"),
        checked: thumbs,
        onSelect: () => act.change(n, { thumbs: !thumbs }),
      },
    ],
  };
  return [
    {
      label: t("share.own.menu.switch"),
      icon: <Swap size={16} />,
      onSelect: () => act.switchSource(n),
    },
    quality,
    sound,
    ...pause,
    advanced,
    null,
    stop,
  ];
}

/** Everything the rows of the player's shares read, in one place (one set of subscriptions). */
function useOwnMenuInputs(): {
  shares: VoiceMyShare[];
  input: (mine: VoiceMyShare) => OwnMenuInput;
} {
  const share = useShare();
  const shares = useOwnShares();
  const lost = useOwnUi((s) => s.lost);
  const stored = useVoicePrefs((s) => s.settings?.share);
  const settings = { ...DEFAULT_SHARE_SETTINGS, ...stored };
  return {
    shares,
    input: (mine) => ({
      mine,
      status: ownStatus(mine, lost[mine.n]),
      edit: share.edit === true,
      maxPreset: share.maxPreset,
      hwMax: share.hwMax ?? "high",
      canAudio: share.audio,
      settings,
    }),
  };
}

/**
 * The menu of the player's share `n` (empty when it is gone): the card's, the pill's and the
 * stage's own tile's (SQOL-U1).
 */
export function useOwnShareItems(n: number): Array<MenuItem | null> {
  const { t } = useTranslation();
  const { shares, input } = useOwnMenuInputs();
  const mine = shares.find((m) => m.n === n);
  return mine ? ownShareItems(input(mine), t) : [];
}

/** What a share shows, in one word or its name ("Oyun", "Kale planı - Paint", "Ekran 2"). */
export function ownShareName(mine: Pick<VoiceMyShare, "kind" | "label">, t: TFunction): string {
  return mine.kind === "game" ? t("share.kind.game") : mine.label || t(`share.kind.${mine.kind}`);
}

/**
 * The title bar pill's menu: one share's rows directly; two shares each in a submenu named by
 * what it shows (design.md §6: a line that stands for several gives each its own submenu).
 */
export function useOwnPillItems(): Array<MenuItem | null> {
  const { t } = useTranslation();
  const { shares, input } = useOwnMenuInputs();
  if (shares.length === 0) return [];
  const [only] = shares;
  if (shares.length === 1 && only) return ownShareItems(input(only), t);
  return shares.map((mine) => ({
    label: ownShareName(mine, t),
    onSelect: nothing,
    items: ownShareItems(input(mine), t),
  }));
}
