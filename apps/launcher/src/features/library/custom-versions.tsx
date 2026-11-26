// CUSTOM-VER: "Özel" in "Yeni Kurulum" (features.customVersions): the versions other installers
// (OptiFine's, Forge's, a modpack's own) put in the data folder. The list stands where the
// Minecraft versions are; the column beside it, where the loader is picked otherwise, says how a
// version gets there (installers ask for the data folder).
import { Copy, FolderOpen, Package } from "@phosphor-icons/react";
import { Button, Field, Select, type SelectOption, Skeleton, toast } from "@pl/ui";
import { type UseQueryResult, useMutation, useQuery } from "@tanstack/react-query";
import { Fragment } from "react";
import { useTranslation } from "react-i18next";
import { showError } from "@/features/content/jobs";
import { api, type CustomVersions } from "@/lib/api";
import { copyText } from "@/lib/clipboard";

/** The signed config's `features.customVersions`, from the home content (the query Home reads). */
export function useCustomVersionsOn(): boolean {
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  return Boolean(data?.customVersions);
}

/** The list, read again when the window comes back (an installer may have just run). */
export function useCustomVersions(enabled: boolean): UseQueryResult<CustomVersions> {
  return useQuery({
    queryKey: ["custom-versions"],
    queryFn: () => api.customVersions(),
    enabled,
    refetchOnWindowFocus: true,
  });
}

/** A still line in the list's place, as tall as the list's box: not a field, nothing to press. */
function QuietLine({ text }: { text: string }) {
  return (
    <p className="flex h-[34px] items-center gap-2 text-[13px] text-ash-400">
      <Package size={16} className="shrink-0" aria-hidden />
      {text}
    </p>
  );
}

/** The custom versions where the Minecraft versions are listed otherwise, or why there are none. */
export function CustomVersionList({
  query,
  options,
  value,
  onChange,
  label,
  searchable,
}: {
  query: UseQueryResult<CustomVersions>;
  options: SelectOption<string>[];
  value: string;
  onChange: (id: string) => void;
  label: string;
  searchable: { searchLabel: string; emptyText: string };
}) {
  const { t } = useTranslation();
  if (query.isPending) return <Skeleton className="h-[34px] w-full" />;
  if (query.isError) return <QuietLine text={t("customVer.failed")} />;
  if (options.length === 0) return <QuietLine text={t("customVer.empty")} />;
  return (
    <Select
      value={value}
      onValueChange={onChange}
      label={label}
      options={options}
      searchable={searchable}
    />
  );
}

/** A path that wraps after its separators, never inside a folder's name. */
function PathText({ path }: { path: string }) {
  const parts = path.split(/(?<=[\\/])/);
  return (
    <>
      {parts.map((part, index) => (
        // The parts of one path never move: their place is their key.
        // biome-ignore lint/suspicious/noArrayIndexKey: a fixed split of one string
        <Fragment key={index}>
          {part}
          {index < parts.length - 1 ? <wbr /> : null}
        </Fragment>
      ))}
    </>
  );
}

/** Beside the list: the folder an installer is pointed at, to copy or open. */
export function CustomVersionHelp({ folder }: { folder?: string }) {
  const { t } = useTranslation();
  const open = useMutation({
    mutationFn: () => api.openCustomVersionsFolder(),
    onError: (error) => showError(error),
  });
  const copy = () => {
    if (!folder) return;
    copyText(folder).then(
      () => toast.success(t("customVer.add.copied")),
      () => toast.error(t("common.copyFailed")),
    );
  };
  return (
    <Field label={t("customVer.add.title")}>
      <p className="text-[12.5px] leading-snug text-pretty text-ash-300">
        {t("customVer.add.body")}
      </p>
      {folder ? (
        <p className="text-[12.5px] leading-snug [overflow-wrap:anywhere] text-ash-300 select-text">
          <PathText path={folder} />
        </p>
      ) : (
        <Skeleton className="h-[18px] w-full" />
      )}
      {/* Ghost buttons sit on the column's edge: their icons line up with the text above. */}
      <div className="-ml-3 flex flex-wrap gap-1">
        <Button
          variant="ghost"
          size="sm"
          icon={<Copy size={15} />}
          disabled={!folder}
          onClick={copy}
        >
          {t("customVer.add.copy")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          icon={<FolderOpen size={15} />}
          onClick={() => open.mutate()}
        >
          {t("customVer.add.open")}
        </Button>
      </div>
    </Field>
  );
}
