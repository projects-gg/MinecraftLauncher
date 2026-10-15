import { Copy, FileText, GearSix, Warning } from "@phosphor-icons/react";
import { Button, Chip, Dialog, toast } from "@pl/ui";
import { useNavigate } from "@tanstack/react-router";
import { openPath } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import { isTauri } from "@/lib/platform";
import { useLaunch } from "@/stores/launch";

/** Friendly crash diagnosis after the game closes unexpectedly. */
export function CrashDialog() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const crash = useLaunch((s) => s.crash);
  const instanceId = useLaunch((s) => s.instanceId);
  const dismiss = useLaunch((s) => s.dismissCrash);

  const details = crash
    ? [
        crash.title,
        crash.hint,
        crash.mods.length ? `mods: ${crash.mods.join(", ")}` : "",
        crash.logFile ?? "",
      ]
        .filter(Boolean)
        .join("\n")
    : "";

  return (
    <Dialog
      open={Boolean(crash)}
      onOpenChange={(open) => !open && dismiss()}
      title={
        <span className="flex items-center gap-2.5">
          <Warning size={22} weight="fill" className="text-flare-400" />
          {crash?.title ?? t("crash.title")}
        </span>
      }
      closeLabel={t("common.close")}
      width={560}
      footer={
        <>
          <Button
            variant="ghost"
            icon={<Copy size={16} />}
            onClick={() => {
              void navigator.clipboard
                .writeText(details)
                .then(() => toast.success(t("crash.copied")));
            }}
          >
            {t("crash.copy")}
          </Button>
          {crash?.logFile && isTauri() ? (
            <Button
              variant="secondary"
              icon={<FileText size={16} />}
              onClick={() => crash.logFile && void openPath(crash.logFile)}
            >
              {t("crash.openLog")}
            </Button>
          ) : null}
          {instanceId ? (
            <Button
              variant="primary"
              icon={<GearSix size={16} weight="fill" />}
              onClick={() => {
                dismiss();
                void navigate({
                  to: "/library/$instanceId",
                  params: { instanceId },
                  search: { tab: "settings" },
                });
              }}
            >
              {t("crash.settings")}
            </Button>
          ) : null}
        </>
      }
    >
      {crash ? (
        <div className="space-y-4">
          <p className="text-[14.5px] leading-relaxed text-bone-100">{crash.hint}</p>
          {crash.mods.length ? (
            <div>
              <p className="mb-2 text-[12.5px] text-ash-400">{t("crash.mods")}</p>
              <div className="flex flex-wrap gap-1.5">
                {crash.mods.map((m) => (
                  <Chip key={m} tone="flare">
                    {m}
                  </Chip>
                ))}
              </div>
            </div>
          ) : null}
        </div>
      ) : null}
    </Dialog>
  );
}
