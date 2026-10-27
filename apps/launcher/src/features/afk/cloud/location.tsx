// Where a desktop AFK account can go once the player has 7/24 Bulut accounts. The "Bilgisayarda /
// Bulutta" switch on the cards is gone (AFK-CLEAR: it read like a view filter): an account in both
// places is one card in the cloud list, whose "Bu bilgisayarda çalıştır" / "Bulutta çalıştır" says
// what happens; an account only on this computer moves with "Buluta taşı" in its menu.
import { CloudArrowUp } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import type { AfkAccount } from "@/lib/api";
import { runsInCloud, twinOf, useCloudPage, useCloudUi } from "./use-cloud";

/** The player has cloud accounts and 7/24 Bulut answers: the page shows two lists. */
export function useCloudPlaces(): boolean {
  const { data } = useCloudPage();
  const overview = data?.overview;
  return Boolean(
    overview?.available && overview.session === "signedIn" && overview.accounts.length > 0,
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
