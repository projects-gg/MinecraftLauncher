import { Plus } from "@phosphor-icons/react";
import {
  Button,
  cn,
  Dialog,
  Field,
  Input,
  Segmented,
  Select,
  SwapLabel,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { showError } from "@/features/content/jobs";
import { ManualModsDialog } from "@/features/content/manual-mods-dialog";
import { useModpackBrowser } from "@/features/content/modpack-browser";
import { useModpackImport } from "@/features/content/modpack-import";
import { useDrop } from "@/features/drop/store";
import { useSetSource } from "@/features/modsets/set-source";
import {
  api,
  type InstanceIcon as IconKey,
  type LoaderKind,
  type ModpackImportResult,
} from "@/lib/api";
import { InstanceIcon, instanceIcons } from "@/shell/instance-icon";
import { usePrefs } from "@/stores/prefs";
import { defaultInstanceName } from "./default-name";

type Source = "new" | "sets" | "browse" | "import";
type VersionFilter = "release" | "snapshot" | "old";

const loaderOptions: LoaderKind[] = ["vanilla", "fabric", "quilt", "forge", "neoForge"];

export function CreateInstanceDialog({
  open,
  onOpenChange,
  initialSource,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Where the dialog opens ("İçe aktar" opens the file import); "Sıfırdan" otherwise. */
  initialSource?: Exclude<Source, "new">;
}) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const motionPrefs = useMotionPrefs();
  const [source, setSource] = useState<Source>(initialSource ?? "new");
  // Set while rendering, not in an effect: the panel opens at the right size, no first frame of
  // "Sıfırdan" that the morph would then glide away from.
  const [shownFor, setShownFor] = useState(open);
  if (open !== shownFor) {
    setShownFor(open);
    if (open && initialSource) setSource(initialSource);
  }
  const [name, setName] = useState("");
  const [icon, setIcon] = useState<IconKey>("grass");
  const [filter, setFilter] = useState<VersionFilter>("release");
  const showSnapshots = usePrefs((s) => s.showSnapshots);
  useEffect(() => {
    if (!showSnapshots && filter === "snapshot") setFilter("release");
  }, [showSnapshots, filter]);
  // The version the player picked. Until then (or while the filter hides it) the recommended
  // version is selected, else the newest one in the list.
  const [pickedVersion, setPickedVersion] = useState<string>("");
  const [loader, setLoader] = useState<LoaderKind>("fabric");
  const [loaderVersion, setLoaderVersion] = useState<string>("");

  const versionsQuery = useQuery({
    queryKey: ["mc-versions"],
    queryFn: () => api.minecraftVersions(),
    enabled: open,
  });
  const filtered = useMemo(
    () =>
      (versionsQuery.data ?? []).filter((v) =>
        filter === "release"
          ? // The recommended version stays listed even when it is a snapshot.
            v.kind === "release" || v.recommended
          : filter === "snapshot"
            ? v.kind === "snapshot"
            : v.kind === "oldBeta" || v.kind === "oldAlpha",
      ),
    [versionsQuery.data, filter],
  );
  const mcVersion = filtered.some((v) => v.id === pickedVersion)
    ? pickedVersion
    : ((filtered.find((v) => v.recommended) ?? filtered[0])?.id ?? "");

  const loaderQuery = useQuery({
    queryKey: ["loader-versions", loader, mcVersion],
    queryFn: () => api.loaderVersions(loader, mcVersion),
    enabled: open && loader !== "vanilla" && Boolean(mcVersion),
  });

  useEffect(() => {
    const first = loaderQuery.data?.find((v) => v.stable) ?? loaderQuery.data?.[0];
    setLoaderVersion(first?.version ?? "");
  }, [loaderQuery.data]);

  // An empty name is fine: the instance is named after its loader and version ("Fabric
  // 1.21.4"), numbered when that name is taken. The field shows it as its placeholder.
  const instancesQuery = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
    enabled: open,
  });
  const defaultName = defaultInstanceName(
    `${loader === "vanilla" ? "Minecraft" : t(`loaders.${loader}`)} ${mcVersion}`,
    (instancesQuery.data ?? []).map((i) => i.name),
  );

  useEffect(() => {
    if (!open) {
      setName("");
      setSource("new");
      setPickedVersion("");
      setFilter("release");
    }
  }, [open]);
  // A modpack dropped onto the window opens the import step (it takes the file from there); the
  // file is forgotten once the dialog closes.
  const pendingPack = useDrop((s) => s.pendingPack);
  useEffect(() => {
    if (open && pendingPack) setSource("import");
  }, [open, pendingPack]);
  const wasOpen = useRef(open);
  useEffect(() => {
    if (wasOpen.current && !open) useDrop.getState().setPendingPack(null);
    wasOpen.current = open;
  }, [open]);

  const [manual, setManual] = useState<ModpackImportResult | null>(null);
  const finishPack = (
    result: ModpackImportResult,
    key: "modpack.import.done" | "modpack.browse.done",
  ) => {
    void queryClient.invalidateQueries({ queryKey: ["instances"] });
    toast.success(t(key, { name: result.instance.name }));
    if (result.skipped > 0) toast(t("modpack.import.skipped", { count: result.skipped }));
    onOpenChange(false);
    // The new instance is the one on screen: OYNA plays it, not the previous choice.
    usePrefs.getState().selectInstance(result.instance.id);
    void navigate({ to: "/library/$instanceId", params: { instanceId: result.instance.id } });
    if (result.manualCount > 0) setManual(result);
  };
  const importFlow = useModpackImport({
    active: open && source === "import",
    onCancel: () => onOpenChange(false),
    onDone: (result) => finishPack(result, "modpack.import.done"),
  });
  const browseFlow = useModpackBrowser({
    active: open && source === "browse",
    onCancel: () => onOpenChange(false),
    onDone: (result) => finishPack(result, "modpack.browse.done"),
  });
  const setFlow = useSetSource({
    active: open && source === "sets",
    onCancel: () => onOpenChange(false),
    instances: (instancesQuery.data ?? []).map((i) => i.name),
    onDone: (created) => {
      void queryClient.invalidateQueries({ queryKey: ["instances"] });
      toast.success(t("modSets.create.done", { name: created.name }));
      onOpenChange(false);
      // The new instance is the one on screen: OYNA plays it, not the previous choice.
      usePrefs.getState().selectInstance(created.id);
      void navigate({ to: "/library/$instanceId", params: { instanceId: created.id } });
    },
  });
  const close = (next: boolean) => {
    if (!next) {
      // Closing the dialog stops a running import or install.
      importFlow.cancelJob();
      browseFlow.cancelJob();
      setFlow.cancelJob();
    }
    onOpenChange(next);
  };

  const create = useMutation({
    mutationFn: () =>
      api.createInstance({
        name: name.trim() || defaultName,
        icon,
        mcVersion,
        loader,
        loaderVersion: loader === "vanilla" ? undefined : loaderVersion,
      }),
    onSuccess: (created) => {
      void queryClient.invalidateQueries({ queryKey: ["instances"] });
      toast.success(t("library.created", { name: created.name }));
      onOpenChange(false);
      // The new instance is the one on screen: OYNA plays it, not the previous choice.
      usePrefs.getState().selectInstance(created.id);
      void navigate({ to: "/library/$instanceId", params: { instanceId: created.id } });
    },
    onError: (error) => showError(error),
  });
  const canCreate = !create.isPending && Boolean(mcVersion);
  const versionSearch = {
    searchLabel: t("modCenter.versions.search"),
    emptyText: t("create.noMatch"),
  };

  return (
    <>
      <Dialog
        open={open}
        onOpenChange={close}
        title={t("create.title")}
        closeLabel={t("common.close")}
        width={680}
        // The three sources and the modpack steps differ a lot in height: the panel glides.
        morph={motionPrefs.layout}
        // A form (and a modpack search) made as a whole: a stray click beside it keeps it.
        dismissOnOutsideClick={false}
        footer={
          source === "sets" ? (
            setFlow.footer
          ) : source === "browse" ? (
            browseFlow.footer
          ) : source === "import" ? (
            importFlow.footer
          ) : source === "new" ? (
            <>
              <Button variant="ghost" onClick={() => onOpenChange(false)}>
                {t("create.cancel")}
              </Button>
              <Button
                variant="primary"
                icon={<Plus size={16} weight="bold" />}
                onClick={() => create.mutate()}
                disabled={!canCreate}
              >
                <SwapLabel
                  labels={[t("create.submit"), t("create.creating")]}
                  active={create.isPending ? 1 : 0}
                />
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
            { value: "sets", label: t("modSets.create.source") },
            { value: "browse", label: t("modpack.sourceBrowse") },
            { value: "import", label: t("create.sourceImport") },
          ]}
          className="mb-5"
        />

        {source === "new" ? (
          <div className="grid gap-5">
            <div className="grid grid-cols-[1fr_auto] items-start gap-4">
              <Field label={t("create.name")} htmlFor="instance-name">
                <Input
                  id="instance-name"
                  value={name}
                  // The default name is what the instance really gets: legible, not a hint.
                  placeholder={defaultName}
                  className="placeholder:text-ash-400"
                  onChange={(e) => setName(e.target.value)}
                  // Enter creates it, like the button (the defaults are a complete choice).
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.nativeEvent.isComposing && canCreate) {
                      e.preventDefault();
                      create.mutate();
                    }
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
                  <label
                    key={key}
                    className={cn(
                      "cursor-pointer rounded-md p-1 transition-[background-color,box-shadow] hover:bg-white/8 has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-gold-300",
                      icon === key && "bg-white/8 ring-2 ring-gold-400",
                    )}
                  >
                    <input
                      type="radio"
                      name="instance-icon"
                      value={key}
                      checked={icon === key}
                      onChange={() => setIcon(key)}
                      className="sr-only"
                    />
                    <InstanceIcon icon={key} size={40} />
                    <span className="sr-only">{t(`create.iconNames.${key}`)}</span>
                  </label>
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
                      { value: "release" as VersionFilter, label: t("create.filterRelease") },
                      ...(showSnapshots
                        ? [
                            {
                              value: "snapshot" as VersionFilter,
                              label: t("create.filterSnapshot"),
                            },
                          ]
                        : []),
                      { value: "old" as VersionFilter, label: t("create.filterOld") },
                    ]}
                  />
                  {/* A hundred releases (hundreds of snapshots): the searchable list. */}
                  <Select
                    value={mcVersion}
                    onValueChange={setPickedVersion}
                    label={t("create.version")}
                    options={filtered.map((v) => ({
                      value: v.id,
                      label: v.id,
                      badge: v.recommended ? t("create.recommended") : undefined,
                    }))}
                    disabled={!filtered.length}
                    searchable={versionSearch}
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
                      searchable={versionSearch}
                    />
                  ) : null}
                </div>
              </Field>
            </div>
          </div>
        ) : source === "sets" ? (
          setFlow.body
        ) : source === "browse" ? (
          browseFlow.body
        ) : (
          importFlow.body
        )}
      </Dialog>
      <ManualModsDialog result={manual} onClose={() => setManual(null)} />
    </>
  );
}
