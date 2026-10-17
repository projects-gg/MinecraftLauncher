import { CaretDown, Copy, WarningCircle } from "@phosphor-icons/react";
import { Button, cn } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ModIssue } from "@/lib/api";
import { copyDetails } from "@/lib/clipboard";
import { issueLines } from "./issue-lines";
import { glide, rise, useMotionPrefs } from "./motion";

/**
 * "Sorunlar" strip at the top of the mod center: files the launcher could not read or change
 * (they never stop a sync or a launch) and a mods folder found empty. Pre-launch mod checks and
 * their repair actions plug in through `children` (rendered above the file list). It lives in
 * the mod center's layout group: opening the details glides the content below.
 */
export function ModIssuesBanner({
  issues,
  reset,
  children,
}: {
  issues: ModIssue[];
  reset: boolean;
  children?: ReactNode;
}) {
  const { t } = useTranslation();
  const prefs = useMotionPrefs();
  const [open, setOpen] = useState(false);
  if (issues.length === 0 && !reset && !children) return null;

  const copy = () =>
    void copyDetails(issues.map((i) => `${i.folder}/${i.fileName}: ${i.detail}`).join("\n"));
  const position = prefs.layout ? ("position" as const) : false;

  return (
    <motion.section
      layout={prefs.layout}
      transition={{ layout: glide }}
      style={{ borderRadius: 10 }}
      aria-label={t("modCenter.issues.label")}
      className="glass mb-4 border border-flare-400/25 px-4 py-2"
    >
      {children ? (
        <motion.div layout={position} transition={{ layout: glide }}>
          {children}
        </motion.div>
      ) : null}
      {reset ? (
        <motion.p
          layout={position}
          transition={{ layout: glide }}
          className={cn(
            "flex min-h-9 items-center gap-2.5 text-[13.5px] text-bone-100",
            children && "border-white/6 border-t",
          )}
        >
          <WarningCircle size={18} className="shrink-0 text-gold-300" aria-hidden />
          {t("modCenter.issues.reset")}
        </motion.p>
      ) : null}
      {issues.length > 0 ? (
        <motion.div
          layout={position}
          transition={{ layout: glide }}
          className={cn("relative", (reset || children) && "border-t border-white/6")}
        >
          <div className="flex min-h-9 items-center gap-2.5">
            <WarningCircle size={18} className="shrink-0 text-flare-400" aria-hidden />
            <p className="min-w-0 flex-1 text-[13.5px] text-ash-400">
              <span className="font-semibold text-bone-50">
                {t("modCenter.issues.title", { count: issues.length })}.
              </span>{" "}
              {t("modCenter.issues.body")}
            </p>
            <Button
              size="sm"
              variant="ghost"
              aria-expanded={open}
              icon={
                <CaretDown
                  size={14}
                  className={cn(
                    "transition-transform duration-200 ease-[cubic-bezier(0.16,1,0.3,1)]",
                    open && "rotate-180",
                  )}
                />
              }
              onClick={() => setOpen((v) => !v)}
            >
              {open ? t("modCenter.issues.hide") : t("modCenter.issues.show")}
            </Button>
          </div>
          <AnimatePresence initial={false} mode="popLayout">
            {open ? (
              <motion.div key="details" {...rise(prefs.reduce, 4, 0.2)} className="pb-2 pl-7">
                {/* Titles and kinds of files only; file names go to "Ayrıntıları kopyala". */}
                <ul className="space-y-1 text-[12.5px] text-ash-300">
                  {issueLines(t, issues).map((line) => (
                    <li key={line}>{line}</li>
                  ))}
                </ul>
                <Button
                  size="sm"
                  variant="ghost"
                  className="mt-1.5 -ml-3"
                  icon={<Copy size={14} />}
                  onClick={copy}
                >
                  {t("common.copyDetails")}
                </Button>
              </motion.div>
            ) : null}
          </AnimatePresence>
        </motion.div>
      ) : null}
    </motion.section>
  );
}
