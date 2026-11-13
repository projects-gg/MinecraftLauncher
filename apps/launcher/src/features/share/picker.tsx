// "Ekranını paylaş" (SHARE; share.md §3): what to share, then how. The game the launcher started
// comes first and is chosen at once ("Oyunu paylaş"); then the other windows with their pictures;
// the screens last, with the one line that says what a screen shows. Under them the two choices
// most people need (quality, the sound); the rest waits under "Gelişmiş", closed. What the player
// chooses here is kept for the next share (Settings → Sesli sohbet → "Ekran paylaşımı").
//
// SQOL ("Ekranı Değiştir", sqol.md §1.2): the same list for a share that goes on. What it shows now
// is marked "Paylaşılıyor", what the player's other share shows is marked and cannot be chosen, the
// choices under the list are not there, and a choice switches at once (the watchers stay).
import { CaretDown, GameController, Monitor, Warning } from "@phosphor-icons/react";
import { Button, cn, Dialog, Segmented, Skeleton, Switch, Tooltip, useMotionPrefs } from "@pl/ui";
import type { TFunction } from "i18next";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useVoicePrefs } from "@/features/voice/prefs";
import { shareApi } from "@/share/api";
import {
  DEFAULT_SHARE_SETTINGS,
  type SharePreset,
  type ShareSource,
  type ShareSourceInfo,
  type ShareSources,
  type VoiceMyShare,
  type VoiceShareSettings,
} from "@/share/types";
import { setShareSetting, startShare, switchShare } from "./actions";
import { endSwitch, useOwnUi } from "./own-store";
import { useShare, useShareUi } from "./store";

const PRESETS: readonly SharePreset[] = ["low", "normal", "high"];
const RANK: Record<SharePreset, number> = { low: 0, normal: 1, high: 2 };

/** A dialog's exit (210 ms, design.md §5) and a little more: the switch is forgotten after it. */
const SWITCH_FORGET_MS = 400;

const sameSource = (a: ShareSource | null, b: ShareSource) =>
  a !== null && a.kind === b.kind && ("id" in a ? a.id : "") === ("id" in b ? b.id : "");

/** Whether one of the player's shares shows this source (`label`: the name the picker gives it). */
export function showsSource(
  mine: Pick<VoiceMyShare, "kind" | "label" | "source">,
  info: ShareSourceInfo,
  label: string,
): boolean {
  if (mine.source) return sameSource(mine.source, info.source);
  if (mine.kind !== info.source.kind) return false;
  return mine.kind === "game" || mine.label === label;
}

/** How a choice stands in "Ekranı Değiştir": free, shown now, or shown by the other share. */
type Busy = "free" | "current" | "taken";

/** A picture of a source (or its kind's mark while it has none), 16:9. */
function Thumb({
  info,
  kind,
  busy = "free",
}: {
  info: ShareSourceInfo | null;
  kind: "game" | "window" | "screen";
  busy?: Busy;
}) {
  const { t } = useTranslation();
  return (
    <span className="relative block aspect-video w-full overflow-hidden rounded-[6px] bg-obsidian-950/70 ring-1 ring-white/8">
      {info?.thumb ? (
        <img
          src={info.thumb}
          alt=""
          draggable={false}
          className={cn(
            "absolute inset-0 size-full object-cover",
            busy === "taken" && "opacity-45",
          )}
        />
      ) : (
        <span className="absolute inset-0 flex items-center justify-center text-ash-500">
          {kind === "game" ? <GameController size={22} /> : <Monitor size={22} />}
        </span>
      )}
      {busy !== "free" ? (
        // On a dark base, so the words read on any picture.
        <span
          aria-hidden
          className="chamfer absolute top-1.5 left-1.5 flex h-[20px] items-center bg-obsidian-950/85 px-[7px] text-[11.5px] font-semibold whitespace-nowrap text-jade-400 [--cut:5px]"
        >
          {t("share.pick.sharing")}
        </span>
      ) : null}
    </span>
  );
}

/**
 * What a choice is in each mode: a radio to pick before "Paylaş" (a picture choice: a radio input
 * cannot hold it), or a button that switches at once; what a share shows now is its current one.
 */
function choiceProps(
  mode: "start" | "switch",
  chosen: boolean,
  busy: Busy,
  label: string | null,
  t: TFunction,
) {
  if (mode === "start") {
    return {
      role: "radio" as const,
      "aria-checked": chosen,
      ...(label !== null
        ? { "aria-label": chosen ? `${label}, ${t("share.pick.chosen")}` : label }
        : {}),
    };
  }
  return {
    ...(label !== null
      ? { "aria-label": busy === "free" ? label : `${label}, ${t("share.pick.sharing")}` }
      : {}),
    "aria-disabled": busy !== "free" || undefined,
    "aria-current": busy === "current" || undefined,
  };
}

/** One choice: a picture and its name; chosen, it sits on the quiet plate with a bone edge. */
function Choice({
  info,
  kind,
  label,
  meta,
  chosen,
  mode,
  busy,
  onChoose,
}: {
  info: ShareSourceInfo;
  kind: "window" | "screen";
  label: string;
  meta?: string | null;
  chosen: boolean;
  mode: "start" | "switch";
  busy: Busy;
  onChoose: () => void;
}) {
  const { t } = useTranslation();
  const lit = mode === "start" ? chosen : busy === "current";
  const off = mode === "switch" && busy !== "free";
  return (
    <button
      type="button"
      {...choiceProps(mode, chosen, busy, label, t)}
      onClick={off ? undefined : onChoose}
      className={cn(
        "group flex min-w-0 flex-col gap-1.5 rounded-[var(--radius-panel)] p-1.5 text-left outline-none",
        "transition-[background-color,box-shadow,transform,scale] duration-150 ease-out",
        "focus-visible:ring-2 focus-visible:ring-gold-300",
        off ? "cursor-default" : "cursor-pointer active:scale-[0.98]",
        lit ? "bg-white/8 ring-1 ring-bone-100/70" : off ? null : "hover:bg-white/[0.045]",
      )}
    >
      <Thumb info={info} kind={kind} busy={mode === "switch" ? busy : "free"} />
      <span className="flex min-w-0 items-baseline gap-1.5 px-0.5">
        <span
          className={cn(
            "truncate text-[12.5px] leading-[1.35] font-medium",
            busy === "taken" && mode === "switch" ? "text-ash-400" : "text-bone-100",
          )}
        >
          {label}
        </span>
        {meta ? (
          <span className="shrink-0 text-[12px] leading-[1.35] text-ash-400">{meta}</span>
        ) : null}
      </span>
    </button>
  );
}

function GameChoice({
  game,
  chosen,
  mode,
  busy,
  onChoose,
}: {
  game: ShareSourceInfo | null;
  chosen: boolean;
  mode: "start" | "switch";
  busy: Busy;
  onChoose: () => void;
}) {
  const { t } = useTranslation();
  const lit = mode === "start" ? chosen : busy === "current";
  const off = !game || (mode === "switch" && busy !== "free");
  const label = t("share.pick.game");
  return (
    // Its name is its words (the game's version included); "Paylaşılıyor" is read after them.
    <button
      type="button"
      {...choiceProps(mode, chosen, busy, null, t)}
      aria-disabled={off || undefined}
      onClick={off ? undefined : onChoose}
      className={cn(
        "flex w-full items-center gap-4 rounded-[var(--radius-panel)] p-2 text-left outline-none",
        "transition-[background-color,box-shadow,transform,scale] duration-150 ease-out",
        "focus-visible:ring-2 focus-visible:ring-gold-300",
        off ? "cursor-default" : "cursor-pointer active:scale-[0.99]",
        lit ? "bg-white/8 ring-1 ring-bone-100/70" : off ? null : "hover:bg-white/[0.045]",
      )}
    >
      <span className={cn("w-[152px] shrink-0", !game && "opacity-50")}>
        <Thumb info={game} kind="game" busy={mode === "switch" && game ? busy : "free"} />
      </span>
      <span className="flex min-w-0 flex-col gap-0.5">
        <span
          className={cn(
            "font-display text-[15px] leading-[1.3] font-semibold",
            game && !(mode === "switch" && busy === "taken") ? "text-bone-50" : "text-ash-300",
          )}
        >
          {label}
        </span>
        <span className="truncate text-[12.5px] leading-[1.4] text-ash-400">
          {game ? game.label : t("share.pick.gameOff")}
        </span>
        {mode === "switch" && game && busy !== "free" ? (
          <span className="sr-only">{t("share.pick.sharing")}</span>
        ) : null}
      </span>
    </button>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <h3 className="px-1 text-[12.5px] font-semibold text-ash-300">{title}</h3>
      {children}
    </section>
  );
}

/** What the picker offers while its pictures are made. */
function Loading() {
  const { t } = useTranslation();
  return (
    <div
      role="status"
      className="flex flex-col gap-4"
      aria-busy="true"
      aria-label={t("share.pick.loading")}
    >
      <Skeleton className="h-[110px] w-full rounded-[var(--radius-panel)]" />
      <div className="grid grid-cols-3 gap-2">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="aspect-video w-full rounded-[6px]" />
        ))}
      </div>
    </div>
  );
}

/** The two choices on the surface, and "Gelişmiş" under them. */
export function ShareOptions({
  settings,
  audio,
  maxPreset,
  compact = false,
}: {
  settings: VoiceShareSettings;
  /** The computer can share an app's own sound. */
  audio: boolean;
  maxPreset: SharePreset;
  /** The settings page: the rows sit in its own list. */
  compact?: boolean;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const [open, setOpen] = useState(false);
  const advancedId = useId();
  const advanced = useRef<HTMLDivElement>(null);
  // Opened, the choices come into view (the picker's list scrolls above them).
  useEffect(() => {
    if (!open) return;
    const frame = requestAnimationFrame(() =>
      advanced.current?.scrollIntoView({ block: "nearest", behavior: reduce ? "auto" : "smooth" }),
    );
    return () => cancelAnimationFrame(frame);
  }, [open, reduce]);
  const presets = PRESETS.filter((p) => RANK[p] <= RANK[maxPreset]);
  const quality = RANK[settings.quality] > RANK[maxPreset] ? maxPreset : settings.quality;
  const fps = settings.fps === null ? "auto" : String(settings.fps);
  const row = "flex min-h-[36px] items-center justify-between gap-4";
  return (
    <div className={cn("flex flex-col", compact ? "gap-3" : "gap-2.5")}>
      <div className={row}>
        <span className="text-[13px] font-medium text-bone-100">{t("share.quality.label")}</span>
        <Segmented
          label={t("share.quality.label")}
          value={quality}
          onValueChange={(value) => setShareSetting("quality", value)}
          options={presets.map((value) => ({ value, label: t(`share.quality.${value}`) }))}
        />
      </div>
      <div className={row}>
        <Tooltip content={t("share.audio.unsupported")} disabled={audio}>
          <span className="flex min-w-0 flex-1">
            <Switch
              label={t("share.audio.label")}
              checked={audio && settings.audio}
              disabled={!audio}
              onCheckedChange={(on) => setShareSetting("audio", on)}
              className="w-full"
            />
          </span>
        </Tooltip>
      </div>
      <div>
        <button
          type="button"
          aria-expanded={open}
          aria-controls={advancedId}
          onClick={() => setOpen((v) => !v)}
          className="-mx-1 flex h-8 cursor-pointer items-center gap-1.5 rounded-[6px] px-1 text-[13px] font-medium text-ash-300 outline-none transition-colors duration-150 hover:text-bone-100 focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          {t("share.advanced.label")}
          <CaretDown
            size={13}
            weight="bold"
            aria-hidden
            className={cn("transition-transform duration-200 ease-out", open && "rotate-180")}
          />
        </button>
        <AnimatePresence initial={false}>
          {open ? (
            <motion.div
              key="advanced"
              ref={advanced}
              id={advancedId}
              className="flex flex-col gap-2.5 pt-1.5"
              initial={{ opacity: 0, transform: `translateY(${reduce ? 0 : -4}px)` }}
              animate={{
                opacity: 1,
                transform: "translateY(0px)",
                transition: { duration: 0.2, ease: [0.16, 1, 0.3, 1] },
              }}
              exit={{ opacity: 0, transition: { duration: 0.12 } }}
            >
              <div className={row}>
                <span className="text-[13px] text-bone-100">{t("share.advanced.motion")}</span>
                <Segmented
                  label={t("share.advanced.motion")}
                  value={settings.motion}
                  onValueChange={(value) => setShareSetting("motion", value)}
                  options={[
                    { value: "auto", label: t("share.advanced.motionAuto") },
                    { value: "clarity", label: t("share.advanced.clarity") },
                    { value: "motion", label: t("share.advanced.smooth") },
                  ]}
                />
              </div>
              <div className={row}>
                <span className="text-[13px] text-bone-100">{t("share.advanced.fps")}</span>
                <Segmented
                  label={t("share.advanced.fps")}
                  value={fps}
                  onValueChange={(value) =>
                    setShareSetting("fps", value === "auto" ? null : Number(value))
                  }
                  options={[
                    { value: "auto", label: t("share.advanced.fpsAuto") },
                    { value: "15", label: "15" },
                    { value: "30", label: "30" },
                    ...(quality === "high" ? [{ value: "60", label: "60" }] : []),
                  ]}
                />
              </div>
              <div className={row}>
                <span className="text-[13px] text-bone-100">{t("share.advanced.audioSource")}</span>
                <Segmented
                  label={t("share.advanced.audioSource")}
                  value={settings.audioSource}
                  disabled={!audio || !settings.audio}
                  onValueChange={(value) => setShareSetting("audioSource", value)}
                  options={[
                    { value: "app", label: t("share.advanced.audioApp") },
                    { value: "system", label: t("share.advanced.audioSystem") },
                  ]}
                />
              </div>
              <Switch
                label={t("share.advanced.cursor")}
                checked={settings.cursor}
                onCheckedChange={(on) => setShareSetting("cursor", on)}
              />
              <Switch
                label={t("share.advanced.thumbs")}
                checked={settings.thumbs ?? true}
                onCheckedChange={(on) => setShareSetting("thumbs", on)}
              />
              <Switch
                label={t("share.advanced.hideLauncher")}
                checked={settings.hideLauncher}
                onCheckedChange={(on) => setShareSetting("hideLauncher", on)}
              />
              <Switch
                label={t("share.advanced.quiet")}
                checked={settings.quiet}
                onCheckedChange={(on) => setShareSetting("quiet", on)}
              />
            </motion.div>
          ) : null}
        </AnimatePresence>
      </div>
    </div>
  );
}

export function SharePicker() {
  const { t } = useTranslation();
  const open = useShareUi((s) => s.picker);
  const close = useShareUi((s) => s.closePicker);
  const share = useShare();
  const switching = useOwnUi((s) => s.switching);
  const stored = useVoicePrefs((s) => s.settings?.share);
  const settings = { ...DEFAULT_SHARE_SETTINGS, ...stored };
  const [sources, setSources] = useState<ShareSources | null>(null);
  const [chosen, setChosen] = useState<{ source: ShareSource; label: string } | null>(null);
  // "Ekranı Değiştir": the share it switches (none: a new share).
  const target = switching === null ? null : (share.mine.find((m) => m.n === switching) ?? null);
  const mode = target ? "switch" : "start";
  const others = target ? share.mine.filter((m) => m.n !== target.n) : [];

  // A fresh list (and pictures) at every opening; the game is chosen at once while it runs (a
  // new share only: a switch goes as soon as something is pressed).
  // biome-ignore lint/correctness/useExhaustiveDependencies: the opening asks once; the mode is read then
  useEffect(() => {
    if (!open) return;
    let alive = true;
    setSources(null);
    setChosen(null);
    void shareApi
      .sources()
      .then((list) => {
        if (!alive) return;
        setSources(list);
        if (list.game && mode === "start") {
          setChosen({ source: list.game.source, label: list.game.label });
        }
      })
      .catch(() => {
        if (alive) setSources({ game: null, windows: [], screens: [], audio: false });
      });
    return () => {
      alive = false;
    };
  }, [open]);

  // Closed any way (Esc, "Vazgeç", a choice, the room ending): the next opening starts a new share
  // again, once the closing panel has faded (its words stay as they were meanwhile).
  useEffect(() => {
    if (open) return;
    const timer = setTimeout(endSwitch, SWITCH_FORGET_MS);
    return () => clearTimeout(timer);
  }, [open]);

  // Sharing went away meanwhile (the room ended), or the share being switched ended: the picker
  // goes with it (not before the state first came: a picker opened at the page's start waits).
  const wasOn = useRef(share.on);
  useEffect(() => {
    if (open && wasOn.current && !share.on) close();
    wasOn.current = share.on;
  }, [open, share.on, close]);
  useEffect(() => {
    if (open && switching !== null && !target) close();
  }, [open, switching, target, close]);

  const screenName = (info: ShareSourceInfo, index: number) =>
    info.label || t("share.pick.screen", { n: index + 1 });
  const busyOf = (info: ShareSourceInfo, label: string): Busy => {
    if (!target) return "free";
    if (showsSource(target, info, label)) return "current";
    return others.some((m) => showsSource(m, info, label)) ? "taken" : "free";
  };
  // One switch or one start per opening: the closing panel still takes the second press of a
  // double click (two switches, two shares).
  const acted = useRef(false);
  useEffect(() => {
    if (open) acted.current = false;
  }, [open]);
  const choose = (info: ShareSourceInfo, label: string) => {
    if (target) {
      if (!open || acted.current || busyOf(info, label) !== "free") return;
      acted.current = true;
      const n = target.n;
      close();
      void switchShare(n, info.source, label);
      return;
    }
    if (!open || acted.current) return;
    setChosen({ source: info.source, label });
  };
  const go = () => {
    if (!chosen || !open || acted.current) return;
    acted.current = true;
    close();
    void startShare(chosen.source, chosen.label, share.mineMax);
  };
  // The game first, then the windows, the screens last (one list in both modes).
  const choices = (list: ShareSources) => (
    <>
      <GameChoice
        game={list.game}
        chosen={chosen?.source.kind === "game"}
        mode={mode}
        busy={list.game ? busyOf(list.game, list.game.label) : "free"}
        onChoose={() => list.game && choose(list.game, list.game.label)}
      />
      <Section title={t("share.pick.windows")}>
        {list.windows.length ? (
          <div className="grid grid-cols-4 gap-1.5">
            {list.windows.map((info) => (
              <Choice
                key={"id" in info.source ? info.source.id : info.label}
                info={info}
                kind="window"
                label={info.label}
                chosen={sameSource(chosen?.source ?? null, info.source)}
                mode={mode}
                busy={busyOf(info, info.label)}
                onChoose={() => choose(info, info.label)}
              />
            ))}
          </div>
        ) : (
          <p className="px-1 text-[12.5px] text-ash-400">{t("share.pick.noWindows")}</p>
        )}
      </Section>
      {list.screens.length ? (
        <Section title={t("share.pick.screens")}>
          <p className="flex items-center gap-1.5 px-1 text-[12.5px] text-bone-100">
            <Warning size={14} weight="fill" aria-hidden className="shrink-0 text-gold-400" />
            {t("share.pick.screenWarn")}
          </p>
          <div className="grid grid-cols-4 gap-1.5">
            {list.screens.map((info, index) => (
              <Choice
                key={"id" in info.source ? info.source.id : String(index)}
                info={info}
                kind="screen"
                label={screenName(info, index)}
                meta={info.primary ? t("share.pick.primary") : null}
                chosen={sameSource(chosen?.source ?? null, info.source)}
                mode={mode}
                busy={busyOf(info, screenName(info, index))}
                onChoose={() => choose(info, screenName(info, index))}
              />
            ))}
          </div>
        </Section>
      ) : null}
    </>
  );

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && close()}
      title={mode === "switch" ? t("share.pick.switchTitle") : t("share.pick.title")}
      closeLabel={t("common.close")}
      width={640}
      footer={
        mode === "switch" ? (
          <div className="flex w-full justify-end">
            <Button variant="ghost" onClick={close}>
              {t("share.pick.cancel")}
            </Button>
          </div>
        ) : (
          // The choices stay in reach under the list, however long it is.
          <div className="flex w-full flex-col gap-3">
            <ShareOptions
              settings={settings}
              audio={sources?.audio ?? share.audio}
              maxPreset={share.maxPreset}
            />
            <div className="flex justify-end gap-2">
              <Button variant="ghost" onClick={close}>
                {t("share.pick.cancel")}
              </Button>
              <Button variant="primary" disabled={!chosen} onClick={go}>
                {t("share.pick.start")}
              </Button>
            </div>
          </div>
        )
      }
    >
      <div className="flex flex-col gap-5">
        {sources === null ? (
          <Loading />
        ) : mode === "start" ? (
          <div className="flex flex-col gap-5" role="radiogroup" aria-label={t("share.pick.title")}>
            {choices(sources)}
          </div>
        ) : (
          // biome-ignore lint/a11y/useSemanticElements: buttons that switch at once, not a form's fieldset
          <div
            className="flex flex-col gap-5"
            role="group"
            aria-label={t("share.pick.switchTitle")}
          >
            {choices(sources)}
          </div>
        )}
      </div>
    </Dialog>
  );
}
