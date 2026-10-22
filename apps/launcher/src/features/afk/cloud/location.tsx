// "Bilgisayarda / Bulutta" on a desktop AFK card, once the player has cloud accounts: where this
// account runs. With a cloud twin (the same name on the same server) a pick stops it on one side
// and starts it on the other; without one, "Bulutta" opens "Buluta taşı" (the plate glides back
// if the player leaves it there).
import { CloudArrowUp } from "@phosphor-icons/react";
import { Segmented, toast } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { type AfkAccount, api } from "@/lib/api";
import { afkFailed } from "../errors";
import { cloudApi } from "./api";
import { runsInCloud, storeAccount, twinOf, useCloudPage, useCloudUi } from "./use-cloud";

type Place = "here" | "cloud";

/** The player has cloud accounts and 7/24 Bulut answers: the cards say where each one runs. */
export function useCloudPlaces(): boolean {
  const { data } = useCloudPage();
  const overview = data?.overview;
  return Boolean(
    overview?.available && overview.session === "signedIn" && overview.accounts.length > 0,
  );
}

export function CloudLocation({ account }: { account: AfkAccount }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data } = useCloudPage();
  const twin = twinOf(data, account.id);
  const [busy, setBusy] = useState(false);
  const value: Place = runsInCloud(twin) ? "cloud" : "here";

  const choose = async (next: Place) => {
    if (next === value || busy) return;
    if (!twin) {
      useCloudUi.getState().openMove(account.id);
      return;
    }
    setBusy(true);
    try {
      if (next === "cloud") {
        if (!["stopped", "error", "clientFailed"].includes(account.state.kind)) {
          await api.afkStop(account.id);
        }
        storeAccount(queryClient, await cloudApi.start(twin.id));
        toast.success(t("cloudAfk.location.toCloud", { name: account.nickname }));
      } else {
        storeAccount(queryClient, await cloudApi.stop(twin.id));
        await api.afkConnect(account.id);
        toast.success(t("cloudAfk.location.toHere", { name: account.nickname }));
      }
    } catch (error) {
      afkFailed(error, t("cloudAfk.actions.failed"));
      void queryClient.invalidateQueries({ queryKey: ["cloud-afk"] });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Segmented<Place>
      value={value}
      onValueChange={(next) => void choose(next)}
      label={t("cloudAfk.location.label")}
      disabled={busy}
      className="mt-3 self-start"
      options={[
        { value: "here", label: t("cloudAfk.location.here") },
        { value: "cloud", label: t("cloudAfk.location.cloud") },
      ]}
    />
  );
}

/** "Buluta taşı" in a desktop card's menus, while 7/24 Bulut answers and the player is signed in. */
export function useMoveMenuItem(account: AfkAccount) {
  const { t } = useTranslation();
  const { data } = useCloudPage();
  const overview = data?.overview;
  if (!overview?.available || overview.session !== "signedIn") return null;
  if (runsInCloud(twinOf(data, account.id))) return null;
  return {
    label: t("cloudAfk.move.action"),
    icon: <CloudArrowUp size={16} />,
    onSelect: () => useCloudUi.getState().openMove(account.id),
  };
}
