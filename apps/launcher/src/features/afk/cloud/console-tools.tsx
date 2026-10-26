// The smart console's tools (CAFK-FEAT §3.2): the kind chips (one at a time), "Konsolda ara"
// (it takes the chips' place while open) and "İndir" with its three ranges.
import { DownloadSimple, MagnifyingGlass, X } from "@phosphor-icons/react";
import { IconButton, Menu, SearchInput, toast } from "@pl/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ToggleChip } from "@/features/library/parts";
import { afkFailed } from "../errors";
import { type CloudAccount, type CloudLogRange, cloudApi } from "./api";
import { KIND_FILTERS, type KindFilter } from "./console-kinds";

const RANGES: CloudLogRange[] = ["hour", "today", "days3"];

export function ConsoleTools({
  account,
  filter,
  onFilter,
  query,
  onQuery,
}: {
  account: CloudAccount;
  filter: KindFilter;
  onFilter: (filter: KindFilter) => void;
  query: string;
  onQuery: (query: string) => void;
}) {
  const { t } = useTranslation();
  const [searching, setSearching] = useState(false);
  const [downloading, setDownloading] = useState(false);

  const download = async (range: CloudLogRange) => {
    setDownloading(true);
    try {
      const file = await cloudApi.download(account.id, account.name, range);
      if (!file) {
        toast(t("cloudAfkFeat.console.downloadEmpty"));
        return;
      }
      toast.success(t("cloudAfkFeat.console.downloaded"), {
        action: {
          label: t("cloudAfkFeat.console.showFile"),
          onClick: () =>
            void cloudApi
              .showDownload(file.fileName)
              .catch((error: unknown) => afkFailed(error, t("cloudAfkFeat.console.showFailed"))),
        },
      });
    } catch (error) {
      afkFailed(error, t("cloudAfkFeat.console.downloadFailed"));
    } finally {
      setDownloading(false);
    }
  };

  const closeSearch = () => {
    setSearching(false);
    onQuery("");
  };

  return (
    <div className="flex h-11 shrink-0 items-center gap-2 px-5">
      {searching ? (
        <div className="flex min-w-0 flex-1 items-center gap-1.5">
          <SearchInput
            value={query}
            onChange={onQuery}
            placeholder={t("cloudAfkFeat.console.search")}
            label={t("cloudAfkFeat.console.search")}
            autoFocus
            className="min-w-0 flex-1"
          />
          <IconButton
            label={t("cloudAfkFeat.console.searchClose")}
            icon={<X size={16} />}
            size="sm"
            onClick={closeSearch}
          />
        </div>
      ) : (
        <fieldset
          aria-label={t("cloudAfkFeat.console.filters.label")}
          className="-my-1 flex min-w-0 flex-1 items-center gap-1 overflow-x-auto py-1 [scrollbar-width:none]"
        >
          {KIND_FILTERS.map((value) => (
            <ToggleChip
              key={value}
              tone="info"
              pressed={filter === value}
              onPressedChange={() => onFilter(value)}
            >
              {t(`cloudAfkFeat.console.filters.${value}`)}
            </ToggleChip>
          ))}
        </fieldset>
      )}
      <div className="flex shrink-0 items-center gap-0.5">
        {searching ? null : (
          <IconButton
            label={t("cloudAfkFeat.console.search")}
            icon={<MagnifyingGlass size={16} />}
            size="sm"
            onClick={() => setSearching(true)}
          />
        )}
        <Menu
          trigger={
            <IconButton
              label={t("cloudAfkFeat.console.download")}
              icon={<DownloadSimple size={16} />}
              size="sm"
              disabled={downloading}
            />
          }
          items={RANGES.map((range) => ({
            label: t(`cloudAfkFeat.console.ranges.${range}`),
            onSelect: () => void download(range),
          }))}
        />
      </div>
    </div>
  );
}
