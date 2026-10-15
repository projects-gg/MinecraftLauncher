import { FileArrowUp, Sparkle } from "@phosphor-icons/react";
import { Button, cn, Dialog, Field, Input, Segmented, Select, toast } from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type InstanceIcon as IconKey, type LoaderKind } from "@/lib/api";
import { InstanceIcon, instanceIcons } from "@/shell/instance-icon";

type Source = "new" | "import";
type VersionFilter = "release" | "snapshot" | "old";

const loaderOptions: LoaderKind[] = ["vanilla", "fabric", "quilt", "forge", "neoForge"];

export function CreateInstanceDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [source, setSource] = useState<Source>("new");
  const [name, setName] = useState("");
  const [nameError, setNameError] = useState<string | null>(null);
  const [icon, setIcon] = useState<IconKey>("grass");
  const [filter, setFilter] = useState<VersionFilter>("release");
  const [mcVersion, setMcVersion] = useState<string>("");
  const [loader, setLoader] = useState<LoaderKind>("fabric");
  const [loaderVersion, setLoaderVersion] = useState<string>("");

  const versionsQuery = useQuery({ queryKey: ["mc-versions"], queryFn: () => api.minecraftVersions(), enabled: open });
  const filtered = useMemo(
    () =>
      (versionsQuery.data ?? []).filter((v) =>
        filter === "release" ? v.kind === "release" : filter === "snapshot" ? v.kind === "snapshot" : v.kind === "oldBeta" || v.kind === "oldAlpha",
      ),
    [versionsQuery.data, filter],
  );

  useEffect(() => {
    if (filtered.length && !filtered.some((v) => v.id === mcVersion)) setMcVersion(filtered[0]?.id ?? "");
  }, [filtered, mcVersion]);

  const loaderQuery = useQuery({
    queryKey: ["loader-versions", loader, mcVersion],
    queryFn: () => api.loaderVersions(loader, mcVersion),
    enabled: open && loader !== "vanilla" && Boolean(mcVersion),
  });

  useEffect(() => {
    const first = loaderQuery.data?.find((v) => v.stable) ?? loaderQuery.data?.[0];
    setLoaderVersion(first?.version ?? "");
  }, [loaderQuery.data]);

  useEffect(() => {
    if (!open) {
      setName("");
      setNameError(null);
      setSource("new");
    }
  }, [open]);

  const create = useMutation({
    mutationFn: () =>
      api.createInstance({
        name: name.trim(),
        icon,
        mcVersion,
        loader,
        loaderVersion: loader === "vanilla" ? undefined : loaderVersion,
      }),
    onSuccess: (created) => {
      void queryClient.invalidateQueries({ queryKey: ["instances"] });
      toast.success(t("library.created", { name: created.name }));
      onOpenChange(false);
      void navigate({ to: "/library/$instanceId", params: { instanceId: created.id } });
    },
  });

  const submit = () => {
    if (!name.trim()) {
      setNameError(t("create.nameRequired"));
      return;
    }
    create.mutate();
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("create.title")}
      description={t("create.description")}
      closeLabel={t("common.close")}
      width={620}
      footer={
        source === "new" ? (
          <>
            <Button variant="ghost" onClick={() => onOpenChange(false)}>
              {t("create.cancel")}
            </Button>
            <Button variant="primary" icon={<Sparkle size={16} weight="fill" />} onClick={submit} disabled={create.isPending || !mcVersion}>
              {t("create.submit")}
            </Button>
          </>
        ) : undefined
      }
    >
      <Segmented
        value={source}
        onValueChange={setSource}
        label={t("create.title")}
        options={[
          { value: "new", label: t("create.sourceNew") },
          { value: "import", label: t("create.sourceImport") },
        ]}
        className="mb-5"
      />

      {source === "new" ? (
        <div className="grid gap-5">
          <div className="grid grid-cols-[1fr_auto] items-start gap-4">
            <Field label={t("create.name")} htmlFor="instance-name" error={nameError}>
              <Input
                id="instance-name"
                value={name}
                invalid={Boolean(nameError)}
                onChange={(e) => {
                  setName(e.target.value);
                  setNameError(null);
                }}
                maxLength={48}
                autoFocus
              />
            </Field>
            <InstanceIcon icon={icon} size={64} className="mt-6" />
          </div>

          <Field label={t("create.icon")}>
            <div role="radiogroup" aria-label={t("create.icon")} className="flex flex-wrap gap-2">
              {(Object.keys(instanceIcons) as IconKey[]).map((key) => (
                <button
                  key={key}
                  type="button"
                  role="radio"
                  aria-checked={icon === key}
                  aria-label={key}
                  onClick={() => setIcon(key)}
                  className={cn(
                    "rounded-md p-1 transition-[background-color,box-shadow] hover:bg-white/8 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
                    icon === key && "bg-white/8 ring-2 ring-gold-400",
                  )}
                >
                  <InstanceIcon icon={key} size={40} />
                </button>
              ))}
            </div>
          </Field>

          <div className="grid grid-cols-2 gap-4">
            <Field label={t("create.version")}>
              <div className="flex flex-col gap-2">
                <Segmented
                  value={filter}
                  onValueChange={setFilter}
                  label={t("create.version")}
                  options={[
                    { value: "release", label: t("create.filterRelease") },
                    { value: "snapshot", label: t("create.filterSnapshot") },
                    { value: "old", label: t("create.filterOld") },
                  ]}
                />
                <Select
                  value={mcVersion}
                  onValueChange={setMcVersion}
                  label={t("create.version")}
                  options={filtered.map((v) => ({ value: v.id, label: v.id }))}
                  disabled={!filtered.length}
                />
              </div>
            </Field>
            <Field
              label={t("create.loader")}
              help={loader === "vanilla" ? t("create.loaderHelpVanilla") : t("create.loaderHelp")}
            >
              <div className="flex flex-col gap-2">
                <Select
                  value={loader}
                  onValueChange={setLoader}
                  label={t("create.loader")}
                  options={loaderOptions.map((l) => ({ value: l, label: t(`loaders.${l}`) }))}
                />
                {loader !== "vanilla" ? (
                  <Select
                    value={loaderVersion}
                    onValueChange={setLoaderVersion}
                    label={t("create.loaderVersion")}
                    disabled={!loaderQuery.data?.length}
                    options={(loaderQuery.data ?? []).map((v) => ({
                      value: v.version,
                      label: v.stable ? v.version : `${v.version} (${t("create.unstable")})`,
                    }))}
                  />
                ) : null}
              </div>
            </Field>
          </div>
        </div>
      ) : (
        <div className="flex flex-col items-center justify-center rounded-[var(--radius-panel)] border-2 border-dashed border-white/12 px-6 py-12 text-center">
          <FileArrowUp size={40} weight="duotone" className="text-gold-400" />
          <p className="mt-3 font-display text-[16px] font-semibold text-bone-50">{t("create.drop")}</p>
          <p className="mt-1 text-[13px] text-ash-400">{t("create.dropHint")}</p>
          <Button className="mt-5" variant="secondary">
            {t("create.chooseFile")}
          </Button>
        </div>
      )}
    </Dialog>
  );
}
