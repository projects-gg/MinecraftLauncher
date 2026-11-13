// The player's own share on the room panel (SQOL, sqol.md §1.2; replaces SHARE's "Yayında" line):
// its small live picture (asked only while the card is on screen; the room's last picture
// otherwise, none in Performance mode), what it shows with "YAYINDA" after it, its quality
// ("Normal · 720p"), who watches, and "Ekranı Değiştir", "⋯" and "Durdur". A right click (Shift+F10,
// the menu key) opens the "⋯" menu. Its states say themselves on the same line: "Başlıyor…",
// "Duraklatıldı" (with "Sürdür"), "Pencere küçültüldü.", and a window or game that closed ("Başka
// Ekran Seç", "Paylaşımı Durdur", a thin line running out with the minute it waits). Pressing the
// picture asks the stage to show the share big (SQOL-U1 draws it); its tooltip says what really
// goes out ("1280×720 · 30 fps · Donanım").
import {
  AppWindow,
  DotsThree,
  GameController,
  Minus,
  Monitor,
  Pause,
  Play,
  SpeakerSlash,
  Swap,
  Warning,
} from "@phosphor-icons/react";
import {
  Button,
  cn,
  IconButton,
  Menu,
  Tooltip,
  usePerf,
  usePressGuard,
  useReduceMotion,
} from "@pl/ui";
import type { TFunction } from "i18next";
import {
  type KeyboardEvent,
  type MouseEvent,
  type RefObject,
  useEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { SpeakingFace } from "@/features/voice/faces";
import { useFaces, useVoiceUi } from "@/features/voice/store";
import type { VoiceMyShare } from "@/share/types";
import { holdShare, lostText, qualityText, stopShare } from "./actions";
import { type LiveTarget, useShareThumb } from "./live";
import { LiveTag } from "./live-card";
import { statsLine, useOwnSharePreview, useOwnShareStats } from "./own-live";
import { ownShareName, useOwnShareItems } from "./own-menu";
import {
  isLost,
  type OwnStatus,
  openSwitch,
  showOwnTile,
  useOwnShare,
  useOwnStatus,
} from "./own-store";
import { useShare } from "./store";

/** Who watches: their faces (three at most) and how many; their names on the pointer. */
export function Watchers({ ids }: { ids: string[] }) {
  const { t } = useTranslation();
  const faces = useFaces(ids);
  if (!ids.length) {
    return (
      <span className="text-[12px] leading-[1.35] text-ash-400">{t("share.live.resting")}</span>
    );
  }
  const names = faces.map((f) => f.name).join(", ");
  return (
    <Tooltip content={t("share.live.watchedBy", { names })}>
      <span className="flex min-w-0 items-center gap-1.5">
        <span className="sr-only">{t("share.live.watchedBy", { names })}</span>
        <span className="flex shrink-0 -space-x-1.5" aria-hidden>
          {faces.slice(0, 3).map((face) => (
            <SpeakingFace
              key={face.id}
              face={face}
              size={18}
              round
              speaking={false}
              className="ring-2 ring-obsidian-900"
            />
          ))}
        </span>
        <span
          className="truncate text-[12px] leading-[1.35] whitespace-nowrap text-ash-300"
          aria-hidden
        >
          {t("share.live.watchers", { count: ids.length })}
        </span>
      </span>
    </Tooltip>
  );
}

/** The card is on screen (an older engine without the observer: always). */
function useOnScreen(ref: RefObject<HTMLElement | null>): boolean {
  const [seen, setSeen] = useState(true);
  useEffect(() => {
    const element = ref.current;
    if (!element || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver((entries) => {
      const last = entries[entries.length - 1];
      if (last) setSeen(last.isIntersecting);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref]);
  return seen;
}

/** The share's picture as the room has it (its `th`), for when no live one comes. */
function useStill(mine: VoiceMyShare, ask: boolean): string | null {
  const room = useVoiceUi((s) => s.state.room);
  const target: LiveTarget | null = room
    ? {
        person: room.me.id,
        room: room.id,
        crew: room.crew,
        general: room.general,
        roomName: null,
        n: mine.n,
        kind: mine.kind,
        watchers: mine.watchers.length,
        th: mine.th ?? null,
        offer: "own",
      }
    : null;
  const { thumb } = useShareThumb(target, ask && mine.th != null);
  return thumb?.kind === "image" ? thumb.dataUrl : null;
}

/**
 * The thin line under a share whose window or game went away: it runs out with the minute (a
 * transform, on the compositor; none with less motion, where the words say it).
 */
function RunningOut({ until }: { until: number }) {
  const line = useRef<HTMLSpanElement>(null);
  const reduce = useReduceMotion();
  useEffect(() => {
    const element = line.current;
    if (!element || reduce || typeof element.animate !== "function") return;
    const left = Math.max(0, until * 1000 - Date.now());
    const from = Math.min(1, left / 60_000);
    const run = element.animate([{ transform: `scaleX(${from})` }, { transform: "scaleX(0)" }], {
      duration: left,
      easing: "linear",
      fill: "forwards",
    });
    // Cancelling rejects `finished`: nobody waits on it.
    run.finished.catch(() => {});
    return () => run.cancel();
  }, [until, reduce]);
  if (reduce) return null;
  return (
    <span
      ref={line}
      aria-hidden
      className="absolute inset-x-0 bottom-0 h-[2px] origin-left bg-gold-400/80"
    />
  );
}

const KIND_ICON = { game: GameController, window: AppWindow, screen: Monitor } as const;

/** The share's small picture, and its state over it when it is not live. */
function Picture({
  mine,
  status,
  until,
  src,
}: {
  mine: VoiceMyShare;
  status: OwnStatus;
  until: number | null;
  src: string | null;
}) {
  const Kind = KIND_ICON[mine.kind];
  const lost = isLost(status);
  // The state's own mark over the picture (none while live or starting).
  const mark =
    status === "paused" ? (
      <Pause size={20} weight="fill" className="text-bone-100" />
    ) : status === "minimized" ? (
      // The window's own minimise mark (an arrow down read as "download").
      <Minus size={20} weight="bold" className="text-bone-100" />
    ) : lost ? (
      <Warning size={20} weight="fill" className="text-gold-400" />
    ) : null;
  return (
    <span className="relative block aspect-video w-[112px] shrink-0 overflow-hidden rounded-[6px] bg-obsidian-950/70 ring-1 ring-white/8">
      {src ? (
        <img
          src={src}
          alt=""
          draggable={false}
          className={cn("absolute inset-0 size-full object-cover", lost && "opacity-40")}
        />
      ) : mark ? null : (
        <span className="absolute inset-0 flex items-center justify-center text-ash-500">
          <Kind size={20} aria-hidden />
        </span>
      )}
      {mark ? (
        <span
          aria-hidden
          className={cn(
            "absolute inset-0 flex items-center justify-center",
            src && "bg-obsidian-950/60",
          )}
        >
          {mark}
        </span>
      ) : null}
      {lost && until !== null ? <RunningOut until={until} /> : null}
    </span>
  );
}

/** The line under the share's name: its quality while live, else its state. */
function stateLine(status: OwnStatus, mine: VoiceMyShare, t: TFunction): string {
  switch (status) {
    case "starting":
      return t("share.live.starting");
    case "paused":
      return t("share.own.paused");
    case "minimized":
      return t("share.own.minimized");
    case "closed":
      return lostText(mine.kind);
    case "gameClosed":
      return lostText("game");
    case "live":
      return qualityText(mine.preset, mine.fps);
  }
}

/** One of the player's shares (`mine` as the state has it; changes on their way show at once). */
export function OwnShareCard({ mine: given }: { mine: VoiceMyShare }) {
  const { t } = useTranslation();
  const share = useShare();
  const mine = useOwnShare(given.n) ?? given;
  const { status, until } = useOwnStatus(mine);
  const items = useOwnShareItems(mine.n);
  const [menuOpen, setMenuOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const seen = useOnScreen(root);
  const { performanceMode, unseen } = usePerf();
  // The live picture only while someone can see it, never in Performance mode (it stops video).
  const asking = seen && !unseen && !performanceMode && status === "live";
  const frame = useOwnSharePreview(mine.n, asking);
  const still = useStill(mine, seen && !unseen && frame === null);
  const stats = useOwnShareStats(mine.n);
  const name = ownShareName(mine, t);
  const lost = isLost(status);
  const edit = share.edit === true;
  const hasMenu = items.length > 1;
  const pressable = status !== "starting" && !lost;

  const openMenu = () => {
    if (hasMenu) setMenuOpen(true);
  };
  // A right click on the card (not one passing through from an open menu or a tooltip).
  const onContextMenu = (event: MouseEvent<HTMLDivElement>) => {
    if (!event.currentTarget.contains(event.target as Node)) return;
    event.preventDefault();
    openMenu();
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
      event.preventDefault();
      openMenu();
    }
  };

  // The first button's role: "Sürdür" becomes "Ekranı Değiştir" in its place the moment it is
  // pressed, so a new role never takes the second press of a double click (design.md §6; the
  // pause's double click opened the picker).
  const role = !edit
    ? null
    : lost
      ? "pickOther"
      : status === "paused"
        ? "resume"
        : status === "live" || status === "minimized"
          ? "switch"
          : null;
  const guard = usePressGuard(role ?? "none");
  const primary =
    role === null ? null : (
      <Button
        size="sm"
        variant={role === "pickOther" ? "bone" : "secondary"}
        icon={
          role === "resume" ? <Play size={15} weight="fill" /> : <Swap size={15} weight="bold" />
        }
        aria-disabled={!guard.armed || undefined}
        {...guard.handlers}
        onClick={(event) => {
          if (!guard.take(event)) return;
          if (role === "resume") void holdShare(mine.n, false);
          else openSwitch(mine.n);
        }}
      >
        {role === "pickOther"
          ? t("share.own.pickOther")
          : role === "resume"
            ? t("share.own.resume")
            : t("share.own.switch")}
      </Button>
    );

  const picture = <Picture mine={mine} status={status} until={until} src={frame ?? still} />;

  return (
    // A named group of buttons (a right click or Shift+F10 in it opens "⋯").
    // biome-ignore lint/a11y/useSemanticElements: a card of buttons, not a form's fieldset
    <div
      ref={root}
      role="group"
      aria-label={t("share.own.cardLabel", { name })}
      onContextMenu={onContextMenu}
      onKeyDown={onKeyDown}
      className="rounded-[var(--radius-panel)] bg-white/[0.045] p-2"
    >
      <div className="flex gap-2.5">
        {pressable ? (
          <Tooltip
            content={
              <span className="flex flex-col">
                <span>{t("share.own.showOwn")}</span>
                {stats ? (
                  <span className="font-normal text-ash-300 tabular-nums">{statsLine(stats)}</span>
                ) : null}
              </span>
            }
          >
            <button
              type="button"
              aria-label={t("share.own.showOwn")}
              onClick={() => showOwnTile(mine.n)}
              className="cursor-pointer rounded-[6px] outline-none transition-[transform,scale] duration-150 ease-out active:scale-[0.97] focus-visible:ring-2 focus-visible:ring-gold-300"
            >
              {picture}
            </button>
          </Tooltip>
        ) : (
          picture
        )}
        <div className="flex min-w-0 flex-1 flex-col justify-center gap-0.5">
          <span className="flex min-w-0 items-center gap-1.5">
            <span className="truncate text-[13px] leading-[1.3] font-semibold text-bone-50">
              {name}
            </span>
            {status === "live" ? <LiveTag /> : null}
          </span>
          {/* A new state fades in where the old one was (CSS, a fade with less motion too). */}
          <span
            key={status}
            className={cn(
              "flex min-w-0 animate-[fade-in_160ms_ease-out] items-center gap-1 text-[12px] leading-[1.35]",
              lost ? "text-bone-100" : "text-ash-400",
            )}
          >
            <span className="truncate">{stateLine(status, mine, t)}</span>
            {status === "live" && !mine.audio ? (
              <SpeakerSlash size={13} aria-hidden className="shrink-0 text-ash-400" />
            ) : null}
            {status === "live" ? (
              <span className="sr-only">
                {mine.audio ? t("share.own.soundOn") : t("share.own.soundOff")}
              </span>
            ) : null}
          </span>
          {status !== "starting" ? <Watchers ids={mine.watchers} /> : null}
        </div>
      </div>
      <div className="mt-2 flex items-center gap-1.5">
        {primary}
        <span className="min-w-0 flex-1" />
        {hasMenu ? (
          <Menu
            items={items}
            open={menuOpen}
            onOpenChange={setMenuOpen}
            title={name}
            trigger={
              <IconButton
                label={t("share.own.more")}
                icon={<DotsThree size={18} weight="bold" />}
                size="sm"
                aria-keyshortcuts="Shift+F10"
              />
            }
          />
        ) : null}
        <Button
          size="sm"
          variant="secondary"
          aria-label={t("share.own.menu.stop")}
          onClick={() => void stopShare(mine.n)}
        >
          {lost ? t("share.own.menu.stop") : t("share.live.stop")}
        </Button>
      </div>
    </div>
  );
}
