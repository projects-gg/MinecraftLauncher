import { Wrench } from "@phosphor-icons/react";
import { Button, toast } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { newJobId, showError } from "@/features/content/jobs";
import { glide, swap, useMotionPrefs } from "@/features/modcenter/motion";
import { api, type HealthIssue, type ModCheck } from "@/lib/api";
import { refreshAfterRepair, useModFlow } from "./flow";
import { type FixingState, IssueList, SeverityIcon } from "./issue-list";
import { stepNote, stepText } from "./texts";

const FIRST = 3;

/**
 * The mod check's problems in the mod center's "Sorunlar" strip: what would stop the game (or
 * only warn), each with its fix, and "Onar" for all of them at once (or "Yeniden yükle" when a
 * repair has nothing left to try). A fixed row fades out; the strip goes when nothing is left.
 */
export function ModHealthSection({ instanceId, check }: { instanceId: string; check: ModCheck }) {
  const { t } = useTranslation();
  const prefs = useMotionPrefs();
  const queryClient = useQueryClient();
  const openFlow = useModFlow((s) => s.open);
  const [expanded, setExpanded] = useState(false);
  const [fixing, setFixing] = useState<FixingState | null>(null);
  const blocking = check.blocking > 0;
  const shown = expanded ? check.issues : check.issues.slice(0, FIRST);
  const position = prefs.layout ? ("position" as const) : false;

  const fix = async (issue: HealthIssue) => {
    setFixing({ key: issue.key });
    try {
      const result = await api.modHealth.repair(
        instanceId,
        { kind: "issue", key: issue.key },
        newJobId(),
        (p) => setFixing({ key: issue.key, percent: p.percent }),
      );
      const failed = result.steps.find((s) => s.status === "failed");
      if (failed) {
        toast(stepText(t, failed), { description: stepNote(t, failed) });
      }
    } catch (error) {
      showError(error);
    } finally {
      setFixing(null);
      refreshAfterRepair(queryClient, instanceId);
    }
  };

  return (
    <div className="pt-0.5 pb-1">
      <div className="flex min-h-9 items-center gap-2.5">
        <SeverityIcon severity={blocking ? "blocking" : "warning"} />
        <p className="relative min-w-0 flex-1 text-[13.5px] font-semibold text-bone-50">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span key={blocking ? "blocked" : "warnings"} {...swap(0.18)} className="block">
              {blocking ? t("modHealth.banner.blocked") : t("modHealth.banner.warnings")}
            </motion.span>
          </AnimatePresence>
        </p>
        {check.repairable ? (
          <Button
            size="sm"
            variant="secondary"
            icon={<Wrench size={15} weight="fill" className="text-gold-300" />}
            disabled={Boolean(fixing)}
            onClick={() =>
              openFlow(instanceId, "center", { kind: "repair", scope: { kind: "all" } })
            }
          >
            {t("modHealth.banner.repair")}
          </Button>
        ) : blocking && check.hasMods ? (
          <Button
            size="sm"
            variant="secondary"
            disabled={Boolean(fixing)}
            onClick={() => openFlow(instanceId, "center", { kind: "reinstall" })}
          >
            {t("modHealth.banner.reinstall")}
          </Button>
        ) : null}
      </div>
      <div className="pl-7">
        <IssueList
          issues={shown}
          minecraft={check.minecraft}
          label={blocking ? t("modHealth.banner.blocked") : t("modHealth.banner.warnings")}
          onFix={(issue) => void fix(issue)}
          fixing={fixing}
          dense
        />
        {check.issues.length > FIRST ? (
          <motion.div layout={position} transition={{ layout: glide }}>
            <Button
              size="sm"
              variant="ghost"
              className="-ml-3"
              aria-expanded={expanded}
              onClick={() => setExpanded((v) => !v)}
            >
              {expanded
                ? t("modHealth.banner.showLess")
                : t("modHealth.banner.showAll", { count: check.issues.length })}
            </Button>
          </motion.div>
        ) : null}
      </div>
    </div>
  );
}
