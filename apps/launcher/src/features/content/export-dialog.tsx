import { Export } from "@phosphor-icons/react";
import { Button, Dialog, Field, Input, Switch, toast } from "@pl/ui";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { newJobId, type RunningJob, showError } from "./jobs";
import { JobStatus } from "./parts";

/** File name without characters Windows refuses. */
function safeFileName(text: string): string {
  return (
    text
      .trim()
      .replace(/[<>:"/\\|?*]+/g, "")
      .replace(/\s+/g, " ")
      .slice(0, 80) || "modpack"
  );
}

/** Export an instance as a Modrinth `.mrpack` (Modrinth-hosted files are linked, not bundled). */
export function ExportDialog({
  instance,
  onClose,
}: {
  instance: { id: string; name: string } | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [nameError, setNameError] = useState<string | null>(null);
  const [version, setVersion] = useState("1.0.0");
  const [summary, setSummary] = useState("");
  const [includeConfig, setIncludeConfig] = useState(true);
  const [includeOptions, setIncludeOptions] = useState(false);
  const [includeWorlds, setIncludeWorlds] = useState(false);
  const [job, setJob] = useState<RunningJob | null>(null);

  // Fresh form for every instance (not on every parent render).
  const instanceName = instance?.name;
  useEffect(() => {
    if (instanceName === undefined) return;
    setName(instanceName);
    setNameError(null);
    setVersion("1.0.0");
    setSummary("");
    setIncludeConfig(true);
    setIncludeOptions(false);
    setIncludeWorlds(false);
  }, [instanceName]);

  const close = () => {
    if (job) void api.content.cancel(job.jobId);
    onClose();
  };

  const submit = async () => {
    if (!instance) return;
    if (!name.trim()) {
      setNameError(t("modpack.export.nameRequired"));
      return;
    }
    const path = await api.content.pickSavePath(
      `${safeFileName(name)} ${safeFileName(version)}.mrpack`,
    );
    if (!path) return;
    const jobId = newJobId();
    setJob({ jobId });
    try {
      const result = await api.content.exportPack(
        instance.id,
        {
          path,
          name: name.trim(),
          version: version.trim(),
          summary: summary.trim() || undefined,
          includeConfig,
          includeOptions,
          includeWorlds,
        },
        jobId,
        (progress) => setJob({ jobId, progress }),
      );
      const body = t("modpack.export.doneBody", {
        hosted: result.hostedFiles,
        bundled: result.bundledFiles,
      });
      toast.success(t("modpack.export.done"), {
        description: result.offline ? `${body} ${t("modpack.export.offline")}` : body,
      });
      onClose();
    } catch (error) {
      showError(error);
    } finally {
      setJob(null);
    }
  };

  return (
    <Dialog
      open={Boolean(instance)}
      onOpenChange={(open) => !open && close()}
      title={t("modpack.export.title")}
      description={t("modpack.export.description")}
      closeLabel={t("common.close")}
      width={540}
      // A form saved as a whole: a stray click beside it keeps what was chosen.
      dismissOnOutsideClick={false}
      footer={
        job ? (
          <JobStatus
            job={job}
            onCancel={() => void api.content.cancel(job.jobId)}
            className="w-full"
            detailed
          />
        ) : (
          <>
            <Button variant="ghost" onClick={close}>
              {t("create.cancel")}
            </Button>
            <Button
              variant="primary"
              icon={<Export size={16} weight="bold" />}
              onClick={() => void submit()}
            >
              {t("modpack.export.submit")}
            </Button>
          </>
        )
      }
    >
      <div className="grid gap-4">
        <div className="grid grid-cols-[1fr_140px] gap-3">
          <Field label={t("modpack.export.name")} htmlFor="export-name" error={nameError}>
            <Input
              id="export-name"
              value={name}
              maxLength={64}
              invalid={Boolean(nameError)}
              disabled={Boolean(job)}
              onChange={(e) => {
                setName(e.target.value);
                setNameError(null);
              }}
            />
          </Field>
          <Field label={t("modpack.export.version")} htmlFor="export-version">
            <Input
              id="export-version"
              value={version}
              maxLength={24}
              disabled={Boolean(job)}
              onChange={(e) => setVersion(e.target.value)}
            />
          </Field>
        </div>
        <Field
          label={t("modpack.export.summary")}
          htmlFor="export-summary"
          help={t("modpack.export.summaryHelp")}
        >
          <Input
            id="export-summary"
            value={summary}
            maxLength={160}
            disabled={Boolean(job)}
            onChange={(e) => setSummary(e.target.value)}
          />
        </Field>
        <div className="grid gap-3 pt-1">
          <Switch
            checked={includeConfig}
            onCheckedChange={setIncludeConfig}
            label={t("modpack.export.includeConfig")}
            disabled={Boolean(job)}
          />
          <Switch
            checked={includeOptions}
            onCheckedChange={setIncludeOptions}
            label={t("modpack.export.includeOptions")}
            disabled={Boolean(job)}
          />
          <Switch
            checked={includeWorlds}
            onCheckedChange={setIncludeWorlds}
            label={t("modpack.export.includeWorlds")}
            description={t("modpack.export.includeWorldsHelp")}
            disabled={Boolean(job)}
          />
        </div>
      </div>
    </Dialog>
  );
}
