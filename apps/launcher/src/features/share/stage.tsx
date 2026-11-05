// The stage (SHARE; share.md §3): what the player watches. In the launcher's window it covers the
// page under the title bar (one share big, two side by side), or sits small in the corner
// ("Küçült") while the player uses the launcher; in the "Yayın" window it fills the window. Each
// share has its name, its sound ("Yayın sesi", kept per person) and "İzlemeyi bırak"; the stage has
// "Ayrı pencerede aç" / "Launcher'a geri al", "Tam ekran" and, in its window, "Her zaman üstte".
//
// The page asks for the small layer when a share's tile is small (`q`), the full one otherwise
// (`f`); half an hour without a touch it asks "Hâlâ izliyor musun?". Motion: the stage fades and
// rises a little in (transform and opacity), the corner and the full stage cross-fade; reduced
// motion keeps the fades.
import {
  ArrowSquareIn,
  ArrowSquareOut,
  ArrowsIn,
  ArrowsInSimple,
  ArrowsOut,
  CornersOut,
  PushPin,
  PushPinSlash,
  Screencast,
  SpeakerHigh,
  SpeakerSimpleSlash,
  X,
} from "@phosphor-icons/react";
import { Button, cn, Menu, type MenuItem, Tooltip, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { QualityBars } from "@/features/voice/faces";
import { useVoicePrefs } from "@/features/voice/prefs";
import { useFace, useVoiceUi } from "@/features/voice/store";
import { formatPercent } from "@/lib/format";
import { shareApi } from "@/share/api";
import { DEFAULT_SHARE_SETTINGS, type VoiceShareView, type VoiceWatch } from "@/share/types";
import { unwatchShare } from "./actions";
import { useShare, useShareQuality, useShareUi } from "./store";
import { ShareVideo } from "./video";

/** A tile narrower than this (CSS pixels) takes the small layer. */
export const SMALL_TILE_PX = 480;
/** Layer changes wait for a resize to settle. */
const LAYER_SETTLE_MS = 400;

/** Set by the "Yayın" window's page: names come from the launcher there (no friend list). */
let inPopout = false;
export function markPopout() {
  inPopout = true;
}

/** A sharer's name on this page. */
export function useShareName(id: string): string {
  const face = useFace(id);
  const names = useShare().names;
  return inPopout ? (names[id] ?? face.name) : face.name;
}

/** The shares the stage shows: the ones the player watches, never their own preview. */
export function stageWatches(share: VoiceShareView, me: string | null): VoiceWatch[] {
  return share.watching.filter((watch) => watch.person !== me);
}

function useVolume(person: string) {
  const share = useShare();
  return share.volumes[person] ?? { volume: 100, muted: false };
}

/** A control on the stage: an icon on a dark plate, its words in the tooltip. */
function StageButton({
  label,
  onClick,
  children,
  pressed,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
  pressed?: boolean;
}) {
  return (
    <Tooltip content={label}>
      <button
        type="button"
        aria-label={label}
        aria-pressed={pressed}
        onClick={onClick}
        className={cn(
          "flex size-8 cursor-pointer items-center justify-center rounded-[8px] text-bone-100 outline-none",
          "bg-obsidian-950/70 ring-1 ring-white/10 backdrop-blur-sm",
          "transition-[background-color,transform,scale] duration-150 ease-out hover:bg-obsidian-800/90 active:scale-[0.94]",
          "focus-visible:ring-2 focus-visible:ring-gold-300",
          pressed && "bg-white/14",
        )}
      >
        {children}
      </button>
    </Tooltip>
  );
}

/** "Yayın sesi" for one sharer: the person menu's slider, and its mute. */
function VolumeButton({ person }: { person: string }) {
  const { t } = useTranslation();
  const { volume, muted } = useVolume(person);
  const [value, setValue] = useState(volume);
  useEffect(() => setValue(volume), [volume]);
  const items: MenuItem[] = [
    {
      label: t("share.stage.volume"),
      icon: <SpeakerHigh size={16} />,
      onSelect: () => {},
      slider: {
        value,
        min: 0,
        max: 100,
        step: 5,
        format: formatPercent,
        onChange: (next: number, final: boolean) => {
          setValue(next);
          if (final) void shareApi.streamVolume(person, Math.round(next), false);
        },
      },
    },
    {
      label: muted ? t("share.stage.unmute") : t("share.stage.mute"),
      icon: muted ? <SpeakerHigh size={16} /> : <SpeakerSimpleSlash size={16} />,
      onSelect: () => void shareApi.streamVolume(person, volume, !muted),
    },
  ];
  return (
    <Menu
      items={items}
      align="end"
      trigger={
        <button
          type="button"
          aria-label={t("share.stage.volume")}
          className={cn(
            "flex size-8 cursor-pointer items-center justify-center rounded-[8px] outline-none",
            "bg-obsidian-950/70 ring-1 ring-white/10 backdrop-blur-sm",
            "transition-[background-color,transform,scale] duration-150 ease-out hover:bg-obsidian-800/90 active:scale-[0.94]",
            "focus-visible:ring-2 focus-visible:ring-gold-300",
            muted ? "text-flare-400" : "text-bone-100",
          )}
        >
          {muted ? (
            <SpeakerSimpleSlash size={16} weight="bold" />
          ) : (
            <SpeakerHigh size={16} weight="bold" />
          )}
        </button>
      }
    />
  );
}

/** One watched share: its picture, its name, its sound and its way out. */
function Tile({ watch, small }: { watch: VoiceWatch; small: boolean }) {
  const { t } = useTranslation();
  const name = useShareName(watch.person);
  const { volume, muted } = useVolume(watch.person);
  const quality = useShareQuality(watch.person, watch.n);
  const kind = useShare().live.find((l) => l.person === watch.person && l.n === watch.n)?.kind;
  const box = useRef<HTMLElement>(null);

  // The layer follows the tile's size (a small tile gets the small layer).
  useEffect(() => {
    const element = box.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      clearTimeout(timer);
      timer = setTimeout(() => {
        const layer = entry.contentRect.width < SMALL_TILE_PX ? "q" : "f";
        if (layer !== watch.layer) void shareApi.layer(watch.person, watch.n, layer);
      }, LAYER_SETTLE_MS);
    });
    observer.observe(element);
    return () => {
      clearTimeout(timer);
      observer.disconnect();
    };
  }, [watch.person, watch.n, watch.layer]);

  return (
    <section
      ref={box}
      className="relative min-h-0 min-w-0 flex-1 overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-950 ring-1 ring-white/8"
      aria-label={t("share.stage.label", { name })}
    >
      {watch.phase === "held" ? null : (
        <ShareVideo person={watch.person} n={watch.n} volume={volume} muted={muted} />
      )}
      {watch.phase !== "live" ? (
        <p className="absolute inset-0 flex items-center justify-center px-6 text-center text-[13px] text-ash-300">
          {watch.phase === "held" ? t("share.stage.held") : t("share.stage.connecting")}
        </p>
      ) : null}
      {small ? null : (
        <div className="absolute inset-x-2 bottom-2 flex items-end justify-between gap-2">
          <span className="flex min-w-0 items-center gap-2 rounded-[8px] bg-obsidian-950/70 px-2.5 py-1.5 ring-1 ring-white/10 backdrop-blur-sm">
            <span className="truncate text-[13px] leading-[1.3] font-semibold text-bone-50">
              {name}
            </span>
            {kind ? (
              <span className="shrink-0 text-[12px] leading-[1.3] text-ash-300">
                {t(`share.kind.${kind}`)}
              </span>
            ) : null}
            {quality === "weak" ? (
              <Tooltip content={t("share.stage.weak")}>
                <span className="flex shrink-0">
                  <QualityBars quality="weak" />
                </span>
              </Tooltip>
            ) : null}
          </span>
          <span className="flex shrink-0 gap-1.5">
            <VolumeButton person={watch.person} />
            <StageButton
              label={t("share.stage.stop")}
              onClick={() => void unwatchShare(watch.person, watch.n)}
            >
              <X size={16} weight="bold" />
            </StageButton>
          </span>
        </div>
      )}
    </section>
  );
}

/** "Hâlâ izliyor musun?" over the stage. */
function StillAsk({ watches }: { watches: VoiceWatch[] }) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  return (
    <motion.div
      role="alertdialog"
      aria-label={t("share.stage.stillAsk")}
      className="absolute inset-0 z-10 flex items-center justify-center bg-obsidian-950/55"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: { duration: 0.2 } }}
      exit={{ opacity: 0, transition: { duration: 0.14 } }}
    >
      <motion.div
        className="glass-strong flex flex-col items-center gap-4 rounded-[var(--radius-panel)] px-7 py-6"
        initial={{ opacity: 0, y: reduce ? 0 : 6 }}
        animate={{ opacity: 1, y: 0, transition: { duration: 0.24, ease: [0.16, 1, 0.3, 1] } }}
      >
        <p className="font-display text-[17px] leading-[1.25] font-semibold text-bone-50">
          {t("share.stage.stillAsk")}
        </p>
        <div className="flex gap-2">
          <Button
            variant="ghost"
            onClick={() => {
              for (const w of watches) void unwatchShare(w.person, w.n);
            }}
          >
            {t("share.stage.stop")}
          </Button>
          <Button variant="primary" autoFocus onClick={() => void shareApi.still()}>
            {t("share.stage.stillYes")}
          </Button>
        </div>
      </motion.div>
    </motion.div>
  );
}

/** The stage's own controls (top right). */
function StageControls({ popout }: { popout: boolean }) {
  const { t } = useTranslation();
  const full = useShareUi((s) => s.full);
  const setFull = useShareUi((s) => s.setFull);
  const setSmall = useShareUi((s) => s.setSmall);
  const onTop = useVoicePrefs((s) => (s.settings?.share ?? DEFAULT_SHARE_SETTINGS).alwaysOnTop);
  const [pinned, setPinned] = useState(onTop);
  useEffect(() => setPinned(onTop), [onTop]);
  const fullscreen = (on: boolean) => {
    setFull(on);
    void shareApi.fullscreen(on);
  };
  return (
    <div className="flex gap-1.5">
      {popout ? (
        <StageButton
          label={t("share.stage.onTop")}
          pressed={pinned}
          onClick={() => {
            setPinned(!pinned);
            void shareApi.onTop(!pinned);
          }}
        >
          {pinned ? <PushPinSlash size={16} weight="bold" /> : <PushPin size={16} weight="bold" />}
        </StageButton>
      ) : full ? null : (
        <StageButton label={t("share.stage.shrink")} onClick={() => setSmall(true)}>
          <ArrowsInSimple size={16} weight="bold" />
        </StageButton>
      )}
      {full ? null : (
        <StageButton
          label={popout ? t("share.stage.popin") : t("share.stage.popout")}
          onClick={() => void shareApi.popout(!popout)}
        >
          {popout ? (
            <ArrowSquareIn size={16} weight="bold" />
          ) : (
            <ArrowSquareOut size={16} weight="bold" />
          )}
        </StageButton>
      )}
      <StageButton
        label={full ? t("share.stage.exitFullscreen") : t("share.stage.fullscreen")}
        onClick={() => fullscreen(!full)}
      >
        {full ? <ArrowsIn size={16} weight="bold" /> : <CornersOut size={16} weight="bold" />}
      </StageButton>
    </div>
  );
}

/** The stage's content: the tiles, the controls, the question. */
export function StageBody({ watches, popout }: { watches: VoiceWatch[]; popout: boolean }) {
  const share = useShare();
  const full = useShareUi((s) => s.full);
  const setFull = useShareUi((s) => s.setFull);

  // Esc leaves full screen; any key or press on the stage answers "Hâlâ izliyor musun?" too.
  useEffect(() => {
    if (!full) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setFull(false);
        void shareApi.fullscreen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [full, setFull]);

  return (
    <div className="relative flex size-full min-h-0 flex-col p-3">
      <div className="flex min-h-0 flex-1 gap-3">
        {watches.map((watch) => (
          <Tile key={`${watch.person}:${watch.n}`} watch={watch} small={false} />
        ))}
      </div>
      <div className="absolute top-5 right-5">
        <StageControls popout={popout} />
      </div>
      <AnimatePresence>{share.stillAsking ? <StillAsk watches={watches} /> : null}</AnimatePresence>
    </div>
  );
}

/** The corner picture while the stage is small. */
function CornerStage({ watch }: { watch: VoiceWatch }) {
  const { t } = useTranslation();
  const name = useShareName(watch.person);
  const setSmall = useShareUi((s) => s.setSmall);
  const { volume, muted } = useVolume(watch.person);
  return (
    <div className="relative size-full overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-950 ring-1 ring-white/12">
      <button
        type="button"
        aria-label={`${t("share.stage.grow")}: ${t("share.stage.label", { name })}`}
        onClick={() => setSmall(false)}
        className="absolute inset-0 cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:ring-inset"
      >
        <ShareVideo person={watch.person} n={watch.n} volume={volume} muted={muted} fit="cover" />
      </button>
      <span className="pointer-events-none absolute bottom-1.5 left-1.5 max-w-[60%] truncate rounded-[6px] bg-obsidian-950/70 px-2 py-1 text-[12px] leading-[1.3] font-semibold text-bone-50">
        {name}
      </span>
      <span className="absolute top-1.5 right-1.5 flex gap-1">
        <StageButton label={t("share.stage.grow")} onClick={() => setSmall(false)}>
          <ArrowsOut size={15} weight="bold" />
        </StageButton>
        <StageButton
          label={t("share.stage.stop")}
          onClick={() => void unwatchShare(watch.person, watch.n)}
        >
          <X size={15} weight="bold" />
        </StageButton>
      </span>
    </div>
  );
}

/** The stage in the launcher's window (nothing while nothing is watched here). */
export function ShareStage() {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const share = useShare();
  const me = useVoiceUi((s) => s.state.room?.me.id ?? null);
  const small = useShareUi((s) => s.small);
  const full = useShareUi((s) => s.full);
  const watches = share.surface === "main" ? stageWatches(share, me) : [];
  const shown = watches.length > 0;
  const first = watches[0];
  return (
    <AnimatePresence>
      {shown && small && first && !full ? (
        <motion.section
          key="corner"
          aria-label={t("share.stage.label", { name: "" })}
          className="fixed right-4 bottom-[calc(var(--spacing-commandbar)+12px)] z-20 aspect-video w-[320px] shadow-[0_18px_50px_rgb(63_10_10/0.45)]"
          initial={{ opacity: 0, scale: reduce ? 1 : 0.96 }}
          animate={{
            opacity: 1,
            scale: 1,
            transition: { duration: 0.22, ease: [0.16, 1, 0.3, 1] },
          }}
          exit={{ opacity: 0, scale: reduce ? 1 : 0.97, transition: { duration: 0.14 } }}
        >
          <CornerStage watch={first} />
        </motion.section>
      ) : shown ? (
        <motion.section
          key="stage"
          aria-label={t("share.live.on")}
          className={cn(
            "fixed z-20 flex flex-col bg-obsidian-950/92 backdrop-blur-md",
            full ? "inset-0 z-[60]" : "top-titlebar right-0 bottom-0 left-rail",
          )}
          initial={{ opacity: 0, y: reduce ? 0 : 8 }}
          animate={{ opacity: 1, y: 0, transition: { duration: 0.26, ease: [0.16, 1, 0.3, 1] } }}
          exit={{ opacity: 0, transition: { duration: 0.16 } }}
        >
          <h2 className="sr-only">
            <Screencast aria-hidden />
            {t("share.live.on")}
          </h2>
          <StageBody watches={watches} popout={false} />
        </motion.section>
      ) : null}
    </AnimatePresence>
  );
}
