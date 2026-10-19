import { CheckCircle, DownloadSimple } from "@phosphor-icons/react";
import { Button, Dialog, Skeleton, SwapLabel, toast } from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type LegacyImportReport } from "@/lib/api";
import { formatBytes } from "@/lib/format";

function Check({
  checked,
  onChange,
  label,
  hint,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  hint?: string;
  disabled?: boolean;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-3 rounded-md px-2 py-2 hover:bg-white/4 has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-50">
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
        className="mt-0.5 size-4 accent-[var(--color-gold-500)]"
      />
      <span>
        <span className="block text-[14px] text-bone-50">{label}</span>
        {hint ? <span className="block text-[12.5px] text-ash-400">{hint}</span> : null}
      </span>
    </label>
  );
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

  const worldsFit = !scan?.freeBytes || scan.worldsBytes < scan.freeBytes - 2 * 1024 ** 3;
  useEffect(() => {
    if (scan && !worldsFit) setWorlds(false);
  }, [scan, worldsFit]);

  const run = useMutation({
    mutationFn: () =>
      api.legacyImport({ premium, nickname, afk, gameData, worlds: worlds && gameData }),
    onSuccess: (result) => {
      setReport(result);
      void queryClient.invalidateQueries();
    },
    onError: (error) => toast.error(error instanceof Error ? error.message : String(error)),
  });

  const close = () => {
    onOpenChange(false);
    onFinished?.(report !== null);
    setReport(null);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => (next ? onOpenChange(true) : close())}
      title={report ? t("migrate.doneTitle") : t("migrate.title")}
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
          {[
            report.premiumImported ? t("migrate.donePremium") : null,
            report.nickname ? t("migrate.doneNickname", { name: report.nickname }) : null,
            report.afkAccounts > 0 ? t("migrate.doneAfk", { count: report.afkAccounts }) : null,
            report.copiedFiles + report.linkedFiles > 0
              ? t("migrate.doneFiles", { count: report.copiedFiles + report.linkedFiles })
              : null,
            ...report.notes,
          ]
            .filter(Boolean)
            .map((line) => (
              <li key={line} className="flex gap-2.5">
                <CheckCircle size={18} weight="fill" className="mt-px shrink-0 text-jade-400" />
                {line}
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
        <div className="space-y-0.5 pt-1">
          {scan.hasPremium ? (
            <Check checked={premium} onChange={setPremium} label={t("migrate.premium")} />
          ) : null}
          {scan.nickname ? (
            <Check
              checked={nickname}
              onChange={setNickname}
              label={t("migrate.nickname", { name: scan.nickname })}
            />
          ) : null}
          {scan.afkAccounts > 0 || scan.hasAfkClient ? (
            <Check
              checked={afk}
              onChange={setAfk}
              label={t("migrate.afk", { count: scan.afkAccounts })}
            />
          ) : null}
          <Check checked={gameData} onChange={setGameData} label={t("migrate.gameData")} />
          {scan.worlds > 0 ? (
            <Check
              checked={worlds && gameData}
              onChange={setWorlds}
              disabled={!gameData || !worldsFit}
              label={t("migrate.worlds", {
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
