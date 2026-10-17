// The Java question before a launch (J1): the instance's custom Java does not suit the game.
// "Kur ve oyna" launches again and the launcher switches the instance (to "Otomatik", or to its
// own Java the mods need); "Yine de dene" launches with the custom Java and is not asked again
// this session; Escape or the close button cancels. Mounted once by the app shell.
import { ArrowRight, DownloadSimple, WarningCircle } from "@phosphor-icons/react";
import { Button, cn, Dialog, easeOutExpo } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { motion } from "motion/react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { api, type JavaChoice } from "@/lib/api";
import { useLaunch } from "@/stores/launch";
import { useJavaPrompt } from "./prompt-store";
import { promptBody, promptTitle } from "./texts";

/** One side of the comparison: the instance's Java, or the one the game needs. */
function Tile({
  label,
  major,
  muted = false,
  delay,
}: {
  label: string;
  /** Missing or not working: a warning instead of a version. */
  major?: number;
  muted?: boolean;
  delay: number;
}) {
  return (
    <motion.div
      initial={{ opacity: 0, transform: "translateY(4px)" }}
      animate={{
        opacity: 1,
        transform: "translateY(0px)",
        transition: { duration: 0.32, ease: easeOutExpo, delay },
      }}
      className="min-w-0 rounded-[10px] bg-obsidian-950/45 px-4 pt-3 pb-2.5 ring-1 ring-white/7"
    >
      <div className="flex h-[32px] items-end gap-1.5">
        {major === undefined ? (
          <WarningCircle size={28} weight="fill" className="mb-px text-flare-400" aria-hidden />
        ) : (
          <>
            <span className="pb-[3px] text-[12.5px] font-medium text-ash-400">Java</span>
            <span
              className={cn(
                "font-display text-[30px] leading-none font-semibold tabular-nums",
                muted ? "text-ash-300" : "text-bone-50",
              )}
            >
              {major}
            </span>
          </>
        )}
      </div>
      <p className="mt-1.5 truncate text-[12px] font-medium text-ash-400">{label}</p>
    </motion.div>
  );
}

/** Java changes made anywhere ("Kur ve oyna", an update, "Otomatik'e geç") refresh what shows them. */
function useJavaChanges() {
  const queryClient = useQueryClient();
  useEffect(
    () =>
      api.java.onChange(() => {
        void queryClient.invalidateQueries({ queryKey: ["java-overview"] });
        void queryClient.invalidateQueries({ queryKey: ["instance"] });
      }),
    [queryClient],
  );
}

export function JavaPromptDialog() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  useJavaChanges();
  const request = useJavaPrompt((s) => s.request);
  const close = useJavaPrompt((s) => s.close);
  // Keeps the question on screen while the dialog animates out.
  const shown = useRef(request);
  if (request) shown.current = request;
  const prompt = shown.current?.prompt;

  const answer = (choice: JavaChoice) => {
    const current = useJavaPrompt.getState().request;
    if (!current) return;
    close();
    void useLaunch.getState().play(current.prompt.instanceId, { ...current.options, java: choice });
  };
  const openSettings = () => {
    const current = useJavaPrompt.getState().request;
    if (!current) return;
    close();
    void navigate({
      to: "/library/$instanceId",
      params: { instanceId: current.prompt.instanceId },
      search: { tab: "settings" },
    });
  };

  return (
    <Dialog
      open={Boolean(request)}
      onOpenChange={(open) => {
        if (!open) close();
      }}
      title={prompt ? promptTitle(t, prompt) : ""}
      description={prompt ? promptBody(t, prompt) : undefined}
      closeLabel={t("javaPrompt.cancel")}
      width={440}
      footer={
        prompt ? (
          <>
            <Button variant="ghost" onClick={() => answer("tryAnyway")}>
              {t("javaPrompt.tryAnyway")}
            </Button>
            {prompt.fixable ? (
              <Button
                variant="primary"
                icon={<DownloadSimple size={16} weight="bold" />}
                onClick={() => answer("install")}
                // The answer most players want; Enter takes it.
                autoFocus
              >
                {t("javaPrompt.install")}
              </Button>
            ) : (
              <Button variant="secondary" onClick={openSettings}>
                {t("javaPrompt.openSettings")}
              </Button>
            )}
          </>
        ) : null
      }
    >
      {prompt ? (
        <div className="grid grid-cols-[1fr_auto_1fr] items-center gap-3 pt-1.5">
          <Tile label={t("javaPrompt.current")} major={prompt.have} muted delay={0.05} />
          <motion.span
            aria-hidden
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: { duration: 0.24, delay: 0.1 } }}
            className="text-ash-400"
          >
            <ArrowRight size={18} />
          </motion.span>
          <Tile
            label={prompt.forMods ? t("javaPrompt.neededMods") : t("javaPrompt.needed")}
            major={prompt.need}
            delay={0.12}
          />
        </div>
      ) : null}
    </Dialog>
  );
}
