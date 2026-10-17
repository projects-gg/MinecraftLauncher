// "N yeni ekran görüntüsü": after a game session that took screenshots, a toast with the newest
// ones fanned out like prints. "Kopyala" puts the newest on the clipboard (straight into Discord
// or WhatsApp); "Göster" opens that instance's gallery.
import { Check, CopySimple } from "@phosphor-icons/react";
import { Button, easeOutExpo, toast } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { type NewScreenshots, useLaunch } from "@/stores/launch";

/** Where each print rests in the fan, back to front, by how many there are. */
const fans: Record<number, string[]> = {
  1: ["translate(0px, 0px) rotate(0deg)"],
  2: ["translate(-8px, 2px) rotate(-8deg)", "translate(0px, 0px) rotate(0deg)"],
  3: [
    "translate(-9px, 2px) rotate(-9deg)",
    "translate(9px, 3px) rotate(7deg)",
    "translate(0px, 0px) rotate(0deg)",
  ],
};

function Prints({ urls }: { urls: string[] }) {
  const reduce = useReducedMotion() ?? false;
  // The newest print lies on top.
  const stack = urls.slice(0, 3).reverse();
  const places = fans[stack.length] ?? [];
  return (
    <span aria-hidden className="relative block h-11 w-[68px] shrink-0">
      {stack.map((url, index) => {
        const rest = places[index] ?? "none";
        return (
          <motion.img
            key={url}
            src={url}
            alt=""
            draggable={false}
            className="absolute top-1/2 left-1/2 -mt-[15px] -ml-6 h-[30px] w-12 rounded-[3px] object-cover shadow-[0_4px_12px_rgb(10_9_12/0.55)] ring-1 ring-white/20"
            // The prints arrive stacked and spread once the toast has landed.
            initial={{
              opacity: 0,
              transform: reduce ? rest : "translate(0px, 6px) rotate(0deg)",
            }}
            animate={{
              opacity: 1,
              transform: rest,
              transition: reduce
                ? { duration: 0.2 }
                : { type: "spring", stiffness: 260, damping: 22, delay: 0.12 + index * 0.05 },
            }}
          />
        );
      })}
    </span>
  );
}

/**
 * "Kopyala": the newest screenshot goes on the clipboard and the label turns into "Kopyalandı"
 * in place for a moment, as on the server pill. The button keeps the longer label's width, so
 * "Göster" beside it never moves.
 */
function CopyNewest({ instanceId, fileName }: { instanceId: string; fileName: string }) {
  const { t } = useTranslation();
  const reduce = useReducedMotion() ?? false;
  const [copied, setCopied] = useState(false);
  const busy = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  const copy = async () => {
    // A double click copies once.
    if (busy.current) return;
    busy.current = true;
    try {
      const result = await api.library.copyScreenshot(instanceId, fileName);
      if (result === "path") {
        // Only the file's path reached the clipboard: say so rather than "Kopyalandı".
        toast(t("instance.screenshots.copiedPath"));
        return;
      }
      setCopied(true);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1600);
    } catch {
      toast.error(t("common.copyFailed"));
    } finally {
      busy.current = false;
    }
  };

  return (
    <Button size="sm" variant="ghost" onClick={() => void copy()}>
      <span className="grid justify-items-center *:[grid-area:1/1]">
        <motion.span
          className="flex items-center gap-1.5"
          initial={false}
          animate={{
            opacity: copied ? 0 : 1,
            transition: { duration: copied ? 0.1 : 0.2, ease: easeOutExpo },
          }}
        >
          <CopySimple size={14} aria-hidden />
          {t("shots.copy")}
        </motion.span>
        {/* Holds the width of the longer label. */}
        <span aria-hidden className="invisible flex items-center gap-1.5">
          <Check size={14} weight="bold" />
          {t("shots.copied")}
        </span>
        <AnimatePresence>
          {copied ? (
            <motion.span
              key="copied"
              aria-hidden
              className="flex items-center gap-1.5 text-jade-400"
              initial={{ opacity: 0, transform: reduce ? "none" : "translateY(5px)" }}
              animate={{
                opacity: 1,
                transform: reduce ? "none" : "translateY(0px)",
                transition: { duration: 0.22, ease: easeOutExpo, delay: 0.05 },
              }}
              exit={{
                opacity: 0,
                transform: reduce ? "none" : "translateY(-4px)",
                transition: { duration: 0.14, ease: easeOutExpo },
              }}
            >
              <Check size={14} weight="bold" />
              {t("shots.copied")}
            </motion.span>
          ) : null}
        </AnimatePresence>
      </span>
      <span className="sr-only" aria-live="polite">
        {copied ? t("shots.copied") : ""}
      </span>
    </Button>
  );
}

function ShotsToast({
  toastId,
  shots,
  onOpen,
}: {
  toastId: string | number;
  shots: NewScreenshots;
  onOpen: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex w-full items-center gap-3 font-normal">
      <Prints urls={shots.recent} />
      <div className="flex min-w-0 flex-1 flex-col gap-2">
        <p className="text-[13.5px] leading-snug font-semibold text-bone-50">
          {t("shots.title", { count: shots.count })}
        </p>
        <div className="flex items-center justify-end gap-1.5">
          {shots.newest ? (
            <CopyNewest instanceId={shots.instanceId} fileName={shots.newest} />
          ) : null}
          <Button
            size="sm"
            variant="secondary"
            onClick={() => {
              toast.dismiss(toastId);
              onOpen();
            }}
          >
            {t("shots.show")}
          </Button>
        </div>
      </div>
    </div>
  );
}

/** Announces the screenshots of the session that just ended (mounted once in the shell). */
export function ScreenshotNotice() {
  const shots = useLaunch((s) => s.newShots);
  const clear = useLaunch((s) => s.clearNewShots);
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  useEffect(() => {
    if (!shots) return;
    clear();
    void queryClient.invalidateQueries({ queryKey: ["screenshots", shots.instanceId] });
    void queryClient.invalidateQueries({ queryKey: ["instance", shots.instanceId] });
    const open = () =>
      void navigate({
        to: "/library/$instanceId",
        params: { instanceId: shots.instanceId },
        search: { tab: "screenshots" },
      });
    toast.custom((id) => <ShotsToast toastId={id} shots={shots} onOpen={open} />, {
      id: `shots:${shots.at}`,
      duration: 9000,
      classNames: { content: "min-w-0 flex-1", title: "w-full" },
    });
  }, [shots, clear, navigate, queryClient]);

  return null;
}
