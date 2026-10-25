import { ARM_MS, cn, focusPanel, SwapLabel, Switch, toast, usePressGuard } from "@pl/ui";
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

/**
 * The longest install name the switch shows in full: with the longest sentence ("Minecraft
 * 1.21.11 sürümünde çalışmaz.") the line still fits the help's width (measured: 406 of 449 px).
 */
const NAME_MAX = 16;

/** A long install name in the switch: cut so the verb stays ("Arkadaşlarla Sur… kurulumuna geç"). */
function shortName(name: string): string {
  return name.length > NAME_MAX ? `${name.slice(0, NAME_MAX - 1).trimEnd()}…` : name;
}

/**
 * Settings → Genel "Oyun içi katman" (OVL2, on by default): the overlay's mod, which the launcher
 * ships and puts in every Fabric instance on a version it supports (OVL-B), and a bridge to it at
 * each game start. Its one place: no mod list shows the mod. The line under it says how it opens;
 * while the switch is on but the overlay cannot work it says why instead, in the same cell, so
 * nothing below moves (`overlayLine`, the most basic blocker first): no instance can carry it
 * (LIB1: the player deleted the official pack and has no Fabric instance of a supported version),
 * with "Kur" as the library offers it; "Oyun açılınca launcher: Kapansın"; or the install Play
 * opens is not Fabric, or not a version the mod is made for (OVL-SEL1), with "Projects kurulumuna
 * geç", which selects the official pack or another install it works in for Play. The shortcut
 * itself is set in the game's Controls.
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
  // The version the `version` line named last: fading out after a switch, it keeps its words.
  const shownVersion = useRef("");
  if (line.kind === "version" && play) shownVersion.current = play.mcVersion;
  const words = useLineWords();

  const sentence: Record<OverlayLineKind, string> = {
    opens: t("overlay.settingHelp"),
    noHome: t("overlay.packMissing"),
    closes: t("overlay.closeHelp"),
    notFabric: t("overlay.fabricOnly"),
    // Before it ever showed, the install's own version: the cell is as tall as it will be.
    version: t("overlay.versionMissing", {
      version: shownVersion.current || (play?.mcVersion ?? ""),
    }),
  };

  /** The action a line carries: "Kur", or the switch to an install the overlay works in. */
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
            labels={[t("lib1.installShort"), t("lib1.installing")]}
            active={installing ? 1 : 0}
          />
        </LineAction>
      );
    }
    // Nothing else carries the overlay: the line says why, with no action.
    if ((kind !== "notFabric" && kind !== "version") || !home) return null;
    const short = shortName(home.name);
    return (
      <LineAction
        shown={active && !busy}
        // A game is prepared or open: the command bar's picker is locked, and so is this.
        disabled={busy}
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
          <span className="grid" {...words}>
            {OVERLAY_LINES.map((kind) => {
              const active = kind === line.kind;
              return (
                <span
                  key={kind}
                  inert={!active}
                  className={cn(
                    "[grid-area:1/1] transition-opacity",
                    active ? "opacity-100 delay-[60ms] duration-150" : "opacity-0 duration-100",
                  )}
                >
                  <span aria-hidden>{sentence[kind]}</span>
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
 * The line's words belong to the row's label, which flips the switch. After a press on a line's
 * action its line goes (at once for a switch, once the pack is made for "Kur"), and the words that
 * take its place must not take the rest of that double click, nor a press while the pointer still
 * rests there (`ARM_MS`), as with every control that takes another's place (`usePressGuard`).
 */
function useLineWords() {
  const pressedAt = useRef<number | null>(null);
  return {
    onClick(event: MouseEvent<HTMLElement>) {
      if (event.target instanceof Element && event.target.closest("button")) {
        // A line's action (it keeps the switch still itself). The keyboard has no double click.
        pressedAt.current = event.detail > 0 ? performance.now() : null;
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
 * Marks the settings row while the pointer is on an action in its words: Chromium hands a label's
 * :hover to the switch it names, and the switch lifts as if a click there would flip it
 * (`hovered:` in `@pl/ui` styles.css leaves a marked row out).
 */
function markRow(from: Element | null, on: boolean) {
  from?.closest(".switch-row")?.toggleAttribute("data-action-hover", on);
}

/**
 * A line's action: gold words right after its sentence ("Kur", "Projects kurulumuna geç"). It
 * comes with its line, maybe under a pointer that was pressing the row's words: it ignores the rest
 * of that double click and presses while the pointer rests there for a moment (`usePressGuard`).
 */
function LineAction({
  shown,
  disabled = false,
  name,
  busy = false,
  onPress,
  children,
}: {
  /** Its line shows and the action can be taken. */
  shown: boolean;
  /** Its line shows but the action waits (a game is open): dimmed in place. */
  disabled?: boolean;
  /** The name a screen reader says when the words shown are not the whole of it. */
  name?: string;
  /** Its work is under way: a press does nothing more. */
  busy?: boolean;
  /** The press; `refocus` keeps the keyboard's focus in the row once the line is gone. */
  onPress: (refocus: () => void) => void;
  children: ReactNode;
}) {
  const guard = usePressGuard(shown ? "shown" : "hidden");
  const ref = useRef<HTMLButtonElement>(null);
  // Enter presses on its way down (Space on its way up): the switch takes the focus after it.
  const keyDown = useRef(false);
  // No longer under the pointer once it hides or waits (no pointerleave reaches an inert one).
  useEffect(() => {
    if (!shown) markRow(ref.current, false);
  }, [shown]);
  useEffect(() => {
    const row = ref.current;
    return () => markRow(row, false);
  }, []);
  return (
    <button
      ref={ref}
      type="button"
      disabled={disabled}
      aria-label={name}
      aria-busy={busy || undefined}
      // The words start right after the sentence (`SwapLabel` centres them). The underline fades
      // in under the pointer; a press squeezes them as it does a button; waiting, they dim.
      className="ml-1.5 cursor-pointer rounded-sm font-medium text-gold-300 underline decoration-transparent underline-offset-2 outline-none transition-[opacity,text-decoration-color,scale] duration-150 focus-visible:ring-2 focus-visible:ring-gold-300 enabled:hover:decoration-current enabled:active:scale-[0.97] disabled:cursor-default disabled:opacity-50 [&_.swap-label]:justify-items-start"
      onPointerEnter={(event) => markRow(event.currentTarget, true)}
      onPointerLeave={(event) => {
        guard.handlers.onPointerLeave(event);
        markRow(event.currentTarget, false);
      }}
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
        if (!guard.take(event) || busy || disabled) return;
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
