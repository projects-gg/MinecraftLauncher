import { CheckCircle, MinusCircle, WarningCircle } from "@phosphor-icons/react";
import { Button, cn, Dialog, Progress, SwapLabel, toast } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { AnimatePresence, motion } from "motion/react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { copyErrorDetails } from "@/features/content/jobs";
import { swap } from "@/features/modcenter/motion";
import type { ImportReport, ImportShared, SettingsChange, SourceKind } from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { formatBytes, formatPercent, uiLocale } from "@/lib/format";
import { importPercent, type SettingsUndo, undoSettingsChange, useImportJob } from "./import-job";
import {
  failureDetails,
  PLACE_KEYS,
  type SummaryLine,
  summaryLines,
  summaryTitle,
  type Tone,
} from "./model";

const capitalize = (text: string) => text.charAt(0).toLocaleUpperCase(uiLocale()) + text.slice(1);
const tidy = (text: string) => text.replace(/\s+/g, " ").trim();

/**
 * A summary sentence in words: `what` names the item inside it ("TLauncher'daki dünyaların"),
 * `source` the launcher its things came from ("TLauncher'dan").
 */
function lineText(
  t: TFunction,
  key: string,
  params: Record<string, string | number>,
  what?: ImportShared,
  source?: SourceKind,
) {
  const from = source ? t(`import.from.${source}`) : "";
  const at = source ? t(`import.at.${source}`) : "";
  const words = what ? tidy(t(`import.what.${what}`, { from, at })) : "";
  return tidy(t(`import.${key}`, { ...params, from, at, what: words, What: capitalize(words) }));
}

const TONE_ICON: Record<Tone, { icon: typeof CheckCircle; className: string }> = {
  done: { icon: CheckCircle, className: "text-jade-400" },
  partial: { icon: WarningCircle, className: "text-flare-400" },
  failed: { icon: WarningCircle, className: "text-flare-400" },
  skipped: { icon: MinusCircle, className: "text-ash-400" },
};

/**
 * "Geri al" for imported game settings: takes them back (`undo`), refreshes what showed them, and
 * says why when it is refused (the game is open). The state lives with its summary.
 */
export function useSettingsUndo(
  change: SettingsChange | null | undefined,
  state: SettingsUndo,
  setState: (state: SettingsUndo) => void,
) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  return () => {
    if (!change || state !== "idle") return;
    setState("busy");
    undoSettingsChange(change).then(
      (result) => {
        setState(result);
        void queryClient.invalidateQueries();
      },
      (error: unknown) => {
        setState("idle");
        toast.error(
          error instanceof Error && error.message ? error.message : t("import.done.titleFailed"),
        );
      },
    );
  };
}

/**
 * A summary line that offers "Geri al" for imported game settings. Taken back, the sentence
 * cross-fades in place to say so and the button fades from its slot (the slot keeps its width,
 * so the text beside it never reflows); a change that can no longer be taken back says so under
 * the line.
 */
export function SettingsUndoLine({
  children,
  icon,
  state,
  onUndo,
  className,
}: {
  /** The line's sentence while the settings stand. */
  children: ReactNode;
  icon: ReactNode;
  state: SettingsUndo;
  onUndo: () => void;
  className?: string;
}) {
  const { t } = useTranslation();
  const undone = state === "done";
  return (
    <li className={cn("flex gap-2.5 text-[13.5px] leading-snug text-bone-100", className)}>
      <span className="relative mt-px size-[18px] shrink-0">
        <AnimatePresence initial={false} mode="popLayout">
          <motion.span key={undone ? "undone" : "came"} className="absolute inset-0" {...swap()}>
            {undone ? (
              <MinusCircle size={18} weight="fill" className="text-ash-400" aria-hidden />
            ) : (
              icon
            )}
          </motion.span>
        </AnimatePresence>
      </span>
      <span className="relative min-w-0 flex-1" aria-live="polite">
        <AnimatePresence initial={false} mode="popLayout">
          <motion.span key={undone ? "undone" : "came"} className="block" {...swap()}>
            {undone ? t("import.undo.done") : children}
          </motion.span>
        </AnimatePresence>
        {state === "gone" ? (
          <span className="block text-[12.5px] text-ash-400">{t("import.undo.gone")}</span>
        ) : null}
      </span>
      {/* A fixed slot (the button's height too): the line keeps its size when the button goes. */}
      <span className="-my-1 flex h-7 w-[76px] shrink-0 items-center justify-end">
        <AnimatePresence initial={false}>
          {state === "idle" || state === "busy" ? (
            <motion.span key="undo" {...swap()}>
              <Button size="sm" variant="ghost" disabled={state === "busy"} onClick={onUndo}>
                {t("import.undo.action")}
              </Button>
            </motion.span>
          ) : null}
        </AnimatePresence>
      </span>
    </li>
  );
}

/** What came over, item by item: done in jade, missing pieces in flare, the rest quiet. */
export function ImportSummary({ report }: { report: ImportReport }) {
  const { t } = useTranslation();
  const settingsUndo = useImportJob((s) => s.settingsUndo);
  const setSettingsUndo = useImportJob((s) => s.setSettingsUndo);
  const undo = useSettingsUndo(report.settings, settingsUndo, setSettingsUndo);
  const lines = summaryLines(report);
  return (
    <ul className="space-y-2.5 pt-1">
      {lines.map((line: SummaryLine) => {
        const { icon: Icon, className } = TONE_ICON[line.tone];
        // Where the worlds went reads as one list; the other follow-ups are sentences.
        const places = line.more
          .filter((m) => PLACE_KEYS.includes(m.key))
          .map((m) => lineText(t, m.key, m.params, m.what))
          .join(", ");
        const sentences = line.more
          .filter((m) => !PLACE_KEYS.includes(m.key))
          .map((m) => lineText(t, m.key, m.params, m.what));
        const more = [places && `${places}.`, ...sentences].filter(Boolean).join(" ");
        const icon = (
          <Icon size={18} weight="fill" className={cn("mt-px shrink-0", className)} aria-hidden />
        );
        const text = (
          <>
            {lineText(t, line.key, line.params, line.what, line.source)}
            {more ? <span className="block text-[12.5px] text-ash-400">{more}</span> : null}
          </>
        );
        if (line.undo) {
          return (
            <SettingsUndoLine
              key={line.id}
              icon={<Icon size={18} weight="fill" className={className} aria-hidden />}
              state={settingsUndo}
              onUndo={undo}
            >
              {text}
            </SettingsUndoLine>
          );
        }
        return (
          <li key={line.id} className="flex gap-2.5 text-[13.5px] leading-snug text-bone-100">
            {icon}
            <span className="min-w-0">{text}</span>
          </li>
        );
      })}
    </ul>
  );
}

/** The summary's title for a finished run (or the run's failure). */
export function useSummaryTitle(): string {
  const { t } = useTranslation();
  const report = useImportJob((s) => s.report);
  const status = useImportJob((s) => s.status);
  if (status === "failed" || !report) return t("import.done.titleFailed");
  return t(`import.done.${summaryTitle(report)}`);
}

export function failureMessage(error: unknown, t: TFunction): string {
  return error instanceof Error && error.message ? error.message : t("import.done.titleFailed");
}

/** The summary of the last run (from the status card or the toast). */
export function ImportSummaryDialog() {
  const { t } = useTranslation();
  const open = useImportJob((s) => s.summaryOpen);
  const setOpen = useImportJob((s) => s.setSummaryOpen);
  const report = useImportJob((s) => s.report);
  const error = useImportJob((s) => s.error);
  const title = useSummaryTitle();
  const details = report ? failureDetails(report) : "";
  return (
    <Dialog
      open={open && (report !== undefined || error !== undefined)}
      onOpenChange={setOpen}
      title={title}
      closeLabel={t("common.close")}
      width={520}
      footer={
        <>
          {details || error ? (
            <Button
              variant="ghost"
              className="mr-auto"
              onClick={() =>
                details
                  ? void copyText(details).then(
                      () => toast.success(t("common.detailsCopied")),
                      () => toast.error(t("common.copyFailed")),
                    )
                  : void copyErrorDetails(error)
              }
            >
              {t("common.copyDetails")}
            </Button>
          ) : null}
          <Button variant="primary" onClick={() => setOpen(false)}>
            {t("import.done.close")}
          </Button>
        </>
      }
    >
      {report ? (
        <ImportSummary report={report} />
      ) : (
        <p className="pt-1 text-[13.5px] text-bone-100">{failureMessage(error, t)}</p>
      )}
    </Dialog>
  );
}

/**
 * The running import in a small glass card (the welcome flow shows it while the player goes on):
 * what it does and how far it is, with "İptal"; once done, the title and "Özet". The two states
 * cross-fade in one place; the bar fills by transform.
 */
export function ImportStatusCard({ className }: { className?: string }) {
  const { t } = useTranslation();
  const status = useImportJob((s) => s.status);
  const progress = useImportJob((s) => s.progress);
  const stopping = useImportJob((s) => s.stopping);
  const cancel = useImportJob((s) => s.cancel);
  const setSummaryOpen = useImportJob((s) => s.setSummaryOpen);
  const report = useImportJob((s) => s.report);
  const title = useSummaryTitle();
  if (status === "idle") return null;
  const percent = importPercent(progress);
  const running = status === "running";
  const tone: Tone = running
    ? "done"
    : status === "failed" || !report
      ? "failed"
      : summaryTitle(report) === "title"
        ? "done"
        : summaryTitle(report) === "titleCancelled"
          ? "skipped"
          : "partial";
  const { icon: Icon, className: iconClass } = TONE_ICON[tone];
  return (
    <div
      role="status"
      className={cn("glass rounded-[var(--radius-panel)] px-4 py-3 ring-1 ring-white/8", className)}
    >
      <AnimatePresence mode="popLayout" initial={false}>
        {running ? (
          <motion.div key="running" {...swap()}>
            <div className="flex items-center gap-3">
              <p className="min-w-0 flex-1 truncate text-[13.5px] font-medium text-bone-50">
                {progress && progress.stage !== "preparing"
                  ? t("import.progress.title")
                  : t("import.progress.preparing")}
              </p>
              <span className="min-w-[4ch] text-right font-display text-[15px] font-semibold text-gold-200 tabular-nums">
                {percent !== undefined ? formatPercent(Math.floor(percent)) : ""}
              </span>
              <Button
                size="sm"
                variant="ghost"
                className="-my-1 -mr-2"
                disabled={stopping}
                onClick={cancel}
              >
                <SwapLabel
                  labels={[t("import.progress.cancel"), t("import.progress.stopping")]}
                  active={stopping ? 1 : 0}
                />
              </Button>
            </div>
            <Progress className="mt-2" value={percent} label={t("import.progress.label")} />
            <div className="mt-1.5 flex min-h-[16px] justify-between gap-3 text-[12px] text-ash-400 tabular-nums">
              <span className="min-w-0 truncate">{progress?.current ?? ""}</span>
              <span className="shrink-0">
                {progress && progress.totalBytes > 0 && progress.stage !== "preparing"
                  ? `${formatBytes(Math.min(progress.doneBytes, progress.totalBytes))} / ${formatBytes(progress.totalBytes)}`
                  : ""}
              </span>
            </div>
          </motion.div>
        ) : (
          <motion.div key="done" {...swap()} className="flex items-center gap-3">
            <Icon size={20} weight="fill" className={cn("shrink-0", iconClass)} aria-hidden />
            <p className="min-w-0 flex-1 text-[13.5px] font-medium text-bone-50">{title}</p>
            <Button size="sm" variant="secondary" onClick={() => setSummaryOpen(true)}>
              {t("import.done.summary")}
            </Button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
