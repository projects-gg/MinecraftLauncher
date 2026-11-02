// A voice shortcut's key (VOICE-L3; voice.md §6.3): the key as caps ("Caps Lock", "Fare 4",
// "Ctrl + Shift + M") or "Atanmamış", "Tuş seç" / "Değiştir" and "Kaldır". Picking listens for the
// next key or mouse side button (Esc gives up; a press elsewhere too), asks the launcher whether it
// clashes and says so in the row's one line: Simple Voice Chat's keys, the game's and the overlay's,
// Left Alt (Alt+Shift switches the keyboard's language), another voice shortcut, and a mute or
// deafen key that also fires while typing. Mute and deafen record a combination as it is pressed
// (VOICE-KEYS): Ctrl, Shift and Alt show as caps while held and wait for the key; one let go alone
// is the key itself ("Sağ Ctrl"). Push-to-talk takes the first key. Keys are kept by their place on
// the keyboard, so a Turkish F keyboard's letters show as its own.
import { Button, cn, SwapLabel } from "@pl/ui";
import { Fragment, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { voiceApi } from "@/lib/api/voice-api";
import type { VoiceKey, VoiceKeyCheck, VoiceModifier, VoiceShortcut } from "@/lib/api/voice-types";
import {
  clashText,
  heldModifiers,
  keyboardLayout,
  keyFromEvent,
  keyFromMouse,
  keyLabel,
  keyParts,
  type LayoutMap,
  MODIFIERS,
  modifierLabel,
  modifierOf,
  takesModifiers,
} from "./rules";

/** The cap a key shows on (design.md §6: Tektur 13 px on a 28 px cap with a darker lip). */
export function KeyCap({
  children,
  look = "key",
  small = false,
}: {
  children: string;
  /** `quiet`: words in place of a key ("Atanmamış"); `listening`: waiting for a press, its edge
   * in bone (OYNA keeps the screen's one gold). */
  look?: "key" | "quiet" | "listening";
  /** A list's cap (Discord's keys): 24 px, 12 px type. */
  small?: boolean;
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center justify-center rounded-md whitespace-nowrap transition-[border-color,color] duration-150",
        small ? "h-6 min-w-6 px-1.5 text-[12px]" : "h-7 min-w-7 px-2 text-[13px]",
        look === "key" &&
          "border border-white/14 bg-obsidian-800 font-display font-semibold text-bone-50 shadow-[inset_0_-2px_0_rgb(0_0_0/0.35)]",
        look === "quiet" && "font-sans font-medium text-ash-300",
        look === "listening" &&
          "border border-bone-100/60 bg-obsidian-800 font-sans font-medium text-bone-50",
      )}
    >
      {children}
    </span>
  );
}

/** A combination's caps with a quiet "+" between ("Ctrl + Shift + M"); `waiting` ends it with an
 * open cap while the key is still to come. */
export function KeyCaps({
  parts,
  waiting = false,
  small = false,
}: {
  parts: readonly string[];
  waiting?: boolean;
  small?: boolean;
}) {
  const caps = waiting ? [...parts, "…"] : parts;
  return (
    <span className="inline-flex items-center gap-1">
      {caps.map((part, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: a combination's caps keep their places
        <Fragment key={i}>
          {i > 0 ? (
            <span aria-hidden className="text-[12px] font-medium text-ash-400">
              +
            </span>
          ) : null}
          <KeyCap look={waiting && i === caps.length - 1 ? "listening" : "key"} small={small}>
            {part}
          </KeyCap>
        </Fragment>
      ))}
    </span>
  );
}

/** What identifies a key for its check: the place, the button, what is held with it. */
function keyId(key: VoiceKey | null): string {
  if (!key) return "";
  const at = key.kind === "keyboard" ? key.code : key.button;
  return key.mods?.length ? `${key.mods.join("+")}+${at}` : at;
}

/** The line a picked key brings (`check`), kept while the key stays. */
export function useKeyLine(key: VoiceKey | null, shortcut: VoiceShortcut) {
  const { t } = useTranslation();
  const [check, setCheck] = useState<VoiceKeyCheck | null>(null);
  const id = keyId(key);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the id says when the key changed
  useEffect(() => {
    let alive = true;
    if (!key) {
      setCheck(null);
      return;
    }
    void voiceApi
      .checkKey(key, shortcut)
      .then((answer) => {
        if (alive) setCheck(answer);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [id, shortcut]);
  return check ? clashText(check, t, shortcut) : null;
}

export function KeyPicker({
  value,
  shortcut,
  label,
  onPick,
  onRefused,
}: {
  value: VoiceKey | null;
  shortcut: VoiceShortcut;
  /** The shortcut's name ("Bas-konuş tuşu"), for the controls' names. */
  label: string;
  onPick: (key: VoiceKey | null) => void;
  /** A key that cannot be used: the row's line says so for a moment. */
  onRefused: (text: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const [listening, setListening] = useState(false);
  /** Ctrl, Shift, Alt held while a combination is being pressed. */
  const [held, setHeld] = useState<VoiceModifier[]>([]);
  const button = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!listening) {
      setHeld([]);
      return;
    }
    const combos = takesModifiers(shortcut);
    let alive = true;
    // The layout names a key as it is written on it, whatever Shift or AltGr made the press type.
    let layout: LayoutMap | null = null;
    void keyboardLayout().then((map) => {
      if (alive) layout = map;
    });
    /** A modifier pressed with nothing else: let go alone, it is the key. */
    let lone: string | null = null;
    const pick = async (key: VoiceKey) => {
      setListening(false);
      try {
        const check = await voiceApi.checkKey(key, shortcut);
        if (!check.usable) {
          onRefused(t("voice.key.clash.unusable"));
          return;
        }
      } catch {
        // The launcher keeps only keys it can watch; it says no by dropping the key.
      }
      onPick(key);
    };
    const onKey = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        setListening(false);
        return;
      }
      const mod = combos ? modifierOf(event.code) : null;
      if (mod) {
        if (!event.repeat) {
          lone = heldModifiers(event).some((m) => m !== mod) ? null : event.code;
        }
        setHeld(heldModifiers(event));
        return;
      }
      lone = null;
      const key = keyFromEvent(event, i18n.language, combos ? heldModifiers(event) : [], layout);
      if (key) void pick(key);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (!combos || !modifierOf(event.code)) return;
      event.preventDefault();
      event.stopPropagation();
      if (lone === event.code) {
        lone = null;
        const key = keyFromEvent(event, i18n.language, [], layout);
        if (key) void pick(key);
        return;
      }
      setHeld(heldModifiers(event));
    };
    const onMouse = (event: MouseEvent) => {
      const key = keyFromMouse(event.button, combos ? heldModifiers(event) : []);
      if (key) {
        event.preventDefault();
        event.stopPropagation();
        void pick(key);
      } else if (event.target !== button.current) {
        // A press anywhere else gives up, as Esc does.
        setListening(false);
      }
    };
    // The side buttons go back and forward in a web page: not while one is picked.
    const keepPage = (event: MouseEvent) => {
      if (event.button === 3 || event.button === 4) event.preventDefault();
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("mousedown", onMouse, true);
    window.addEventListener("mouseup", keepPage, true);
    return () => {
      alive = false;
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("mousedown", onMouse, true);
      window.removeEventListener("mouseup", keepPage, true);
    };
  }, [listening, shortcut, onPick, onRefused, t, i18n.language]);

  const current = value ? keyParts(value, t) : null;
  const named = value ? keyLabel(value, t) : null;
  const holding = MODIFIERS.filter((m) => held.includes(m)).map((m) => modifierLabel(m, t));
  return (
    <div className="flex items-center gap-2">
      <span aria-live="polite" className="contents">
        {listening ? (
          holding.length ? (
            <KeyCaps parts={holding} waiting />
          ) : (
            <KeyCap look="listening">{t("voice.key.listening")}</KeyCap>
          )
        ) : current ? (
          <KeyCaps parts={current} />
        ) : (
          <KeyCap look="quiet">{t("voice.key.unset")}</KeyCap>
        )}
      </span>
      <Button
        ref={button}
        size="sm"
        variant="secondary"
        aria-label={`${label}: ${listening ? t("voice.key.cancel") : named ? `${named}, ${t("voice.key.change")}` : t("voice.key.pick")}`}
        aria-pressed={listening}
        onClick={() => setListening((on) => !on)}
      >
        <SwapLabel
          labels={[t("voice.key.pick"), t("voice.key.change"), t("voice.key.cancel")]}
          active={listening ? 2 : current ? 1 : 0}
        />
      </Button>
      {/* Kept in place (hidden) while a key is pressed: the caps do not jump sideways. */}
      {current ? (
        <Button
          size="sm"
          variant="ghost"
          aria-label={`${label}: ${t("voice.key.clear")}`}
          aria-hidden={listening || undefined}
          tabIndex={listening ? -1 : undefined}
          className={listening ? "invisible" : undefined}
          onClick={() => onPick(null)}
        >
          {t("voice.key.clear")}
        </Button>
      ) : null}
    </div>
  );
}
