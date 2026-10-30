// The Projects skin panel's preview: the skin in 3D (three.js loads with the panel and lets the
// WebGL context go when it closes), its state chip, the turn button and a one-shot gold ring
// when a skin was saved (skin1.md §13.5). A new skin dips in, as in Microsoft's panel: the figure
// fades (and settles toward the floor), the new skin goes on and comes back once it is drawn,
// turned to the front. A newer skin takes over from wherever the last one is. Reduced motion and
// Performance mode keep the fades without the movement.
import { ArrowsClockwise } from "@phosphor-icons/react";
import { easeOutExpo, IconButton, Skeleton, useMotionPrefs } from "@pl/ui";
import { motion, useAnimate } from "motion/react";
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { loadImage } from "@/features/skins/skin-art";
import { SkinFlat } from "@/features/skins/skin-flat";
import type { SkinTurn } from "@/features/skins/skin-viewer";
import type { SkinVariant } from "@/lib/api/projects-skin-types";
import type { ChipKind } from "./rules";
import { StatusChip } from "./status-chip";

// three.js is large: the 3D preview loads the first time a skin panel opens.
const SkinViewer3D = lazy(() => import("@/features/skins/skin-viewer"));

/** Placeholder shaped like the player (head, body and arms, legs). */
function FigureSkeleton() {
  return (
    <div aria-hidden className="absolute inset-0 flex flex-col items-center justify-center gap-1">
      <Skeleton className="size-14" />
      <div className="flex gap-1">
        <Skeleton className="h-[84px] w-6" />
        <Skeleton className="h-[84px] w-14" />
        <Skeleton className="h-[84px] w-6" />
      </div>
      <div className="flex gap-1">
        <Skeleton className="h-[84px] w-[26px]" />
        <Skeleton className="h-[84px] w-[26px]" />
      </div>
    </div>
  );
}

export interface StageSkin {
  /** 64×64 PNG data URL; null while it is still coming. */
  picture: string | null;
  variant: SkinVariant;
}

export function SkinStage({
  skin,
  chip,
  breathing,
  ringKey,
}: {
  skin: StageSkin;
  chip: ChipKind | null;
  breathing: boolean;
  /** Grows when a skin was saved: the gold ring plays once. */
  ringKey: number;
}) {
  const { t } = useTranslation();
  const { layout: glide } = useMotionPrefs();
  const [scope, animate] = useAnimate<HTMLDivElement>();
  const [shown, setShown] = useState<StageSkin>(skin);
  const [turn, setTurn] = useState<SkinTurn | null>(null);
  const [flat, setFlat] = useState(false);
  const facing = turn?.side ?? "front";

  const token = useRef(0);
  const drawn = useRef<(() => void) | null>(null);
  const onSkinShown = useCallback(() => {
    drawn.current?.();
    drawn.current = null;
  }, []);

  // The arm model alone changes in place; a new picture dips in once it is decoded. `target` is
  // the picture on its way (renders meanwhile do not start the dip again), and the swap puts on
  // the latest skin given, so an arm model changed during the dip is not lost.
  const shownRef = useRef(shown);
  shownRef.current = shown;
  const latest = useRef(skin);
  latest.current = skin;
  const target = useRef(shown.picture);
  useEffect(() => {
    const before = shownRef.current;
    if (skin.picture === target.current) {
      if (skin.picture === before.picture && skin.variant !== before.variant) setShown(skin);
      return;
    }
    target.current = skin.picture;
    const mine = ++token.current;
    const element = scope.current;
    if (!before.picture || !skin.picture || !element) {
      setShown(skin);
      if (skin.picture) setTurn({ side: "front", key: Date.now() });
      return;
    }
    const picture = skin.picture;
    void (async () => {
      await loadImage(picture).catch(() => null);
      if (mine !== token.current) return;
      await animate(element, glide ? { opacity: 0, scale: 0.96 } : { opacity: 0 }, {
        duration: 0.1,
        ease: easeOutExpo,
      });
      if (mine !== token.current) return;
      await new Promise<void>((resolve) => {
        drawn.current = resolve;
        // A skin the viewer cannot report (the flat view) goes on shortly.
        window.setTimeout(resolve, flat ? 32 : 200);
        setShown(latest.current.picture === picture ? latest.current : { ...skin, picture });
        setTurn({ side: "front", key: Date.now() });
      });
      if (mine !== token.current) return;
      void animate(element, glide ? { opacity: 1, scale: 1 } : { opacity: 1 }, {
        duration: 0.3,
        ease: easeOutExpo,
      });
    })();
  }, [skin, animate, scope, glide, flat]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="relative min-h-0 flex-1 overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-950/45 ring-1 ring-white/6">
        <div
          aria-hidden
          // The floor's light in bone, not a gold halo (one gold per screen, design.md §2).
          className="pointer-events-none absolute inset-x-8 bottom-7 h-14 rounded-[50%] bg-[radial-gradient(closest-side,color-mix(in_oklab,var(--color-bone-100)_12%,transparent),transparent)]"
        />
        <div
          ref={scope}
          role="img"
          aria-label={t("skins.preview.label")}
          // The dip on a skin change settles toward the floor glow.
          className="absolute inset-0 origin-[50%_82%]"
        >
          {flat ? (
            shown.picture ? (
              <div className="absolute inset-0 flex items-end justify-center pb-[52px]">
                <SkinFlat src={shown.picture} variant={shown.variant} legacy={false} scale={8} />
              </div>
            ) : null
          ) : (
            <Suspense fallback={<FigureSkeleton />}>
              <SkinViewer3D
                className="absolute inset-0"
                skin={shown.picture}
                variant={shown.variant}
                cape={null}
                turn={turn}
                onUnsupported={() => setFlat(true)}
                onSkinShown={onSkinShown}
              />
            </Suspense>
          )}
          {shown.picture ? null : <FigureSkeleton />}
        </div>
        {ringKey > 0 ? (
          <motion.span
            key={ringKey}
            aria-hidden
            className="pointer-events-none absolute inset-0 rounded-[var(--radius-panel)] ring-2 ring-gold-300/70 ring-inset"
            initial={{ opacity: 0 }}
            animate={{ opacity: [0, 1, 0] }}
            transition={{ duration: 1.6, times: [0, 0.18, 1], ease: "easeOut" }}
          />
        ) : null}
        <StatusChip kind={chip} breathing={breathing} className="absolute top-3 left-3" />
        {flat ? null : (
          <IconButton
            className="absolute top-2 right-2"
            size="sm"
            tooltipSide="left"
            label={facing === "back" ? t("skins.preview.showFront") : t("skins.preview.showBack")}
            icon={<ArrowsClockwise size={17} />}
            onClick={() => setTurn({ side: facing === "back" ? "front" : "back", key: Date.now() })}
          />
        )}
      </div>
      <p className="mt-3 text-center text-[12.5px] text-ash-400">
        {flat ? t("skins.preview.flat") : t("skins.preview.hint")}
      </p>
    </div>
  );
}
