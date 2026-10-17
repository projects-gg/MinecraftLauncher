import { ArrowSquareOut, FolderOpen } from "@phosphor-icons/react";
import { Button, Dialog } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { api, type ModpackImportResult } from "@/lib/api";
import { openWebLink } from "@/lib/links";

/** After a CurseForge import: the mods the player has to download by hand. */
export function ManualModsDialog({
  result,
  onClose,
}: {
  result: ModpackImportResult | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const links = result?.manualMods ?? [];
  return (
    <Dialog
      open={Boolean(result)}
      onOpenChange={(open) => !open && onClose()}
      title={t("modpack.import.manualTitle")}
      description={t("modpack.import.manualBody")}
      closeLabel={t("common.close")}
      width={560}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("modpack.import.later")}
          </Button>
          <Button
            variant="secondary"
            icon={<FolderOpen size={16} />}
            onClick={() => result && void api.openInstanceFolder(result.instance.id, "mods")}
          >
            {t("modpack.import.openMods")}
          </Button>
        </>
      }
    >
      <p className="mb-3 text-[13px] text-ash-300">
        {t("modpack.import.manual", { count: result?.manualCount ?? links.length })}
      </p>
      {links.length > 0 ? (
        <ul className="space-y-0.5">
          {links.map((link) => (
            <li key={link.url}>
              <button
                type="button"
                onClick={() => openWebLink(link.url)}
                className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2.5 py-2 text-left text-[13.5px] text-bone-100 transition-colors hover:bg-white/6 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
              >
                <span className="min-w-0 flex-1 truncate">{link.name}</span>
                <ArrowSquareOut size={15} className="shrink-0 text-ash-400" />
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </Dialog>
  );
}
