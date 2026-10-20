import {
  ArrowSquareOut,
  CaretLeft,
  CaretRight,
  Copy,
  FolderOpen,
  Trash,
  X,
} from "@phosphor-icons/react";
import {
  Button,
  dialogMotion,
  EntranceGate,
  GatedMotion,
  IconButton,
  scrimMotion,
  useMotionPrefs,
} from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { Dialog as RadixDialog } from "radix-ui";
import { type KeyboardEvent, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Screenshot } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { shotTime } from "../trash-label";
import { formatWhen } from "../when";

/** Pictures decoded for the viewer, kept so moving back and forth never decodes twice. */
const decoded = new Map<string, Promise<void>>();

function decode(url: string): Promise<void> {
  let ready = decoded.get(url);
  if (!ready) {
    const image = new Image();
    image.decoding = "async";
    image.src = url;
    // A picture that cannot be decoded still gets its turn (the <img> shows what it can).
    ready = image.decode().catch(() => undefined);
    decoded.set(url, ready);
    // Only the neighbourhood is worth keeping.
    if (decoded.size > 24) {
      const oldest = decoded.keys().next().value;
      if (oldest !== undefined) decoded.delete(oldest);
    }
  }
  return ready;
}

/**
 * The picture the viewer shows: the wanted one once it is decoded, the previous one meanwhile,
 * so moving between pictures swaps them at once instead of blinking through an empty frame.
 * `first`: the first picture since the viewer opened (the only one that fades in).
 */
function useDecodedSrc(src: string | undefined): { src?: string; first: boolean } {
  const [shown, setShown] = useState<{ src: string; first: boolean } | null>(null);
  useEffect(() => {
    if (!src) {
      setShown(null);
      return;
    }
    let current = true;
    void decode(src).then(() => {
      if (current) setShown((prev) => ({ src, first: prev === null }));
    });
    return () => {
      current = false;
    };
  }, [src]);
  return src && shown ? shown : { first: true };
}

/**
 * Full-window screenshot viewer: arrow keys (and the side buttons) move between pictures, Esc
 * closes, Delete asks to delete, Ctrl+C copies. Neighbours are decoded ahead, so moving is
 * instant (and, being a frequent keyboard action, does not animate); only the first picture
 * fades in.
 */
export function Lightbox({
  shots,
  index,
  onIndexChange,
  onClose,
  onCopy,
  onOpen,
  onReveal,
  onDelete,
}: {
  shots: Screenshot[];
  index: number | null;
  onIndexChange: (index: number) => void;
  onClose: () => void;
  onCopy: (shot: Screenshot) => void;
  onOpen: (shot: Screenshot) => void;
  onReveal: (shot: Screenshot) => void;
  onDelete: (shot: Screenshot) => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const shot = index === null ? undefined : shots[index];
  const count = shots.length;
  const picture = useDecodedSrc(shot?.url);

  const go = (delta: number) => {
    if (index === null || count < 2) return;
    onIndexChange((index + delta + count) % count);
  };

  useEffect(() => {
    if (index === null || count < 2) return;
    for (const delta of [1, -1, 2]) {
      const neighbour = shots[(index + delta + count) % count];
      if (neighbour) void decode(neighbour.url);
    }
  }, [index, count, shots]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!shot) return;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      go(-1);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      go(1);
    } else if (e.key === "Delete") {
      e.preventDefault();
      onDelete(shot);
    } else if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "c") {
      // Ctrl+C copies the picture, as in any image viewer (not AltGr, which is Ctrl+Alt).
      e.preventDefault();
      onCopy(shot);
    }
  };

  const when = shot ? formatWhen(shotTime(shot)) || t("instance.screenshots.untitled") : "";

  return (
    <RadixDialog.Root open={Boolean(shot)} onOpenChange={(open) => !open && onClose()}>
      <AnimatePresence>
        {shot && index !== null ? (
          // The entrance waits for the frame that draws the viewer (a big picture to raster).
          <EntranceGate key="lightbox">
            <RadixDialog.Portal forceMount>
              <RadixDialog.Overlay asChild forceMount>
                <GatedMotion.div
                  className="fixed inset-0 z-40 bg-obsidian-950/90 backdrop-blur-md [html[data-perf=on]_&]:bg-obsidian-950/96 [html[data-perf=on]_&]:backdrop-blur-none"
                  {...scrimMotion(reduce)}
                />
              </RadixDialog.Overlay>
              <RadixDialog.Content
                asChild
                forceMount
                aria-describedby={undefined}
                onKeyDown={onKeyDown}
                // The viewer takes the focus, not the first header button (its tooltip would open).
                onOpenAutoFocus={(event) => {
                  event.preventDefault();
                  (event.currentTarget as HTMLElement).focus({ preventScroll: true });
                }}
              >
                <GatedMotion.div
                  className="fixed inset-0 z-50 flex flex-col outline-none"
                  {...dialogMotion(reduce)}
                >
                  <header className="flex items-center gap-2 px-6 pt-4 pb-3">
                    <div className="min-w-0 flex-1">
                      <RadixDialog.Title className="truncate font-display text-[17px] font-semibold text-bone-50">
                        {when}
                      </RadixDialog.Title>
                      <p className="flex min-w-0 gap-3 text-[12px] text-ash-300 tabular-nums">
                        <span className="shrink-0">
                          {t("instance.screenshots.position", { current: index + 1, total: count })}
                        </span>
                        <span className="shrink-0">{formatBytes(shot.sizeBytes)}</span>
                      </p>
                    </div>
                    <Button variant="ghost" icon={<Copy size={16} />} onClick={() => onCopy(shot)}>
                      {t("instance.screenshots.copy")}
                    </Button>
                    <IconButton
                      label={t("instance.screenshots.openViewer")}
                      icon={<ArrowSquareOut size={18} />}
                      onClick={() => onOpen(shot)}
                    />
                    <IconButton
                      label={t("instance.screenshots.reveal")}
                      icon={<FolderOpen size={18} />}
                      onClick={() => onReveal(shot)}
                    />
                    <IconButton
                      label={t("instance.screenshots.delete")}
                      icon={<Trash size={18} />}
                      onClick={() => onDelete(shot)}
                    />
                    <span className="mx-1 h-6 w-px bg-white/10" aria-hidden />
                    <RadixDialog.Close asChild>
                      <IconButton label={t("common.close")} icon={<X size={20} />} />
                    </RadixDialog.Close>
                  </header>
                  <div className="relative flex min-h-0 flex-1 items-center justify-center px-20 pb-8">
                    {picture.src ? (
                      <motion.img
                        key={picture.src}
                        src={picture.src}
                        alt={when}
                        draggable={false}
                        initial={picture.first ? { opacity: 0 } : false}
                        animate={{ opacity: 1 }}
                        transition={{ duration: 0.18 }}
                        className="max-h-full max-w-full rounded-md object-contain shadow-[0_30px_90px_rgb(34_6_6/0.55)]"
                      />
                    ) : null}
                    {count > 1 ? (
                      <>
                        <IconButton
                          label={t("instance.screenshots.previous")}
                          icon={<CaretLeft size={26} />}
                          tooltipSide="right"
                          className="absolute top-1/2 left-5 size-12 -translate-y-1/2 bg-obsidian-900/70"
                          onClick={() => go(-1)}
                        />
                        <IconButton
                          label={t("instance.screenshots.next")}
                          icon={<CaretRight size={26} />}
                          tooltipSide="left"
                          className="absolute top-1/2 right-5 size-12 -translate-y-1/2 bg-obsidian-900/70"
                          onClick={() => go(1)}
                        />
                      </>
                    ) : null}
                  </div>
                </GatedMotion.div>
              </RadixDialog.Content>
            </RadixDialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </RadixDialog.Root>
  );
}
