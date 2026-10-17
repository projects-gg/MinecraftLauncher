// "N yeni ekran görüntüsü": after a game session that took screenshots, a toast with the newest
// ones fanned out like prints; "Göster" opens that instance's gallery.
import { Button, toast } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { motion, useReducedMotion } from "motion/react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
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
      <p className="min-w-0 flex-1 text-[13.5px] leading-snug font-semibold text-bone-50">
        {t("shots.title", { count: shots.count })}
      </p>
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
