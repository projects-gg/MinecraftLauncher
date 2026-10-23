import {
  ArrowsLeftRight,
  CheckCircle,
  FolderOpen,
  Lifebuoy,
  WarningCircle,
} from "@phosphor-icons/react";
import { Button, ContentIcon, Dialog, Progress, Skeleton, SwapLabel } from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { motion } from "motion/react";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useImportJob } from "@/features/import/import-job";
import { glide, useMotionPrefs } from "@/features/modcenter/motion";
import { api, type MovePlan, type MoveProgress } from "@/lib/api";
import { formatBytes, formatCount, formatPercent } from "@/lib/format";
import { useUi } from "@/stores/ui";
import { CleanupCard } from "./cleanup/cleanup-card";
import { TrashRow } from "./cleanup/trash-row";
import { ErrorLine, type Failure, failure } from "./failure";

/**
 * One setting, laid out like the settings `Row` (title and help on the left, actions on the
 * right), with room under it for a path, a result or an error.
 */
function Item({
  title,
  help,
  actions,
  children,
}: {
  title: string;
  help?: string;
  actions: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="border-b border-white/6 py-4 last:border-b-0">
      <div className="flex items-center justify-between gap-8">
        <div className="min-w-0">
          <p className="text-[14px] font-medium text-bone-50">{title}</p>
          {help ? <p className="mt-0.5 max-w-[56ch] text-[12.5px] text-ash-400">{help}</p> : null}
        </div>
        <div className="flex shrink-0 gap-2">{actions}</div>
      </div>
      {children}
    </div>
  );
}

/** Settings → Veri: the data folder, cleanup, the launcher trash, support bundle and v1 import. */
export function DataSection() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const motionPrefs = useMotionPrefs();
  // Fresh numbers every time the section opens (logs grow while playing).
  const overview = useQuery({
    queryKey: ["data-overview"],
    queryFn: () => api.data.overview(),
    staleTime: 0,
  });
  const [picked, setPicked] = useState<{ path: string; key: number } | null>(null);
  const refresh = () => void queryClient.invalidateQueries({ queryKey: ["data-overview"] });

  const openFolder = useMutation({ mutationFn: () => api.data.openFolder() });
  const pick = useMutation({
    mutationFn: () => api.data.pickFolder(),
    onSuccess: (path) => {
      if (path) setPicked({ path, key: Date.now() });
    },
  });
  const reveal = useMutation({ mutationFn: (path: string) => api.data.revealFile(path) });
  const bundle = useMutation({
    mutationFn: () => api.data.createSupportBundle(),
    onSuccess: () => {
      reveal.reset();
      refresh();
    },
  });

  const data = overview.data;
  const loading = overview.isPending;
  const folderError = openFolder.error ?? pick.error;
  // The rows under the cleanup card glide when it grows or shrinks.
  const glides = motionPrefs.layout ? ("position" as const) : false;

  return (
    <>
      <Item
        title={t("dataSettings.folder")}
        help={t("dataSettings.folderHelp")}
        actions={
          <>
            <Button
              variant="ghost"
              size="sm"
              icon={<FolderOpen size={15} />}
              onClick={() => openFolder.mutate()}
            >
              {t("dataSettings.open")}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              icon={<ArrowsLeftRight size={15} />}
              disabled={!data?.movable || pick.isPending}
              onClick={() => pick.mutate()}
            >
              {t("dataSettings.move")}
            </Button>
          </>
        }
      >
        {data ? (
          <p className="mt-3 rounded-md bg-obsidian-950/45 px-3 py-2 text-[12.5px] break-all text-bone-100 ring-1 ring-white/6 select-text">
            {data.root}
          </p>
        ) : loading ? (
          <Skeleton className="mt-3 h-[34px] w-full" />
        ) : null}
        {data && !data.movable ? (
          <p className="mt-2 text-[12.5px] text-ash-400">{t("dataSettings.pinned")}</p>
        ) : null}
        {data?.unreachable ? (
          <div
            role="alert"
            className="mt-3 flex items-start gap-2.5 rounded-[10px] bg-flare-400/8 px-3.5 py-2.5 ring-1 ring-flare-400/25"
          >
            <WarningCircle size={17} weight="fill" className="mt-px shrink-0 text-flare-400" />
            <p className="min-w-0 text-[13px] break-words text-bone-100">
              {t("dataSettings.unreachable", { path: data.unreachable })}
            </p>
          </div>
        ) : null}
        {folderError ? (
          <ErrorLine
            className="mt-3"
            error={failure(folderError, t("dataSettings.folderFailed"))}
          />
        ) : null}
      </Item>

      {overview.isError ? (
        <ErrorLine
          className="mt-3"
          error={failure(overview.error, t("dataSettings.overviewFailed"))}
        />
      ) : null}

      <CleanupCard root={data?.root} />

      <motion.div layout={glides} transition={{ layout: glide }} className="mt-2">
        <TrashRow />

        <Item
          title={t("dataSettings.support")}
          help={t("dataSettings.supportHelp")}
          actions={
            <Button
              variant="secondary"
              size="sm"
              icon={<Lifebuoy size={15} />}
              disabled={bundle.isPending}
              onClick={() => bundle.mutate()}
            >
              {bundle.isPending
                ? t("dataSettings.supportCreating")
                : t("dataSettings.supportCreate")}
            </Button>
          }
        >
          {bundle.data ? (
            <div className="mt-3 flex items-center gap-3 rounded-[10px] bg-jade-400/8 px-3.5 py-2.5 ring-1 ring-jade-400/20">
              <CheckCircle size={19} weight="fill" className="shrink-0 text-jade-400" />
              <p className="min-w-0 flex-1 text-[13px] text-bone-100" aria-live="polite">
                {t("dataSettings.supportSaved", {
                  place: t(`dataSettings.supportPlaces.${bundle.data.folder}`),
                })}
              </p>
              <Button
                variant="secondary"
                size="sm"
                icon={<FolderOpen size={15} />}
                onClick={() => bundle.data && reveal.mutate(bundle.data.path)}
              >
                {t("dataSettings.reveal")}
              </Button>
            </div>
          ) : bundle.error ? (
            <ErrorLine
              className="mt-3"
              error={failure(bundle.error, t("dataSettings.supportFailed"))}
            />
          ) : null}
          {reveal.error ? (
            <ErrorLine
              className="mt-3"
              error={failure(reveal.error, t("dataSettings.folderFailed"))}
            />
          ) : null}
        </Item>

        <Item
          title={t("dataSettings.legacy")}
          help={t("dataSettings.legacyHelp")}
          actions={
            <Button
              variant="secondary"
              size="sm"
              onClick={() => useUi.getState().setMigrationOpen(true)}
            >
              {t("dataSettings.import")}
            </Button>
          }
        />

        <Item
          title={t("import.settings.title")}
          help={t("import.settings.help")}
          actions={
            <Button
              variant="secondary"
              size="sm"
              onClick={() => useImportJob.getState().setDialogOpen(true)}
            >
              {t("import.settings.action")}
            </Button>
          }
        />
      </motion.div>

      <MoveDialog
        picked={picked}
        onPickAgain={() => pick.mutate()}
        onClose={() => setPicked(null)}
      />
    </>
  );
}

type MoveState =
  | { kind: "checking" }
  | { kind: "plan"; plan: MovePlan }
  | { kind: "refused"; error: Failure }
  | { kind: "moving"; plan: MovePlan; progress: MoveProgress | null; stopping: boolean }
  | { kind: "done" }
  | { kind: "cancelled" }
  | { kind: "failed"; error: Failure };

function PlanLine({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[132px_1fr] gap-3 py-1.5">
      <dt className="text-[13px] text-ash-400">{label}</dt>
      <dd className="min-w-0 text-[13.5px] break-all text-bone-100">{children}</dd>
    </div>
  );
}

/** Picked folder → check and estimate → copy with progress → restart in the new place. */
function MoveDialog({
  picked,
  onPickAgain,
  onClose,
}: {
  picked: { path: string; key: number } | null;
  onPickAgain: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [state, setState] = useState<MoveState>({ kind: "checking" });

  useEffect(() => {
    if (!picked) return;
    let current = true;
    setState({ kind: "checking" });
    api.data.planMove(picked.path).then(
      (plan) => {
        if (current) setState({ kind: "plan", plan });
      },
      (error) => {
        if (current) {
          setState({ kind: "refused", error: failure(error, t("dataSettings.moveDialog.failed")) });
        }
      },
    );
    return () => {
      current = false;
    };
  }, [picked, t]);

  const start = async (plan: MovePlan) => {
    if (!picked) return;
    setState({ kind: "moving", plan, progress: null, stopping: false });
    try {
      await api.data.move(picked.path, (progress) =>
        setState((s) => (s.kind === "moving" ? { ...s, progress } : s)),
      );
      setState({ kind: "done" });
    } catch (error) {
      const f = failure(error, t("dataSettings.moveDialog.failed"));
      setState(f.code === "IPTAL" ? { kind: "cancelled" } : { kind: "failed", error: f });
    }
  };
  const stop = () => {
    setState((s) => (s.kind === "moving" ? { ...s, stopping: true } : s));
    void api.data.cancelMove();
  };

  const busy = state.kind === "moving" || state.kind === "done";
  const m = "dataSettings.moveDialog";

  let body: ReactNode;
  let footer: ReactNode;
  switch (state.kind) {
    case "checking":
      body = (
        <div className="space-y-3 pt-1" aria-busy>
          <p className="text-[13px] text-ash-300">{t(`${m}.checking`)}</p>
          <Skeleton className="h-5 w-3/4" />
          <Skeleton className="h-5 w-2/3" />
          <Skeleton className="h-5 w-1/2" />
        </div>
      );
      footer = (
        <Button variant="ghost" onClick={onClose}>
          {t("dataSettings.cancel")}
        </Button>
      );
      break;
    case "plan": {
      const { plan } = state;
      body = (
        <div className="pt-1">
          <dl className="rounded-[10px] bg-obsidian-950/40 px-4 py-2 ring-1 ring-white/6">
            <PlanLine label={t(`${m}.from`)}>{plan.from}</PlanLine>
            <PlanLine label={t(`${m}.to`)}>
              <span className="text-gold-200">{plan.target}</span>
            </PlanLine>
            <PlanLine label={t(`${m}.size`)}>
              {t(`${m}.sizeValue`, {
                size: formatBytes(plan.totalBytes),
                count: formatCount(plan.fileCount),
              })}
            </PlanLine>
            <PlanLine label={t(`${m}.free`)}>
              {plan.freeBytes === undefined
                ? t("dataSettings.unknown")
                : formatBytes(plan.freeBytes)}
            </PlanLine>
          </dl>
          {plan.enoughSpace ? (
            <p className="mt-4 text-[13px] text-ash-300">{t(`${m}.oldFolder`)}</p>
          ) : (
            <div className="mt-4 flex items-start gap-2.5" role="alert">
              <WarningCircle size={17} weight="fill" className="mt-px shrink-0 text-flare-400" />
              <p className="text-[13px] text-flare-400">{t(`${m}.noSpace`)}</p>
            </div>
          )}
        </div>
      );
      footer = (
        <>
          <Button variant="ghost" className="mr-auto" onClick={onPickAgain}>
            {t(`${m}.pickOther`)}
          </Button>
          <Button variant="ghost" onClick={onClose}>
            {t("dataSettings.cancel")}
          </Button>
          <Button
            variant="primary"
            icon={<ArrowsLeftRight size={16} />}
            disabled={!plan.enoughSpace}
            onClick={() => void start(plan)}
          >
            {t(`${m}.start`)}
          </Button>
        </>
      );
      break;
    }
    case "refused":
      body = <ErrorLine className="pt-1" error={state.error} />;
      footer = (
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.close")}
          </Button>
          <Button variant="secondary" onClick={onPickAgain}>
            {t(`${m}.pickOther`)}
          </Button>
        </>
      );
      break;
    case "moving": {
      const p = state.progress;
      const stage = p?.stage ?? "scanning";
      const percent =
        p && stage !== "scanning" && p.totalBytes > 0
          ? Math.min(100, (p.doneBytes / p.totalBytes) * 100)
          : undefined;
      body = (
        <div className="pt-1" aria-live="polite">
          <div className="flex items-baseline justify-between gap-4">
            <p className="text-[14px] font-medium text-bone-50">{t(`${m}.stages.${stage}`)}</p>
            {percent !== undefined ? (
              <p className="font-display text-[20px] font-semibold text-gold-200 tabular-nums">
                {formatPercent(Math.floor(percent))}
              </p>
            ) : null}
          </div>
          <Progress className="mt-3" value={percent} label={t(`${m}.progressLabel`)} />
          <div className="mt-2.5 flex justify-between gap-4 text-[12.5px] text-ash-400 tabular-nums">
            <span>
              {p && stage !== "scanning"
                ? t(`${m}.bytes`, {
                    done: formatBytes(p.doneBytes),
                    total: formatBytes(p.totalBytes),
                  })
                : " "}
            </span>
            <span>
              {p && stage !== "scanning"
                ? t(`${m}.files`, {
                    done: formatCount(p.doneFiles),
                    total: formatCount(p.totalFiles),
                  })
                : null}
            </span>
          </div>
          <p className="mt-5 text-[13px] text-ash-300">{t(`${m}.keepOpen`)}</p>
        </div>
      );
      footer = (
        <Button variant="secondary" disabled={state.stopping} onClick={stop}>
          <SwapLabel
            labels={[t(`${m}.stop`), t(`${m}.stopping`)]}
            active={state.stopping ? 1 : 0}
          />
        </Button>
      );
      break;
    }
    case "done":
      body = (
        <div className="flex flex-col items-center gap-3 py-4 text-center" aria-live="polite">
          <ContentIcon kind="done" size={48} />
          <div>
            <p className="font-display text-[18px] font-semibold text-bone-50">
              {t(`${m}.doneTitle`)}
            </p>
            <p className="mt-1 text-[13px] text-ash-300">{t(`${m}.restarting`)}</p>
          </div>
          <Progress className="mt-1 w-56" label={t(`${m}.restarting`)} />
        </div>
      );
      footer = null;
      break;
    case "cancelled":
      body = <p className="pt-1 text-[13.5px] text-bone-100">{t(`${m}.cancelled`)}</p>;
      footer = (
        <Button variant="primary" onClick={onClose}>
          {t("common.close")}
        </Button>
      );
      break;
    case "failed":
      body = <ErrorLine className="pt-1" error={state.error} />;
      footer = (
        <Button variant="primary" onClick={onClose}>
          {t("common.close")}
        </Button>
      );
      break;
  }

  return (
    <Dialog
      open={picked !== null}
      onOpenChange={(next) => {
        // A running copy is stopped with its own button; closing would hide it.
        if (!next && !busy) onClose();
      }}
      title={t(`${m}.title`)}
      dismissOnOutsideClick={false}
      description={
        state.kind === "checking" || state.kind === "plan" || state.kind === "moving"
          ? t(`${m}.description`)
          : undefined
      }
      closeLabel={t("common.close")}
      width={560}
      footer={footer}
      // While copying (and restarting) the only way out is "İptal et": no close button.
      className={busy ? "[&>header>button]:hidden" : undefined}
    >
      {body}
    </Dialog>
  );
}
