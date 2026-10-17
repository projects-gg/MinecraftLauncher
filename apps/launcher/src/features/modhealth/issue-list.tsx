import { Warning, WarningCircle } from "@phosphor-icons/react";
import { Button, cn } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { enter, glide, leave, swap, useMotionPrefs } from "@/features/modcenter/motion";
import { Percent } from "@/features/modcenter/parts";
import type { HealthIssue, HealthSeverity } from "@/lib/api";
import { StatusGlyph } from "./status-glyph";
import { fixLabel, issueText } from "./texts";

export function SeverityIcon({ severity, size = 18 }: { severity: HealthSeverity; size?: number }) {
  return severity === "blocking" ? (
    <WarningCircle size={size} weight="fill" className="shrink-0 text-flare-400" aria-hidden />
  ) : (
    <Warning size={size} weight="fill" className="shrink-0 text-gold-300" aria-hidden />
  );
}

export interface FixingState {
  key: string;
  percent?: number;
}

/**
 * Problems of the mod check, one sentence each, worst first. With `onFix` each row offers its
 * fix ("Düzelt", "Bu modu kapat", "Aç"); while it runs the button gives way to a quiet status,
 * and a fixed row fades out while the rows below glide up.
 */
export function IssueList({
  issues,
  minecraft,
  label,
  onFix,
  fixing,
  dense = false,
}: {
  issues: HealthIssue[];
  minecraft: string;
  label: string;
  onFix?: (issue: HealthIssue) => void;
  fixing?: FixingState | null;
  dense?: boolean;
}) {
  const { t } = useTranslation();
  const prefs = useMotionPrefs();
  const shift = (px: number) => (prefs.reduce ? "none" : `translateY(${px}px)`);
  return (
    <ul aria-label={label} className={cn(dense ? "space-y-0.5" : "space-y-1")}>
      <AnimatePresence initial={false}>
        {issues.map((issue) => {
          const working = fixing?.key === issue.key;
          return (
            <motion.li
              key={issue.key}
              layout={prefs.layout ? "position" : false}
              transition={{ layout: glide }}
              initial={{ opacity: 0, transform: shift(4) }}
              animate={{ opacity: 1, transform: shift(0), transition: enter(0.24) }}
              exit={{ opacity: 0, transition: leave(0.22) }}
              className={cn("flex items-center gap-3", dense ? "min-h-8 py-1" : "min-h-10 py-1.5")}
            >
              <SeverityIcon severity={issue.severity} size={dense ? 16 : 18} />
              <p className="min-w-0 flex-1 text-[13.5px] leading-snug text-bone-100">
                {issueText(t, issue, minecraft)}
              </p>
              {onFix && issue.offer ? (
                <span className="relative flex shrink-0 justify-end">
                  <AnimatePresence initial={false} mode="popLayout">
                    {working ? (
                      <motion.span
                        key="working"
                        {...swap(0.18)}
                        className="flex h-8 items-center gap-2 px-1 text-[12.5px] text-gold-200"
                        aria-live="polite"
                      >
                        <StatusGlyph status="running" />
                        {t("modHealth.fix.working")}
                        {typeof fixing?.percent === "number" ? (
                          <Percent value={fixing.percent} />
                        ) : null}
                      </motion.span>
                    ) : (
                      <motion.span key="button" {...swap(0.18)} className="flex">
                        <Button
                          size="sm"
                          variant="secondary"
                          disabled={Boolean(fixing)}
                          onClick={() => onFix(issue)}
                        >
                          {fixLabel(t, issue.offer)}
                        </Button>
                      </motion.span>
                    )}
                  </AnimatePresence>
                </span>
              ) : null}
            </motion.li>
          );
        })}
      </AnimatePresence>
    </ul>
  );
}
