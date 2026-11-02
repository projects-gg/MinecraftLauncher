// A voice shortcut's key (VOICE-L3; voice.md §6.3): the key as a cap ("Caps Lock", "Fare 4") or
// "Atanmamış", "Tuş seç" / "Değiştir" and "Kaldır". Picking listens for the next key or mouse side
// button (Esc gives up; a press elsewhere too), asks the launcher whether it clashes and says so in
// the row's one line: Simple Voice Chat's keys, the game's and the overlay's, Left Alt (Alt+Shift
// switches the keyboard's language), another voice shortcut. Keys are kept by their place on the
// keyboard, so a Turkish F keyboard's letters show as its own.
import { Button, cn, SwapLabel } from "@pl/ui";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { voiceApi } from "@/lib/api/voice-api";
import type { VoiceKey, VoiceKeyCheck, VoiceShortcut } from "@/lib/api/voice-types";
import { clashText, keyFromEvent, keyFromMouse, keyLabel } from "./rules";

/** The cap a key shows on (design.md §6: Tektur 13 px on a 28 px cap with a darker lip). */
export function KeyCap({
  children,
  look = "key",
}: {
  children: string;
  /** `quiet`: words in place of a key ("Atanmamış"); `listening`: waiting for a press, its edge
   * in bone (OYNA keeps the screen's one gold). */
  look?: "key" | "quiet" | "listening";
}) {
  return (
    <span
      className={cn(
        "inline-flex h-7 min-w-7 items-center justify-center rounded-md px-2 text-[13px] whitespace-nowrap transition-[border-color,color] duration-150",
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

/** The line a picked key brings (`check`), kept while the key stays. */
export function useKeyLine(key: VoiceKey | null, shortcut: VoiceShortcut) {
  const { t } = useTranslation();
  const [check, setCheck] = useState<VoiceKeyCheck | null>(null);
  const code = key ? (key.kind === "keyboard" ? key.code : key.button) : "";
  // biome-ignore lint/correctness/useExhaustiveDependencies: the code says when the key changed
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
  }, [code, shortcut]);
  return check ? clashText(check, t) : null;
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
  const button = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!listening) return;
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
      const key = keyFromEvent(event, i18n.language);
      if (key) void pick(key);
    };
    const onMouse = (event: MouseEvent) => {
      const key = keyFromMouse(event.button);
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
    window.addEventListener("mousedown", onMouse, true);
    window.addEventListener("mouseup", keepPage, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("mousedown", onMouse, true);
      window.removeEventListener("mouseup", keepPage, true);
    };
  }, [listening, shortcut, onPick, onRefused, t, i18n.language]);

  const current = value ? keyLabel(value, t) : null;
  return (
    <div className="flex items-center gap-2">
      <span aria-live="polite" className="contents">
        {listening ? (
          <KeyCap look="listening">{t("voice.key.listening")}</KeyCap>
        ) : current ? (
          <KeyCap>{current}</KeyCap>
        ) : (
          <KeyCap look="quiet">{t("voice.key.unset")}</KeyCap>
        )}
      </span>
      <Button
        ref={button}
        size="sm"
        variant="secondary"
        aria-label={`${label}: ${listening ? t("voice.key.cancel") : current ? t("voice.key.change") : t("voice.key.pick")}`}
        aria-pressed={listening}
        onClick={() => setListening((on) => !on)}
      >
        <SwapLabel
          labels={[t("voice.key.pick"), t("voice.key.change"), t("voice.key.cancel")]}
          active={listening ? 2 : current ? 1 : 0}
        />
      </Button>
      {current && !listening ? (
        <Button
          size="sm"
          variant="ghost"
          aria-label={`${label}: ${t("voice.key.clear")}`}
          onClick={() => onPick(null)}
        >
          {t("voice.key.clear")}
        </Button>
      ) : null}
    </div>
  );
}
