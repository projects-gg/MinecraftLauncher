// The official pack's mod keys that still clash in an older options.txt (KEYS2). The launcher
// never edits the file by itself: the mod check reports the clash, "Düzelt" moves only the
// clashing bindings (after a backup) and "Geri al" puts them back. Before a launch the question
// is a step of the mod health dialog, so the fix lands before the game reads the file. Along with
// the clashes, "Düzelt" gives a still unbound push to talk its key (KEYS3): a row of its own
// before the fix and a line of the toast after it.
import { ArrowRight } from "@phosphor-icons/react";
import { Button, cn, easeOutExpo, toast } from "@pl/ui";
import { type QueryClient, useQueryClient } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { showError } from "@/features/content/jobs";
import { stagger, swap, useMotionPrefs } from "@/features/modcenter/motion";
import { api, type KeyChange, type KeyFix } from "@/lib/api";
import { type FlowStep, modCheckKey } from "./flow";
import { actionName, changeSpoken, fixedLines, isUnbound, keyLabel } from "./key-texts";

/** The mod check and the mod center read the file again. */
function recheck(queryClient: QueryClient, instanceId: string) {
  void queryClient.invalidateQueries({ queryKey: modCheckKey(instanceId) });
}

/**
 * "Düzelt" on the mod center's line: fixes at once, then a toast with the new keys (a line of its
 * own for a push to talk that got one) and "Geri al". Refused (with its reason) while the game
 * runs, since the game writes its own keys back.
 */
export async function fixKeysNow(t: TFunction, queryClient: QueryClient, instanceId: string) {
  let fix: KeyFix;
  try {
    fix = await api.modHealth.fixKeys(instanceId);
  } catch (error) {
    showError(error);
    return;
  } finally {
    recheck(queryClient, instanceId);
  }
  const backup = fix.backup;
  if (!backup) return;
  const lines = fixedLines(t, fix.changes);
  toast.success(t("keyClash.fixed"), {
    // One sentence per line; the small gap keeps a wrapped line apart from the next sentence.
    description:
      lines.length > 0
        ? lines.map((line, index) => (
            <span key={line} className={cn("block", index > 0 && "mt-1")}>
              {line}
            </span>
          ))
        : undefined,
    duration: 8000,
    action: {
      label: t("keyClash.undo"),
      onClick: () => {
        void api.modHealth
          .undoKeys(instanceId, backup)
          .catch(showError)
          .finally(() => recheck(queryClient, instanceId));
      },
    },
  });
}

type Cap = "old" | "spent" | "next" | "set";

const capTone: Record<Cap, string> = {
  old: "border-white/14 bg-obsidian-800 text-bone-100",
  spent: "border-white/8 bg-obsidian-800/60 text-ash-400 opacity-60",
  next: "border-gold-400/35 bg-gold-500/[0.07] text-gold-300",
  set: "border-gold-400/80 bg-gold-500/[0.18] text-gold-200",
};

/** A keypress: the new key dips and comes back up (one constant, so a re-render never replays it). */
const SETTLE = { transform: ["scale(0.88)", "scale(1)"] };

/** A key as a keycap: Tektur on a dark cap with a darker lip, like the keys it stands for. */
function KeyCap({
  tone,
  delay,
  settle,
  children,
}: {
  tone: Cap;
  delay: number;
  /** Plays the keypress settle (on "Düzelt"; never on first render or under reduced motion). */
  settle: boolean;
  children: ReactNode;
}) {
  return (
    <motion.span
      animate={settle ? SETTLE : undefined}
      transition={{ duration: 0.28, ease: easeOutExpo, delay }}
      style={{ transitionDelay: `${Math.round(delay * 1000)}ms` }}
      className={cn(
        "inline-flex h-7 min-w-7 items-center justify-center rounded-[5px] border px-1.5",
        "font-display text-[13px] leading-none font-semibold shadow-[inset_0_-2px_0_rgba(0,0,0,0.38)]",
        "transition-[opacity,color,background-color,border-color] duration-200 ease-[cubic-bezier(0.16,1,0.3,1)]",
        capTone[tone],
      )}
    >
      {children}
    </motion.span>
  );
}

/**
 * The bindings a fix changes, one row each: what it does and its mod, then the key it has and the
 * key it gets ("Atanmamış" on either side when there is none). Once fixed, the old caps fade back
 * and the new ones light up row by row.
 */
export function KeyChangeList({
  changes,
  fixed,
  settle,
}: {
  changes: readonly KeyChange[];
  fixed: boolean;
  /** The change just happened (not on first render): the new caps settle like a pressed key. */
  settle: boolean;
}) {
  const { t } = useTranslation();
  const prefs = useMotionPrefs();
  return (
    <ul className="divide-y divide-white/6 rounded-[10px] bg-obsidian-900/55 px-3.5 ring-1 ring-white/7">
      {changes.map((change, index) => {
        const delay = stagger(index, 0.045);
        return (
          <li key={change.name} className="flex min-h-12 items-center gap-4 py-2">
            <div className="min-w-0 flex-1">
              <p className="truncate text-[13.5px] font-medium text-bone-100">
                {actionName(t, change)}
              </p>
              <p className="truncate text-[12px] text-ash-400">{change.modName}</p>
            </div>
            <span className="sr-only">{changeSpoken(t, change, fixed)}</span>
            <span aria-hidden className="flex shrink-0 items-center gap-2">
              {isUnbound(change.key) ? (
                // No key yet (push to talk): the word, fading back like an old cap.
                <span
                  style={{ transitionDelay: `${Math.round(delay * 1000)}ms` }}
                  className={cn(
                    "flex h-7 items-center text-[12.5px]",
                    "transition-[opacity,color] duration-200 ease-[cubic-bezier(0.16,1,0.3,1)]",
                    fixed ? "text-ash-400 opacity-60" : "text-ash-300",
                  )}
                >
                  {keyLabel(t, undefined)}
                </span>
              ) : (
                <KeyCap tone={fixed ? "spent" : "old"} delay={delay} settle={false}>
                  {keyLabel(t, change.key)}
                </KeyCap>
              )}
              <ArrowRight
                size={12}
                weight="bold"
                className={cn(
                  "transition-colors duration-200",
                  fixed ? "text-gold-400/70" : "text-ash-500",
                )}
              />
              {/* The new key's slot is as wide as its widest content (the Caps Lock cap), so the
                  arrows line up. */}
              <span className="flex min-w-[80px]">
                {change.to ? (
                  <KeyCap
                    tone={fixed ? "set" : "next"}
                    delay={delay}
                    settle={settle && fixed && !prefs.reduce}
                  >
                    {keyLabel(t, change.to)}
                  </KeyCap>
                ) : (
                  <span
                    className={cn(
                      "flex h-7 items-center text-[12.5px] transition-colors duration-200",
                      fixed ? "text-bone-100" : "text-ash-300",
                    )}
                  >
                    {keyLabel(t, undefined)}
                  </span>
                )}
              </span>
            </span>
          </li>
        );
      })}
    </ul>
  );
}

/** What the flow dialog shows for the key step. */
export interface KeysStepView {
  titleKey: string;
  title: ReactNode;
  body: ReactNode;
  footer: ReactNode;
}

/**
 * The key question before a launch, as a step of the mod health dialog: the bindings that
 * change, "Yine de başlat" and "Düzelt"; once fixed, the same rows with the new keys lit,
 * "Geri al" and "Oyna". Nothing launches until the player says so.
 */
export function useKeysStep({
  step,
  instanceId,
  session,
  playNow,
}: {
  step: FlowStep | null;
  instanceId: string | null;
  /** The flow's session: a new flow starts unfixed. */
  session: number;
  playNow: () => void;
}): KeysStepView | null {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [fix, setFix] = useState<KeyFix | null>(null);
  const [busy, setBusy] = useState(false);
  const [settle, setSettle] = useState(false);
  const [said, setSaid] = useState("");
  const focus = useRef<"play" | "fix" | null>(null);
  const playButton = useRef<HTMLButtonElement>(null);
  const fixButton = useRef<HTMLButtonElement>(null);
  const fixed = Boolean(fix?.backup);

  // biome-ignore lint/correctness/useExhaustiveDependencies: runs once per flow opened
  useEffect(() => {
    setFix(null);
    setBusy(false);
    setSettle(false);
    setSaid("");
  }, [session]);

  // The button that takes the place of the pressed one gets the focus.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the state flips
  useEffect(() => {
    const target =
      focus.current === "play" ? playButton : focus.current === "fix" ? fixButton : null;
    focus.current = null;
    target?.current?.focus();
  }, [fixed]);

  if (step?.kind !== "keys") return null;
  const issue = step.check.issues.find((i) => i.kind === "keyClash");
  const changes = fix?.changes.length ? fix.changes : (issue?.keys ?? []);

  const doFix = async () => {
    if (!instanceId || busy) return;
    setBusy(true);
    try {
      const result = await api.modHealth.fixKeys(instanceId);
      if (!result.backup) {
        // Nothing clashes any more (fixed elsewhere meanwhile): on with the game.
        playNow();
        return;
      }
      focus.current = "play";
      setSettle(true);
      setFix(result);
      setSaid(t("keyClash.fixed"));
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
      recheck(queryClient, instanceId);
    }
  };

  const doUndo = async () => {
    const backup = fix?.backup;
    if (!instanceId || !backup || busy) return;
    setBusy(true);
    try {
      await api.modHealth.undoKeys(instanceId, backup);
      focus.current = "fix";
      setSettle(false);
      setFix(null);
      setSaid(t("keyClash.undone"));
    } catch (error) {
      showError(error);
    } finally {
      setBusy(false);
      recheck(queryClient, instanceId);
    }
  };

  return {
    titleKey: fixed ? "keys:fixed" : "keys",
    title: fixed ? t("keyClash.fixedTitle") : t("keyClash.title"),
    body: (
      <div>
        <p className="relative mb-3 text-[13.5px] leading-relaxed text-ash-300">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span key={fixed ? "fixed" : "lead"} {...swap(0.18)} className="block">
              {fixed ? t("keyClash.fixedLead") : t("keyClash.lead")}
            </motion.span>
          </AnimatePresence>
        </p>
        <KeyChangeList changes={changes} fixed={fixed} settle={settle} />
        <p role="status" aria-atomic="true" className="sr-only">
          {said}
        </p>
      </div>
    ),
    footer: (
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.div
          key={fixed ? "fixed" : "ask"}
          {...swap(0.16)}
          className="flex items-center gap-2"
        >
          {fixed ? (
            <>
              <Button variant="ghost" disabled={busy} onClick={() => void doUndo()}>
                {t("keyClash.undo")}
              </Button>
              <Button ref={playButton} variant="primary" disabled={busy} onClick={playNow}>
                {t("keyClash.play")}
              </Button>
            </>
          ) : (
            <>
              <Button variant="secondary" disabled={busy} onClick={playNow}>
                {t("keyClash.anyway")}
              </Button>
              <Button
                ref={fixButton}
                variant="primary"
                aria-disabled={busy || undefined}
                onClick={() => void doFix()}
              >
                {t("keyClash.fix")}
              </Button>
            </>
          )}
        </motion.div>
      </AnimatePresence>
    ),
  };
}
