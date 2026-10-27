// The account panel's Hareket tab (CAFK-FEAT §3.7, README "Movement"): where the account stands and
// looks, what it is doing, direction buttons relative to where it looks (a tap is one block; held,
// eight now and again every 1.5 s, and a stop on release; W, A, S, D while the panel has focus),
// jump and sneak, a walk to coordinates, and going to or following the player's own accounts the
// service names (never a typed name). It streams only while the tab is on screen. The words are the
// contract's, the same as the phone's.
import {
  ArrowDown,
  ArrowLeft,
  ArrowLineUp,
  ArrowRight,
  ArrowUp,
  Crosshair,
  HandPalm,
  type Icon,
  PersonSimpleWalk,
  UserFocus,
} from "@phosphor-icons/react";
import { Button, ContentIcon, cn, EmptyState, enter, Input, leave, Switch } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import {
  Fragment,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import type { CloudAccount } from "../api";
import { createHold } from "./hold";
import {
  coordinate,
  dimension,
  distance,
  doingKey,
  endedKey,
  facingKey,
  GOTO_MAX_BLOCKS,
  isGoing,
  type MoveDir,
  type MoveOp,
  offKey,
  refusalKey,
  type WordKey,
} from "./model";
import { useCloudMovement } from "./use-cloud-movement";

/** A refusal shows under the activity line for this long. */
const REFUSAL_MS = 4000;

const KEYS: Record<string, MoveDir> = {
  KeyW: "forward",
  KeyA: "left",
  KeyS: "back",
  KeyD: "right",
};
const KEY_OF: Record<MoveDir, string> = {
  forward: "KeyW",
  left: "KeyA",
  back: "KeyS",
  right: "KeyD",
};

/** The letters on the player's own keyboard at the W, A, S and D places (a Turkish F is different). */
function useKeyLabels(lang: string) {
  const [labels, setLabels] = useState<Record<string, string>>({
    KeyW: "W",
    KeyA: "A",
    KeyS: "S",
    KeyD: "D",
  });
  useEffect(() => {
    const keyboard = (
      navigator as Navigator & {
        keyboard?: { getLayoutMap?: () => Promise<Map<string, string>> };
      }
    ).keyboard;
    let live = true;
    void keyboard
      ?.getLayoutMap?.()
      .then((map) => {
        if (!live) return;
        const next: Record<string, string> = {};
        for (const code of Object.keys(KEYS)) {
          const key = map.get(code);
          next[code] = key ? key.toLocaleUpperCase(lang) : code.slice(3);
        }
        setLabels(next);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [lang]);
  return labels;
}

function Quiet({ text, action }: { text: string; action?: ReactNode }) {
  return (
    <EmptyState
      icon={<ContentIcon kind="movement" size={32} />}
      title={text}
      action={action}
      className="py-14"
    />
  );
}

/**
 * One chamfered pad button: its arrow over its word, its key in the corner. The jump is the wide bar
 * under the four, its arrow beside its word, as the space bar sits under W, A, S and D.
 */
function PadButton({
  icon: Glyph,
  label,
  keyLabel,
  keyShortcut,
  wide,
  held,
  disabled,
  onPointerDown,
  onPointerUp,
  onPointerCancel,
  onClick,
  className,
}: {
  icon: Icon;
  label: string;
  keyLabel?: string;
  /** `aria-keyshortcuts` when the key's name is not its printed letter (Space). */
  keyShortcut?: string;
  wide?: boolean;
  held?: boolean;
  disabled?: boolean;
  onPointerDown?: (event: PointerEvent<HTMLButtonElement>) => void;
  onPointerUp?: (event: PointerEvent<HTMLButtonElement>) => void;
  onPointerCancel?: () => void;
  onClick?: (event: React.MouseEvent<HTMLButtonElement>) => void;
  className?: string;
}) {
  return (
    <button
      type="button"
      data-pad=""
      aria-label={label}
      aria-keyshortcuts={keyShortcut ?? keyLabel}
      data-held={held ? "" : undefined}
      disabled={disabled}
      onPointerDown={onPointerDown}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerCancel}
      onLostPointerCapture={onPointerCancel}
      onClick={onClick}
      onContextMenu={(event) => event.preventDefault()}
      className={cn(
        "group relative isolate flex cursor-pointer touch-none items-center justify-center select-none [--cut:8px]",
        wide ? "flex-row gap-2" : "flex-col gap-1.5",
        "transition-[transform,scale,opacity] duration-150 ease-out active:scale-[0.97] data-[held]:scale-[0.97]",
        "focus-visible:outline-none disabled:pointer-events-none disabled:opacity-45",
        className,
      )}
    >
      <span
        aria-hidden
        className="chamfer-ring absolute -z-20 bg-gold-300 opacity-0 transition-opacity group-focus-visible:opacity-100"
      />
      <span
        aria-hidden
        className="chamfer absolute inset-0 -z-10 bg-white/12 transition-colors duration-150 group-hover:bg-white/20 group-data-[held]:bg-gold-500/70"
      />
      <span
        aria-hidden
        className="chamfer absolute inset-px -z-10 bg-obsidian-800/85 transition-colors duration-150 group-hover:bg-obsidian-700/90 group-data-[held]:bg-[color-mix(in_oklab,var(--color-gold-500)_16%,var(--color-obsidian-800))]"
      />
      <Glyph
        size={wide ? 18 : 20}
        weight="bold"
        aria-hidden
        className="text-bone-100 transition-colors duration-150 group-data-[held]:text-gold-300"
      />
      <span className="text-[12px] leading-none font-medium text-ash-300 transition-colors duration-150 group-hover:text-bone-100">
        {label}
      </span>
      {keyLabel ? (
        <span
          aria-hidden
          className={cn(
            "absolute text-[11px] leading-none font-semibold text-ash-400",
            wide ? "top-1/2 right-3 -translate-y-1/2" : "top-1.5 right-2",
          )}
        >
          {keyLabel}
        </span>
      ) : null}
    </button>
  );
}

/**
 * A walk the service allows, on a secondary button's quiet plate: its icon and its words, then the
 * main account's name (another account's name is in its own words).
 */
function WalkRow({
  icon: Glyph,
  label,
  name,
  disabled,
  onClick,
}: {
  icon: Icon;
  label: string;
  name?: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-label={name ? `${label}, ${name}` : undefined}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "group relative isolate flex h-[32px] w-full min-w-0 cursor-pointer items-center gap-2.5 px-3 text-left select-none [--cut:6px]",
        "transition-[transform,scale,opacity] duration-150 ease-out active:scale-[0.985] focus-visible:outline-none",
        "disabled:pointer-events-none disabled:opacity-45",
      )}
    >
      <span
        aria-hidden
        className="chamfer-ring absolute -z-20 bg-gold-300 opacity-0 transition-opacity group-focus-visible:opacity-100"
      />
      <span
        aria-hidden
        className="chamfer absolute inset-0 -z-10 bg-white/12 transition-colors duration-150 group-hover:bg-white/20"
      />
      <span
        aria-hidden
        className="chamfer absolute inset-px -z-10 bg-obsidian-800/85 transition-colors duration-150 group-hover:bg-obsidian-700/90"
      />
      <Glyph
        size={16}
        aria-hidden
        className="shrink-0 text-ash-300 transition-colors duration-150 group-hover:text-gold-300"
      />
      <span className="min-w-0 truncate text-[13px] font-medium text-bone-100">{label}</span>
      {name ? <span className="min-w-0 truncate text-[12.5px] text-ash-400">{name}</span> : null}
    </button>
  );
}

export function MovementPanel({
  account,
  active,
  link,
  onReconnect,
  onUnavailable,
}: {
  account: CloudAccount;
  /** The tab is on screen: movement streams only then. */
  active: boolean;
  /** The console's reconnect count: the stream is asked for again with it. */
  link: number;
  onReconnect: () => void;
  /** The service cannot move it after all (Studio switched it off): the tab goes. */
  onUnavailable: () => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language === "en" ? "en" : "tr";
  const { state, send } = useCloudMovement(account, active, link);
  const keyLabels = useKeyLabels(lang);
  const follow = account.features?.movement?.follow;
  const main = follow?.main ?? null;
  const own = follow?.own ?? [];
  const move = state.move;
  const pos = state.pos;
  const ok = move?.state === "ok";
  const locked = ok && move?.lock === "trade";
  const inGame = account.live.state === "running";

  const [refusal, setRefusal] = useState<string | null>(null);
  const [held, setHeld] = useState<MoveDir | null>(null);
  // Typed axes only: an empty field means "where it stands now" (its placeholder, the live figure).
  const [coords, setCoords] = useState({ x: "", y: "", z: "" });
  const [invalid, setInvalid] = useState<Set<"x" | "y" | "z">>(new Set());
  const askedName = useRef<string | null>(null);
  const coordId = useId();
  const refusalTimer = useRef<number | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const controlsRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (move?.state === "off" && move.reason === "service") onUnavailable();
  }, [move?.state, move?.reason, onUnavailable]);

  const words = useCallback(
    (word: WordKey | null) => (word ? t(`cloudAfkMove.${word.key}`, word.values) : null),
    [t],
  );

  const refuse = useCallback((text: string) => {
    setRefusal(text);
    if (refusalTimer.current !== null) window.clearTimeout(refusalTimer.current);
    refusalTimer.current = window.setTimeout(() => setRefusal(null), REFUSAL_MS);
  }, []);
  useEffect(
    () => () => {
      if (refusalTimer.current !== null) window.clearTimeout(refusalTimer.current);
    },
    [],
  );

  const act = useCallback(
    async (op: MoveOp, name?: string) => {
      if (name) askedName.current = name;
      const result = await send(op);
      if (result.ok) return;
      const text = words(refusalKey(result.error, name ?? askedName.current, main));
      if (text) refuse(text);
    },
    [send, words, main, refuse],
  );

  // The latest `act` for the hold's timers, so a held button never sends with an old account.
  const actRef = useRef(act);
  actRef.current = act;
  const hold = useMemo(
    () =>
      createHold((op) => {
        void actRef.current(op);
      }),
    [],
  );
  const press = useCallback(
    (dir: MoveDir) => {
      hold.press(dir);
      setHeld(hold.active);
    },
    [hold],
  );
  const release = useCallback(
    (dir?: MoveDir) => {
      hold.release(dir);
      setHeld(hold.active);
    },
    [hold],
  );
  const cancel = useCallback(() => {
    hold.cancel();
    setHeld(null);
  }, [hold]);

  // A press is lost when the window or the tab goes away (no key-up comes then).
  useEffect(() => {
    if (!active) cancel();
    window.addEventListener("blur", cancel);
    return () => {
      window.removeEventListener("blur", cancel);
      cancel();
    };
  }, [active, cancel]);
  // It cannot move any more (a trade, off): a held direction stops.
  useEffect(() => {
    if (!ok || locked) cancel();
  }, [ok, locked, cancel]);
  // A trade makes the controls inert: a focus that was in them stays in the panel, so W, A, S, D
  // work again the moment the trade ends.
  useEffect(() => {
    if (!locked) return;
    const now = document.activeElement;
    if (!now || now === document.body || controlsRef.current?.contains(now)) {
      rootRef.current?.focus({ preventScroll: true });
    }
  }, [locked]);

  // Space is the pad's own key on the panel and its buttons (a jump, as in the game); anywhere else
  // in the panel it does what it always does.
  const padKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    return target === event.currentTarget || target.closest("[data-pad]") !== null;
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.altKey || event.metaKey) return;
    if (event.code === "Space") {
      if (!padKey(event)) return;
      event.preventDefault();
      if (!event.repeat && ok && !locked) void act({ action: "jump" });
      return;
    }
    const dir = KEYS[event.code];
    if (!dir) return;
    const target = event.target as HTMLElement;
    if (target.closest("input, textarea, select, [contenteditable=true]")) return;
    event.preventDefault();
    if (event.repeat || !ok || locked) return;
    press(dir);
  };
  const onKeyUp = (event: KeyboardEvent<HTMLDivElement>) => {
    // A focused pad button never clicks on Space's release: the key-down jumped already.
    if (event.code === "Space") {
      if (padKey(event)) event.preventDefault();
      return;
    }
    const dir = KEYS[event.code];
    if (dir) release(dir);
  };

  const direction = (dir: MoveDir, glyph: Icon, className: string) => (
    <PadButton
      icon={glyph}
      label={t(`cloudAfkMove.pad.${dir}`)}
      keyLabel={keyLabels[KEY_OF[dir]]}
      held={held === dir}
      disabled={!ok}
      className={className}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        try {
          // The release comes to this button even when the pointer slid off it.
          event.currentTarget.setPointerCapture(event.pointerId);
        } catch {
          // A pointer the browser no longer tracks: the press still counts.
        }
        press(dir);
      }}
      onPointerUp={() => release(dir)}
      onPointerCancel={() => {
        if (hold.active === dir) cancel();
      }}
      onClick={(event) => {
        // Enter or Space on a focused button: one step (pointer presses went through the hold).
        if (event.detail === 0) void act({ action: "step", dir, n: 1 });
      }}
    />
  );

  const go = () => {
    const axis = (name: "x" | "y" | "z") =>
      coords[name].trim() === "" ? (pos ? pos[name] : null) : coordinate(coords[name], name);
    const parsed = { x: axis("x"), y: axis("y"), z: axis("z") };
    const bad = new Set((["x", "y", "z"] as const).filter((axis) => parsed[axis] === null));
    setInvalid(bad);
    if (bad.size || parsed.x === null || parsed.y === null || parsed.z === null) return;
    const to = { x: parsed.x, y: parsed.y, z: parsed.z };
    if (pos && distance(pos, to) > GOTO_MAX_BLOCKS) {
      // The typed target stays marked until it changes (the line's words go after 4 s).
      setInvalid(new Set((["x", "y", "z"] as const).filter((axis) => coords[axis].trim() !== "")));
      refuse(t("cloudAfkMove.far"));
      return;
    }
    void act({ action: "goto", ...to });
  };

  // What holds it, from the account first (no need to wait for the machine), then the machine's word.
  const reason =
    move?.state === "off" ? (move.reason ?? "service") : !inGame && !ok ? "not_running" : null;
  if (reason === "restart") {
    return (
      <Quiet
        text={t("cloudAfkMove.restart")}
        action={
          <Button size="sm" variant="secondary" onClick={onReconnect}>
            {t("cloudAfkMove.reconnect")}
          </Button>
        }
      />
    );
  }
  if (reason) return <Quiet text={t(`cloudAfkMove.${offKey(reason)}`)} />;

  const doing = words(doingKey(move));
  const ended =
    move?.doing === "idle"
      ? words(endedKey(move.ended, move.name ?? askedName.current, main))
      : null;
  const going = isGoing(move);
  const activity = doing ?? (ok && move?.doing === "step" ? t("cloudAfkMove.stepping") : null);
  const rest = ended ?? (ok && move?.doing === "idle" ? t("cloudAfkMove.idle") : null);
  const line = refusal
    ? { text: refusal, tone: "text-flare-400" }
    : activity
      ? { text: activity, tone: locked ? "text-bone-100" : "text-gold-300" }
      : rest
        ? { text: rest, tone: "text-ash-300" }
        : null;
  const dim = pos ? dimension(pos.dim) : null;
  const dimText = dim ? ("key" in dim ? t(`cloudAfkMove.dims.${dim.key}`) : dim.text) : "";
  const facing = pos
    ? t("cloudAfkMove.facing", { dir: t(`cloudAfkMove.directions.${facingKey(pos.facing)}`) })
    : "";
  const figure = (axis: "x" | "y" | "z") => (
    <div className="flex min-w-0 flex-col gap-1">
      <span className="text-[12px] leading-none font-medium text-ash-400">
        {axis.toUpperCase()}
      </span>
      <span className="font-display text-[20px] leading-none font-semibold text-bone-50 tabular-nums">
        {pos ? pos[axis] : " "}
      </span>
    </div>
  );

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: W, A, S, D move the account while the panel has focus
    <div
      ref={rootRef}
      data-movement-panel=""
      tabIndex={-1}
      className="flex flex-col gap-3 pb-4 outline-none"
      onKeyDown={onKeyDown}
      onKeyUp={onKeyUp}
    >
      <section
        aria-label={t("cloudAfkMove.position")}
        className="flex items-center gap-6 rounded-[var(--radius-panel)] bg-white/[0.03] px-4 py-3 ring-1 ring-white/7"
      >
        <div className="grid flex-1 grid-cols-3 gap-4">
          {figure("x")}
          {figure("y")}
          {figure("z")}
        </div>
        <div className="flex shrink-0 flex-col items-end gap-1 text-right">
          <span className="min-h-[18px] text-[13.5px] leading-[18px] font-medium text-bone-100">
            {dimText}
          </span>
          <span className="min-h-[16px] text-[12.5px] leading-4 text-ash-300">{facing}</span>
        </div>
      </section>

      {/* One reserved line, nothing below moves: a refusal (4 s), else what it is doing, else how the
          last walk ended, else that it stands; "Dur" at its end while it walks. Words cross-fade in
          place. */}
      <div className="flex min-h-8 items-center gap-3">
        <div aria-live="polite" className="grid min-w-0 flex-1">
          <AnimatePresence initial={false}>
            {line ? (
              <motion.p
                key={line.text}
                className={cn("[grid-area:1/1] truncate text-[13.5px] leading-5", line.tone)}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: enter(0.15) }}
                exit={{ opacity: 0, transition: leave(0.1) }}
              >
                {line.text}
              </motion.p>
            ) : null}
          </AnimatePresence>
        </div>
        <Button
          size="sm"
          variant="secondary"
          icon={<HandPalm size={15} weight="bold" />}
          onClick={() => {
            cancel();
            void act({ action: "stop" });
          }}
          tabIndex={going ? undefined : -1}
          aria-hidden={going ? undefined : true}
          className={cn(
            "transition-[opacity,transform,scale] duration-150",
            going ? "opacity-100" : "pointer-events-none opacity-0",
          )}
        >
          {t("cloudAfkMove.stop")}
        </Button>
      </div>

      {/* A trade holds every control at once: one even dim, nothing inside reachable. */}
      <div
        ref={controlsRef}
        inert={locked}
        className={cn(
          "grid grid-cols-[auto_minmax(0,1fr)] items-start gap-6 transition-opacity duration-150",
          locked && "opacity-55",
        )}
      >
        <div className="flex flex-col items-center gap-4">
          <fieldset
            aria-label={t("cloudAfkMove.pad.label")}
            className="m-0 grid min-w-0 grid-cols-[repeat(3,60px)] grid-rows-[56px_56px_40px] gap-1.5 border-0 p-0"
          >
            {/* The keys' own shape: W over A, S, D, and the space bar under them. */}
            {direction("forward", ArrowUp, "col-start-2 row-start-1")}
            {direction("left", ArrowLeft, "col-start-1 row-start-2")}
            {direction("back", ArrowDown, "col-start-2 row-start-2")}
            {direction("right", ArrowRight, "col-start-3 row-start-2")}
            <PadButton
              wide
              icon={ArrowLineUp}
              label={t("cloudAfkMove.pad.jump")}
              keyLabel={t("cloudAfkMove.pad.space")}
              keyShortcut="Space"
              disabled={!ok}
              className="col-span-3 row-start-3"
              onClick={() => void act({ action: "jump" })}
            />
          </fieldset>
          <Switch
            label={t("cloudAfkMove.pad.sneak")}
            checked={Boolean(move?.sneak)}
            disabled={!ok}
            onCheckedChange={(on) => void act({ action: "sneak", on })}
          />
        </div>

        <div className="flex min-w-0 flex-col gap-4">
          <form
            aria-label={t("cloudAfkMove.goto.title")}
            className="flex flex-col gap-2"
            onSubmit={(event) => {
              event.preventDefault();
              go();
            }}
          >
            <span className="text-[13px] font-medium text-bone-100">
              {t("cloudAfkMove.goto.title")}
            </span>
            <div className="flex items-end gap-2">
              {(["x", "y", "z"] as const).map((axis) => (
                <div key={axis} className="flex min-w-0 flex-1 flex-col gap-1">
                  <label
                    htmlFor={`${coordId}-${axis}`}
                    className="text-[12px] leading-none font-medium text-ash-400"
                  >
                    {axis.toUpperCase()}
                  </label>
                  <Input
                    id={`${coordId}-${axis}`}
                    value={coords[axis]}
                    placeholder={pos ? String(pos[axis]) : undefined}
                    inputMode="numeric"
                    autoComplete="off"
                    spellCheck={false}
                    maxLength={9}
                    disabled={!ok}
                    invalid={invalid.has(axis)}
                    onChange={(event) => {
                      setCoords((c) => ({ ...c, [axis]: event.target.value }));
                      if (invalid.has(axis))
                        setInvalid((s) => new Set([...s].filter((a) => a !== axis)));
                    }}
                    className="h-[32px] px-2.5 tabular-nums"
                  />
                </div>
              ))}
              <Button
                type="submit"
                size="md"
                variant="secondary"
                icon={<Crosshair size={15} />}
                disabled={!ok}
              >
                {t("cloudAfkMove.goto.go")}
              </Button>
            </div>
          </form>

          {main || own.length ? (
            <div className="flex flex-col gap-1.5 border-t border-white/7 pt-4">
              {main ? (
                <>
                  <WalkRow
                    icon={UserFocus}
                    label={t("cloudAfkMove.main.go")}
                    name={main}
                    disabled={!ok}
                    onClick={() => void act({ action: "approach", name: main }, main)}
                  />
                  <WalkRow
                    icon={PersonSimpleWalk}
                    label={t("cloudAfkMove.main.follow")}
                    name={main}
                    disabled={!ok}
                    onClick={() => void act({ action: "follow", name: main }, main)}
                  />
                </>
              ) : null}
              {own.map((name) => (
                <Fragment key={name}>
                  <WalkRow
                    icon={UserFocus}
                    label={t("cloudAfkMove.own.goNamed", { name })}
                    disabled={!ok}
                    onClick={() => void act({ action: "approach", name }, name)}
                  />
                  <WalkRow
                    icon={PersonSimpleWalk}
                    label={t("cloudAfkMove.own.followNamed", { name })}
                    disabled={!ok}
                    onClick={() => void act({ action: "follow", name }, name)}
                  />
                </Fragment>
              ))}
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
