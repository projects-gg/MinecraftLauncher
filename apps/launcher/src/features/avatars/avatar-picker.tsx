// A grid of avatars to pick one from: optional leading choices (the player head today; a Discord
// picture or a Minecraft head in the friends profile later), then every bundled picture in
// manifest order. Arrow keys move, Enter or a click picks; the gold pixel brackets mark the pick
// and travel to the new one.
import { cn, PixelBrackets, springSnappy } from "@pl/ui";
import { MotionConfig, motion } from "motion/react";
import { type KeyboardEvent, type ReactNode, useId, useRef, useState } from "react";
import { PresetFace } from "./preset-face";
import { PRESET_AVATARS, type PresetAvatar } from "./presets";

export interface AvatarChoice {
  /** The stored value (`preset:<slug>`, ...); null for the default choice. */
  id: string | null;
  /** Spoken name. */
  label: string;
  face: (size: number) => ReactNode;
}

/** Next focused index for a key in a `columns`-wide grid of `count` items; null for other keys. */
export function moveInGrid(
  index: number,
  key: string,
  count: number,
  columns: number,
): number | null {
  if (count <= 0) return null;
  const last = count - 1;
  const row = Math.floor(index / columns);
  const lastRow = Math.floor(last / columns);
  switch (key) {
    case "ArrowRight":
      return Math.min(last, index + 1);
    case "ArrowLeft":
      return Math.max(0, index - 1);
    case "ArrowDown":
      if (index + columns <= last) return index + columns;
      // A shorter last row: land on its last item.
      return row < lastRow ? last : index;
    case "ArrowUp":
      return index - columns >= 0 ? index - columns : index;
    case "Home":
      return 0;
    case "End":
      return last;
    default:
      return null;
  }
}

/** The bundled pictures as picker choices. */
export function presetChoices(avatars: readonly PresetAvatar[] = PRESET_AVATARS): AvatarChoice[] {
  return avatars.map((avatar) => ({
    id: avatar.id,
    label: avatar.name,
    face: (size: number) => <PresetFace avatar={avatar} size={size} />,
  }));
}

export function AvatarPicker({
  value,
  onSelect,
  leading = [],
  label,
  columns = 6,
  size = 52,
  className,
}: {
  value: string | null;
  onSelect: (id: string | null) => void;
  /** Choices before the bundled pictures. */
  leading?: readonly AvatarChoice[];
  /** Name of the list for screen readers. */
  label: string;
  columns?: number;
  /** Tile size in px. */
  size?: number;
  className?: string;
}) {
  const bracketsId = useId();
  const listRef = useRef<HTMLDivElement>(null);
  const choices = [...leading, ...presetChoices()];
  // A stored picture that left the set shows the default choice, so that one is marked.
  const current =
    choices.some((choice) => choice.id === value) || !choices.some((c) => c.id === null)
      ? value
      : null;
  const selected = choices.findIndex((choice) => choice.id === current);
  const [focused, setFocused] = useState(Math.max(0, selected));

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const next = moveInGrid(focused, event.key, choices.length, columns);
    if (next === null) return;
    event.preventDefault();
    setFocused(next);
    listRef.current?.querySelectorAll<HTMLElement>("[role=option]")[next]?.focus();
  };

  return (
    <MotionConfig reducedMotion="user">
      <div
        ref={listRef}
        role="listbox"
        aria-label={label}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className={cn("grid gap-2 outline-none", className)}
        style={{ gridTemplateColumns: `repeat(${columns}, ${size}px)` }}
      >
        {choices.map((choice, index) => {
          const isSelected = index === selected;
          return (
            <button
              key={choice.id ?? "default"}
              type="button"
              role="option"
              aria-selected={isSelected}
              aria-label={choice.label}
              tabIndex={index === focused ? 0 : -1}
              onFocus={() => setFocused(index)}
              onClick={() => onSelect(choice.id)}
              className={cn(
                "group/tile relative flex cursor-pointer rounded-[var(--radius-head)] outline-none",
                "transition-[transform,scale,filter] duration-150 ease-out hover:brightness-110 active:scale-[0.95]",
                "focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:ring-offset-2 focus-visible:ring-offset-obsidian-900",
              )}
              style={{ width: size, height: size }}
            >
              {choice.face(size)}
              {isSelected ? (
                <motion.span
                  layoutId={bracketsId}
                  aria-hidden
                  className="pointer-events-none absolute -inset-[4px]"
                  transition={springSnappy}
                >
                  <PixelBrackets size={Math.max(8, Math.round(size / 5))} />
                </motion.span>
              ) : null}
            </button>
          );
        })}
      </div>
    </MotionConfig>
  );
}
