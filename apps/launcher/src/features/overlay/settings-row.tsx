import { Warning } from "@phosphor-icons/react";
import { ARM_MS, cn, focusPanel, Kbd, SwapLabel, Switch, toast, usePressGuard } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { type MouseEvent, type ReactNode, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { officialStatusQuery } from "@/features/library/official";
import { useInstallOfficial } from "@/features/library/official-ui";
import { usePlayInstance } from "@/features/settings/graphics-section";
import { Row } from "@/features/settings/row";
import { api, type InstanceSummary } from "@/lib/api";
import { usePrefs } from "@/stores/prefs";
import { overlayApi } from "./api";
import { OVERLAY_LINES, type OverlayLineKind, overlayHome, overlayLine } from "./line";

/** Before the instance list is read. */
const NO_INSTANCES: InstanceSummary[] = [];

/** The overlay's default shortcut (the player may bind another in the game's Controls). */
const OVERLAY_KEY = "F7";

/** Where the key cap goes in the sentence that names the shortcut. */
const KEY_SLOT = "@@key@@";

/**
 * The longest install name a line shows in full. The lines never wrap (a second line would make
 * the cell taller for every line); with two names this long the widest, in English, is ~570 px,
 * inside the ~610 px the row's words have at the launcher's narrowest window (measured).
 */
const NAME_MAX = 16;

/** A long install name in a line: cut so the words around it stay ("Arkadaşlarla Sur… kurulumuna geç"). */
function shortName(name: string): string {
  return name.length > NAME_MAX ? `${name.slice(0, NAME_MAX - 1).trimEnd()}…` : name;
}

/**
 * Marks the settings row while the pointer is on a line that carries an action: Chromium hands a
 * label's :hover to the switch it names, and the switch would lift as if a click there flipped it
 * (`hovered:` in `@pl/ui` styles.css leaves a marked row out).
 */
function markRow(from: Element | null, on: boolean) {
  from?.closest(".switch-row")?.toggleAttribute("data-action-hover", on);
}

/**
 * Settings → Genel "Oyun içi katman" (OVL2, on by default): the overlay's mod, which the launcher
 * ships and puts in every Fabric instance on a version it supports (OVL-B), and a bridge to it at
 * each game start. Its one place: no mod list shows the mod. The line under it says how it opens;
 * while the switch is on but the overlay cannot work it says why instead, in the same cell, so
 * nothing below moves (`overlayLine`, the most basic blocker first): no install can carry it
 * (LIB1: the player deleted the official pack and has no Fabric install of a supported version),
 * with "Resmî paketi kur"; "Oyun açılınca launcher: Kapansın"; or the install Play opens is not
 * Fabric, or not a version the mod is made for (OVL-SEL1), with "Projects kurulumuna geç", which
 * selects the official pack or another install it works in for Play. A line with an action is not
 * the switch's label: a click beside the action does nothing. The shortcut itself is set in the
 * game's Controls.
 */
export function OverlayRow() {
  const { t } = useTranslation();
  const on = usePrefs((s) => s.inGameOverlay);
  const leaves = usePrefs((s) => s.onGameStart === "close");
  const set = usePrefs((s) => s.set);
  const selectInstance = usePrefs((s) => s.selectInstance);
  const { data: instances } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
  });
  const { data: official } = useQuery(officialStatusQuery);
  // The Minecraft versions the launcher ships the mod for (fixed for this build).
  const { data: overlay } = useQuery({
    queryKey: ["overlay-state"],
    queryFn: () => overlayApi.state(),
    staleTime: Number.POSITIVE_INFINITY,
  });
  // The install Play opens: the command bar's rule, the one "Grafik ve FPS" shows.
  const { instance: play, busy } = usePlayInstance(instances ?? NO_INSTANCES);
  const install = useInstallOfficial();
  const installing = install.isPending;
  const line = overlayLine({
    on,
    leaves,
    instances,
    official,
    versions: overlay?.versions,
    play,
    locked: busy,
  });
  // The install the switch selects: the same on both install lines and whichever install Play
  // opens, so the hidden lines carry it too and it is in place before its line shows (a switch
  // that came with its line under a resting pointer must not take that pointer's double click).
  const home =
    instances && overlay ? overlayHome(instances, official, overlay.versions) : undefined;
  // The install the install lines named last: fading out after a switch, they keep their words.
  // Before they ever showed, Play's own: the cell is as tall as it will be.
  const named = useRef({ name: "", version: "" });
  if ((line.kind === "notFabric" || line.kind === "version") && play) {
    named.current = { name: play.name, version: play.mcVersion };
  }
  const shownName = shortName(named.current.name || (play?.name ?? ""));
  const shownVersion = named.current.version || (play?.mcVersion ?? "");
  const hasAction = (kind: OverlayLineKind) =>
    kind === "noHome" || ((kind === "notFabric" || kind === "version") && home !== undefined);
  const grid = useRef<HTMLSpanElement>(null);
  const words = useLineWords(hasAction(line.kind));
  // A line that goes takes the pointer's mark with it (no pointerleave reaches an inert line).
  const markedFor = useRef(line.kind);
  useEffect(() => {
    if (markedFor.current === line.kind) return;
    markedFor.current = line.kind;
    markRow(grid.current, false);
  }, [line.kind]);

  const sentence: Record<OverlayLineKind, string> = {
    opens: t("overlay.settingHelp", { key: OVERLAY_KEY }),
    noHome: t("overlay.packMissing"),
    closes: t("overlay.closeHelp"),
    notFabric: t("overlay.fabricOnly", { name: shownName }),
    version: t("overlay.versionMissing", { version: shownVersion }),
  };
  /** The sentence as shown: the shortcut as a key cap. */
  const shownSentence = (kind: OverlayLineKind): ReactNode => {
    if (kind !== "opens") return sentence[kind];
    const [before, after] = t("overlay.settingHelp", { key: KEY_SLOT }).split(KEY_SLOT);
    return (
      <>
        {before}
        <Kbd>{OVERLAY_KEY}</Kbd>
        {after}
      </>
    );
  };

  /** The action a line carries: "Resmî paketi kur", or the switch to an install it works in. */
  const action = (kind: OverlayLineKind, active: boolean): ReactNode => {
    if (kind === "noHome") {
      return (
        <LineAction
          shown={active}
          name={t("lib1.install")}
          busy={installing}
          onPress={(refocus) =>
            install.mutate(undefined, {
              onSuccess: (made) => {
                toast.success(t("official.added", { name: made.name }));
                refocus();
              },
            })
          }
        >
          <SwapLabel
            labels={[t("lib1.install"), t("lib1.installing")]}
            active={installing ? 1 : 0}
          />
        </LineAction>
      );
    }
    // Nothing else carries the overlay: the line says why, with no action.
    if (!hasAction(kind) || !home) return null;
    const short = shortName(home.name);
    return (
      <LineAction
        shown={active && !busy}
        // A game is prepared or open: the command bar's picker is locked; the line says why the
        // overlay is not in that game, and the switch fades out until it ends.
        faded={busy}
        // The whole name when the words cut it.
        name={short === home.name ? undefined : t("overlay.switchTo", { name: home.name })}
        onPress={(refocus) => {
          refocus();
          selectInstance(home.id);
        }}
      >
        {t("overlay.switchTo", { name: short })}
      </LineAction>
    );
  };

  return (
    <Row
      title={t("overlay.setting")}
      help={
        <>
          {/* Heard when the switch, the close choice, the pack or the install changes the line;
              first, so the switch's description reads the line before its action. */}
          <span className="sr-only" aria-live="polite">
            {sentence[line.kind]}
          </span>
          {/* As `SwapLine` does, with an action in a line: hidden lines are inert. */}
          <span ref={grid} className="grid" {...words}>
            {OVERLAY_LINES.map((kind) => {
              const active = kind === line.kind;
              const withAction = hasAction(kind);
              return (
                <span
                  key={kind}
                  inert={!active}
                  // A line with an action is not the switch's label (`useLineWords`): no hand, and
                  // the switch does not lift under the pointer.
                  onPointerEnter={
                    withAction ? (event) => markRow(event.currentTarget, true) : undefined
                  }
                  onPointerLeave={
                    withAction ? (event) => markRow(event.currentTarget, false) : undefined
                  }
                  // One line each, never wrapped: a second line would make the cell taller for
                  // every line (the row's words have room past the help's measure).
                  className={cn(
                    "[grid-area:1/1] whitespace-nowrap transition-opacity",
                    withAction && "cursor-default",
                    active ? "opacity-100 delay-[60ms] duration-150" : "opacity-0 duration-100",
                  )}
                >
                  {/* A line that says it will not work leads with the launcher's warning mark
                      (gold, as in the help panel and the mod center). */}
                  {kind === "opens" ? null : (
                    <Warning
                      size={13}
                      weight="fill"
                      aria-hidden
                      className="mr-1.5 inline-block align-[-2px] text-gold-300"
                    />
                  )}
                  <span aria-hidden>{shownSentence(kind)}</span>
                  {action(kind, active)}
                </span>
              );
            })}
          </span>
        </>
      }
    >
      <Switch
        checked={on}
        onCheckedChange={(v) => set("inGameOverlay", v)}
        label={t("overlay.setting")}
        hideLabel
      />
    </Row>
  );
}

/**
 * The line's words belong to the row's label, which flips the switch. A line with an action does
 * not: a click a little beside "Resmî paketi kur" must not turn the overlay off. After a press on
 * an action its line goes (at once for a switch, once the pack is made for "Kur"), and the words
 * that take its place must not take the rest of that double click, nor a press while the pointer
 * still rests there (`ARM_MS`), as with every control that takes another's place (`usePressGuard`).
 */
function useLineWords(actionLine: boolean) {
  const pressedAt = useRef<number | null>(null);
  return {
    onClick(event: MouseEvent<HTMLElement>) {
      if (event.target instanceof Element && event.target.closest("button")) {
        // A line's action (it keeps the switch still itself). The keyboard has no double click.
        pressedAt.current = event.detail > 0 ? performance.now() : null;
        return;
      }
      if (actionLine) {
        event.preventDefault();
        return;
      }
      const at = pressedAt.current;
      if (at === null) return;
      if (event.detail >= 2 || performance.now() - at < ARM_MS) {
        event.preventDefault();
        return;
      }
      // A press of the words' own: they flip the switch again from now on.
      pressedAt.current = null;
    },
    onPointerLeave(event: { pointerType: string }) {
      if (event.pointerType !== "touch") pressedAt.current = null;
    },
  };
}

/**
 * The focus after a line's action, which goes with its line: the keyboard stays in the row, on its
 * switch (the label's control), once the key that pressed the action is up (a held Enter would
 * flip the switch). Only while the focus is still on the action or fell to the page with it.
 */
function focusRowSwitch(from: HTMLElement, keyHeld: boolean) {
  const target = from.closest("label")?.control;
  if (!(target instanceof HTMLElement)) return;
  const move = () => {
    const active = document.activeElement;
    if (active === from || active === null || active === document.body) focusPanel(target);
  };
  if (keyHeld) window.addEventListener("keyup", move, { once: true });
  else move();
}

/**
 * A line's action: gold words right after its sentence ("Resmî paketi kur", "Projects kurulumuna
 * geç"), reaching a little past them (`hit-area`). It comes with its line, maybe under a pointer
 * that was pressing the row's words: it ignores the rest of that double click and presses while
 * the pointer rests there for a moment (`usePressGuard`).
 */
function LineAction({
  shown,
  faded = false,
  name,
  busy = false,
  onPress,
  children,
}: {
  /** Its line shows and the action can be taken. */
  shown: boolean;
  /** Its line shows but the action waits (a game is open): it fades out in place until then. */
  faded?: boolean;
  /** The name a screen reader says when the words shown are not the whole of it. */
  name?: string;
  /** Its work is under way: a press does nothing more. */
  busy?: boolean;
  /** The press; `refocus` keeps the keyboard's focus in the row once the line is gone. */
  onPress: (refocus: () => void) => void;
  children: ReactNode;
}) {
  const guard = usePressGuard(shown ? "shown" : "hidden");
  // Enter presses on its way down (Space on its way up): the switch takes the focus after it.
  const keyDown = useRef(false);
  return (
    <button
      type="button"
      inert={faded || undefined}
      aria-label={name}
      aria-busy={busy || undefined}
      // The words start right after the sentence (`SwapLabel` centres them). The underline fades
      // in under the pointer; a press squeezes them as it does a button. The wider target only
      // while it shows: Chromium reads a hidden line's action into the switch's description when
      // it has the pseudo-element (measured).
      className={cn(
        "relative ml-1.5 cursor-pointer rounded-sm font-medium text-gold-300 underline decoration-transparent underline-offset-2 outline-none transition-[opacity,text-decoration-color,scale] duration-150 hover:decoration-current focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.97] [&_.swap-label]:justify-items-start",
        faded ? "opacity-0" : "opacity-100",
        shown && "hit-area",
      )}
      onPointerLeave={guard.handlers.onPointerLeave}
      onKeyDown={(event) => {
        guard.handlers.onKeyDown(event);
        keyDown.current = true;
      }}
      onKeyUp={() => {
        keyDown.current = false;
      }}
      onClick={(event) => {
        // Inside the row's label: the switch must not flip with it.
        event.preventDefault();
        if (!guard.take(event) || busy || faded) return;
        const button = event.currentTarget;
        const byKeyboard = event.detail === 0;
        onPress(() => {
          if (byKeyboard) focusRowSwitch(button, keyDown.current);
        });
      }}
    >
      {children}
    </button>
  );
}
