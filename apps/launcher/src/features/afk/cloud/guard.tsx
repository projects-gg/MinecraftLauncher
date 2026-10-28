// The same account in two places: a desktop "Bağla" while it runs in the cloud, or a cloud "Başlat"
// while it runs on this computer. Two clients with one name on one server keep kicking each other,
// so the launcher asks first and stops the other one on the player's word. "Bu bilgisayarda
// çalıştır" on the card of a 24/7 account running in the cloud asks the same way, in the move's own
// words (what it leaves behind; AFK-CLEAR).
import { Button, Dialog, toast } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { afkFailed } from "../errors";
import { cloudApi } from "./api";
import { storeAccount, useCloudAction, useCloudUi } from "./use-cloud";

export function CloudGuardDialog() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const guard = useCloudUi((s) => s.guard);
  const setGuard = useCloudUi((s) => s.setGuard);
  const start = useCloudAction("start");
  const [busy, setBusy] = useState(false);
  const close = () => setGuard(null);

  const proceed = async () => {
    if (!guard) return;
    setBusy(true);
    try {
      if (guard.where === "cloud" || guard.where === "move") {
        // The cloud one stops first; then this computer connects, as "Bağla" asked.
        const stopped = await cloudApi.stop(guard.cloud.id);
        storeAccount(queryClient, stopped);
        await api.afkConnect(guard.desktop.id);
        if (guard.where === "move") {
          toast.success(t("cloudAfk.location.toHere", { name: guard.desktop.nickname }));
        }
      } else {
        await api.afkStop(guard.desktop.id);
        await start(guard.cloud);
      }
      close();
    } catch (error) {
      afkFailed(error, t("cloudAfk.actions.failed"));
    } finally {
      setBusy(false);
    }
  };

  const where = guard?.where ?? "cloud";
  const name = guard?.desktop.nickname ?? "";
  return (
    <Dialog
      open={Boolean(guard)}
      onOpenChange={(open) => !open && close()}
      title={
        where === "move"
          ? t("afkClear.twin.moveTitle", { name })
          : where === "cloud"
            ? t("cloudAfk.guard.cloudTitle")
            : t("cloudAfk.guard.hereTitle")
      }
      description={where === "move" ? t("afkClear.twin.moveBody") : t("cloudAfk.guard.body")}
      closeLabel={t("common.close")}
      width={440}
      footer={
        <>
          <Button variant="ghost" onClick={close} disabled={busy}>
            {t("cloudAfk.guard.cancel")}
          </Button>
          <Button variant="primary" onClick={() => void proceed()} disabled={busy}>
            {where === "move"
              ? t("afkClear.twin.runHere")
              : where === "cloud"
                ? t("cloudAfk.guard.stopCloud")
                : t("cloudAfk.guard.stopHere")}
          </Button>
        </>
      }
    />
  );
}
