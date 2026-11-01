import { DownloadSimple } from "@phosphor-icons/react";
import { Button, cn, Dialog, Skeleton, SwapLabel, toast } from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { SettingsUndo } from "@/features/import/import-job";
import { SettingsUndoLine, ToneMark, useSettingsUndo } from "@/features/import/import-status";
import type { Tone } from "@/features/import/model";
import { Check } from "@/features/settings/cleanup/check";
import { api, type LegacyImportReport, type LegacyScan } from "@/lib/api";
import { formatBytes, listFormat, uiLocale } from "@/lib/format";
import { broughtThings } from "./summary";

/**
 * One choice of the v1 import, written as the other launchers' rows are (a title and a quiet
 * second line): the app's own chamfered checkbox (bone when ticked: the dialog's one gold is
 * "Aktar", design.md §2), its words the box's label. A choice that cannot be made dims its box and
 * title; its hint says why.
 */
function Choice({
  checked,
  onChange,
  label,
  meta,
  hint,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  meta?: string;
  hint?: string;
  disabled?: boolean;
}) {
  const id = useId();
  return (
    <label
      htmlFor={id}
      className="flex items-center gap-3 rounded-md px-2 py-[7px] transition-colors duration-150 has-[button:enabled]:cursor-pointer has-[button:enabled]:hover:bg-white/4"
    >
      <Check id={id} checked={checked} disabled={disabled} onCheckedChange={onChange} />
      <span className="min-w-0">
        <span
          className={cn(
            "block text-[13.5px] leading-tight",
            disabled ? "text-ash-300" : "text-bone-50",
          )}
        >
          {label}
        </span>
        {meta ? (
          <span className="mt-0.5 block text-[12px] text-ash-400 tabular-nums">{meta}</span>
        ) : null}
        {hint ? <span className="mt-0.5 block text-[12px] text-ash-400">{hint}</span> : null}
      </span>
    </label>
  );
}

/**
 * The summary's lines after the settings one, each with its honest mark: what came, and the
 * backend's notes (something picked did not come, said in the launcher's language) in flare.
 * The game data is named by what it brought ("Dünyaların, ekran görüntülerin ve paketlerin"), not
 * by a count of files; when a note says something stayed behind it names no kind.
 */
function summaryLines(
  t: TFunction,
  report: LegacyImportReport,
  scan: LegacyScan | undefined,
  worlds: boolean,
): Array<{ tone: Tone; text: string }> {
  const lines: Array<{ tone: Tone; text: string }> = [];
  if (report.premiumImported) lines.push({ tone: "done", text: t("migrate.donePremium") });
  if (report.nickname) {
    lines.push({ tone: "done", text: t("migrate.doneNickname", { name: report.nickname }) });
  }
  if (report.afkAccounts > 0) {
    lines.push({ tone: "done", text: t("migrate.doneAfk", { count: report.afkAccounts }) });
  }
  if (report.copiedFiles + report.linkedFiles > 0) {
    const things = broughtThings(report, scan, worlds);
    const text = things
      ? t("import.v1.files", {
          things: listFormat().format(things.map((thing) => t(`import.v1.things.${thing}`))),
        })
      : t("import.v1.filesSome");
    lines.push({
      tone: "done",
      text: text.charAt(0).toLocaleUpperCase(uiLocale()) + text.slice(1),
    });
  }
  for (const note of report.notes) lines.push({ tone: "failed", text: note });
  return lines;
}

/** One-time import from the v1 launcher. Offered on first start; also in Settings > Data. */
export function MigrationDialog({
  open,
  onOpenChange,
  onFinished,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Called when the dialog closes: whether an import ran (the welcome flow moves on then). */
  onFinished?: (imported: boolean) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: scan, isPending } = useQuery({
    queryKey: ["legacy-scan"],
    queryFn: () => api.legacyScan(),
    enabled: open,
  });
  const [premium, setPremium] = useState(true);
  const [nickname, setNickname] = useState(true);
  const [afk, setAfk] = useState(true);
  const [gameData, setGameData] = useState(true);
  const [worlds, setWorlds] = useState(true);
  const [report, setReport] = useState<LegacyImportReport | null>(null);
  // The game settings it brought can be taken back from the summary (IMPORT-SET).
  const [settingsUndo, setSettingsUndo] = useState<SettingsUndo>("idle");
  const undoSettings = useSettingsUndo(report?.settings, settingsUndo, setSettingsUndo);

  const worldsFit = !scan?.freeBytes || scan.worldsBytes < scan.freeBytes - 2 * 1024 ** 3;
  useEffect(() => {
    if (scan && !worldsFit) setWorlds(false);
  }, [scan, worldsFit]);

  const run = useMutation({
    mutationFn: () =>
      api.legacyImport({ premium, nickname, afk, gameData, worlds: worlds && gameData }),
    onSuccess: (result) => {
      setReport(result);
      setSettingsUndo("idle");
      void queryClient.invalidateQueries();
    },
    onError: (error) => toast.error(error instanceof Error ? error.message : String(error)),
  });

  const close = () => {
    onOpenChange(false);
    onFinished?.(report !== null);
    setReport(null);
  };

  // Something picked did not come (a note): the title says so, as the import summary's does.
  const title = report
    ? t(report.notes.length > 0 ? "import.done.titleSome" : "import.done.title")
    : t("migrate.title");

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => (next ? onOpenChange(true) : close())}
      title={title}
      description={report ? undefined : t("migrate.description")}
      closeLabel={t("common.close")}
      width={540}
      // Choices imported as a whole: a stray click beside the dialog keeps them.
      dismissOnOutsideClick={false}
      footer={
        report ? (
          <Button variant="primary" onClick={close}>
            {t("migrate.close")}
          </Button>
        ) : (
          <>
            <Button variant="ghost" onClick={close}>
              {t("migrate.later")}
            </Button>
            <Button
              variant="primary"
              icon={<DownloadSimple size={16} weight="bold" />}
              disabled={!scan?.found || run.isPending}
              onClick={() => run.mutate()}
            >
              <SwapLabel
                labels={[t("migrate.start"), t("migrate.working")]}
                active={run.isPending ? 1 : 0}
              />
            </Button>
          </>
        )
      }
    >
      {report ? (
        <ul className="space-y-2.5 pt-1 text-[14px] text-bone-100">
          {report.settings ? (
            <SettingsUndoLine
              icon={<ToneMark tone="done" />}
              state={settingsUndo}
              onUndo={undoSettings}
              className="text-[14px]"
            >
              {t("import.undo.v1")}
            </SettingsUndoLine>
          ) : null}
          {summaryLines(t, report, scan, worlds && gameData).map((line) => (
            <li key={line.text} className="flex gap-2.5">
              <ToneMark tone={line.tone} className="mt-px" />
              <span className="min-w-0">{line.text}</span>
            </li>
          ))}
        </ul>
      ) : isPending ? (
        <div className="space-y-3 pt-1">
          <Skeleton className="h-5 w-3/4" />
          <Skeleton className="h-5 w-2/3" />
          <Skeleton className="h-5 w-4/5" />
        </div>
      ) : !scan?.found ? (
        <p className="text-ash-300">{t("migrate.notFound")}</p>
      ) : (
        // The boxes stand on the text's edge; the rows' hover plate reaches past it.
        <div className="-mx-2 space-y-0.5 pt-1">
          {scan.hasPremium ? (
            <Choice
              checked={premium}
              onChange={setPremium}
              label={t("import.v1.choices.premium")}
            />
          ) : null}
          {scan.nickname ? (
            <Choice
              checked={nickname}
              onChange={setNickname}
              label={t("import.v1.choices.nickname")}
              meta={scan.nickname}
            />
          ) : null}
          {scan.afkAccounts > 0 || scan.hasAfkClient ? (
            <Choice
              checked={afk}
              onChange={setAfk}
              label={t("import.v1.choices.afk")}
              meta={
                scan.afkAccounts > 0
                  ? t("import.v1.choices.afkMeta", { count: scan.afkAccounts })
                  : undefined
              }
            />
          ) : null}
          <Choice
            checked={gameData}
            onChange={setGameData}
            label={t("import.v1.choices.gameData")}
            meta={t("import.v1.choices.gameDataMeta")}
          />
          {scan.worlds > 0 ? (
            <Choice
              checked={worlds && gameData}
              onChange={setWorlds}
              disabled={!gameData || !worldsFit}
              label={t("import.v1.choices.worlds")}
              meta={t("import.v1.choices.worldsMeta", {
                count: scan.worlds,
                size: formatBytes(scan.worldsBytes),
              })}
              hint={worldsFit ? undefined : t("migrate.worldsNoSpace")}
            />
          ) : null}
        </div>
      )}
    </Dialog>
  );
}
