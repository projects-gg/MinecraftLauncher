// A friend's (or the player's) picture in the same frame as player heads: a bundled preset, the
// Discord photo, the Minecraft head (cut from the skin with CSS, pixel-sharp), or the first letter.
// The presence dot sits on its corner: filled in game, a ring in the launcher, a bar for
// do-not-disturb; offline faces are dimmed. Shape and the text next to it carry the meaning, not
// colour alone.
import { cn, PlayerHead } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { PresetFace } from "@/features/avatars/preset-face";
import { presetAvatar } from "@/features/avatars/presets";
import type { PresenceState } from "@/lib/api/social-types";
import { pop, useSocialMotion } from "./motion";

export interface FaceSource {
  name: string;
  avatar?: string;
  avatarUrl?: string;
}

const TEXTURE = /^minecraft:([0-9a-f]{40,64})$/;

/** The Minecraft head of a skin texture (face plus hat layer), drawn from the skin itself. */
function MinecraftHead({ hash, size }: { hash: string; size: number }) {
  const url = `https://textures.minecraft.net/texture/${hash}`;
  return (
    <span
      aria-hidden
      className="relative inline-flex shrink-0 overflow-hidden rounded-[var(--radius-head)] bg-obsidian-800 ring-1 ring-white/10"
      style={{
        width: size,
        height: size,
        // Skins are 64 px wide: the face is at (8, 8), the hat at (40, 8), 8 × 8 each.
        backgroundImage: `url("${url}"), url("${url}")`,
        backgroundSize: `${size * 8}px auto, ${size * 8}px auto`,
        backgroundPosition: `-${size * 5}px -${size}px, -${size}px -${size}px`,
        backgroundRepeat: "no-repeat",
        imageRendering: "pixelated",
      }}
    />
  );
}

/** A remote photo that falls back to the letter when it cannot load (offline, blocked). */
function Photo({ src, name, size }: { src: string; name: string; size: number }) {
  const [failed, setFailed] = useState(false);
  if (failed) return <PlayerHead name={name} size={size} />;
  return (
    <span
      className="relative inline-flex shrink-0 overflow-hidden rounded-[var(--radius-head)] bg-obsidian-800 ring-1 ring-white/10"
      style={{ width: size, height: size }}
    >
      <img
        src={src}
        alt=""
        width={size}
        height={size}
        decoding="async"
        draggable={false}
        referrerPolicy="no-referrer"
        onError={() => setFailed(true)}
        className="size-full object-cover"
      />
    </span>
  );
}

export function SocialFace({
  source,
  size,
  className,
}: {
  source: FaceSource;
  size: number;
  className?: string;
}) {
  const preset = presetAvatar(source.avatar);
  const texture = source.avatar ? TEXTURE.exec(source.avatar)?.[1] : undefined;
  const face = preset ? (
    <PresetFace avatar={preset} size={size} />
  ) : texture ? (
    <MinecraftHead hash={texture} size={size} />
  ) : source.avatarUrl ? (
    <Photo src={source.avatarUrl} name={source.name} size={size} />
  ) : (
    <PlayerHead name={source.name} size={size} />
  );
  return <span className={cn("inline-flex shrink-0", className)}>{face}</span>;
}

type DotKind = "playing" | "launcher" | "dnd";

function dotKind(state: PresenceState, dnd: boolean): DotKind | null {
  if (state === "offline") return null;
  if (dnd) return "dnd";
  return state;
}

/** The presence dot alone (also used in status menus). */
export function PresenceDot({
  kind,
  size,
  className,
}: {
  kind: DotKind | "invisible";
  size: number;
  className?: string;
}) {
  return (
    <span
      aria-hidden
      className={cn(
        "relative inline-flex shrink-0 items-center justify-center rounded-full",
        kind === "playing" && "bg-jade-400",
        kind === "launcher" && "bg-transparent shadow-[inset_0_0_0_2px_var(--color-jade-400)]",
        kind === "dnd" && "bg-gold-400",
        kind === "invisible" && "bg-transparent shadow-[inset_0_0_0_2px_var(--color-ash-500)]",
        className,
      )}
      style={{ width: size, height: size }}
    >
      {kind === "dnd" ? (
        <span
          className="rounded-full bg-obsidian-950"
          style={{ width: Math.max(4, Math.round(size * 0.5)), height: 2 }}
        />
      ) : null}
    </span>
  );
}

export function SocialAvatar({
  source,
  size,
  state,
  dnd = false,
  /** Ring colour around the dot: the surface behind the avatar. */
  surface = "var(--color-obsidian-900)",
  className,
}: {
  source: FaceSource;
  size: number;
  state?: PresenceState;
  dnd?: boolean;
  surface?: string;
  className?: string;
}) {
  const { reduce } = useSocialMotion();
  const kind = state ? dotKind(state, dnd) : null;
  const dot = Math.max(9, Math.round(size * 0.3));
  return (
    <span className={cn("relative inline-flex shrink-0", className)}>
      <span
        className={cn(
          "inline-flex transition-[opacity,filter] duration-300",
          state === "offline" && "opacity-55 grayscale-[0.45]",
        )}
      >
        <SocialFace source={source} size={size} />
      </span>
      <AnimatePresence initial={false}>
        {kind ? (
          <motion.span
            key={kind}
            className="absolute flex rounded-full"
            style={{
              right: -Math.round(dot / 3),
              bottom: -Math.round(dot / 3),
              padding: 2,
              background: surface,
            }}
            {...pop(reduce)}
          >
            <PresenceDot kind={kind} size={dot} />
          </motion.span>
        ) : null}
      </AnimatePresence>
    </span>
  );
}
