// Home's GitHub star line (STARS; the rules in stars.ts): one quiet glass pill under the friends
// line, in its form (docs/design.md §6, "Home's friends line"): the GitHub mark, one sentence and
// "GitHub'da yıldız ver" drawn inside one button, then ×. Either answer closes it for good.
import { ArrowSquareOut, GithubLogo, Star, X } from "@phosphor-icons/react";
import { easeOutExpo, IconButton } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { shouldShowOnboarding } from "@/features/onboarding/onboarding";
import { ActionLook } from "@/features/social/home-strip";
import { gameBusy } from "@/features/social/join";
import { enter, leave, useSocialMotion } from "@/features/social/motion";
import { openWebLink } from "@/lib/links";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { newShownRuns, SOURCE_URL, starLineVisible, useStars } from "./stars";

/** Runs of this page counted already (each launch once). */
const counted = new Set<number>();

// Counted wherever the player is (Home is loaded with the app): every game the launcher brings on
// screen, once. Nothing is written once the line has shown.
useLaunch.subscribe((state) => {
  for (const run of newShownRuns(state.games, counted)) {
    counted.add(run);
    useStars.getState().count(Date.now());
  }
});

export function StarLine() {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const busy = useLaunch((s) => gameBusy(s.phase.kind));
  // As the shell decides it (the browser preview has no first-start setup).
  const onboarded = !shouldShowOnboarding(usePrefs((s) => s.onboarded));
  const state = useStars((s) => s.state);
  const session = useStars((s) => s.session);
  const show = useStars((s) => s.show);
  const finish = useStars((s) => s.finish);
  const visible = starLineVisible(state, {
    now: Date.now(),
    busy,
    onboarded,
    shownThisSession: session,
  });

  useEffect(() => {
    if (visible) show(Date.now());
  }, [visible, show]);

  return (
    <AnimatePresence initial={false} mode="popLayout">
      {visible ? (
        <motion.div
          key="stars"
          data-star-line
          // Glides with the banners and the friends line above it (LayoutGroup on Home).
          layout={layout ? "position" : false}
          transition={{ layout: { duration: 0.32, ease: easeOutExpo } }}
          className="glass flex w-fit max-w-full items-center gap-1 rounded-full py-1.5 pr-1 pl-1.5"
          initial={{ opacity: 0, transform: reduce ? "none" : "translateY(8px)" }}
          animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.42) }}
          exit={{
            opacity: 0,
            transform: reduce ? "none" : "translateY(4px)",
            transition: leave(0.3),
          }}
        >
          <button
            type="button"
            onClick={() => {
              finish();
              openWebLink(SOURCE_URL);
            }}
            className="group flex min-w-0 cursor-pointer items-center gap-3 rounded-full py-0.5 pr-0.5 pl-2.5 outline-none transition-colors duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300"
          >
            <GithubLogo size={18} weight="fill" className="shrink-0 text-bone-100" aria-hidden />
            <span className="truncate text-[13.5px] font-medium text-bone-50">
              {t("stars.line")}
            </span>
            <ActionLook icon={<Star size={13} weight="fill" aria-hidden />}>
              {/* It opens GitHub: the external-link mark of Home's banners (design.md §6). */}
              <span className="inline-flex items-center gap-1.5">
                {t("stars.action")}
                <ArrowSquareOut
                  size={13}
                  aria-hidden
                  className="shrink-0 text-ash-300 transition-colors duration-150 group-hover:text-gold-300"
                />
              </span>
            </ActionLook>
          </button>
          <IconButton
            size="sm"
            label={t("stars.close")}
            icon={<X size={13} weight="bold" />}
            onClick={finish}
            className="shrink-0 rounded-full"
          />
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
