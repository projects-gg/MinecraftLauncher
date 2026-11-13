import { Switch } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { Row } from "@/features/settings/row";
import { api } from "@/lib/api";
import { isTauri } from "@/lib/platform";
import { usePrefs } from "@/stores/prefs";

/** The page's choice to the launcher: at its start and on every change (no-op in previews). */
function configure(enabled: boolean) {
  if (!isTauri()) return;
  void commands.iasSyncConfigure(enabled).catch(() => {});
}

/**
 * Keeps "Hesapları Oyun İçi Hesap Değiştiriciye Ekle" in step with the launcher (mounted once,
 * with the command bar): the switch reaches Rust at the start and on every change, so a launch
 * follows it whatever page is open.
 */
export function useIasSyncPush() {
  const enabled = usePrefs((s) => s.iasSync);
  useEffect(() => configure(enabled), [enabled]);
}

/**
 * Settings → Genel "Hesapları Oyun İçi Hesap Değiştiriciye Ekle" (SRV-ACC, on by default): before
 * a game starts, the username accounts join the In-Game Account Switcher's list of an install
 * that has the mod. Shown while the signed config's `features.iasSync` is on.
 */
export function IasSyncRow() {
  const { t } = useTranslation();
  const on = usePrefs((s) => s.iasSync);
  const set = usePrefs((s) => s.set);
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  if (!data?.iasSync) return null;
  return (
    <Row title={t("iasSync.setting")} help={t("iasSync.help")}>
      <Switch
        checked={on}
        onCheckedChange={(value) => set("iasSync", value)}
        label={t("iasSync.setting")}
        hideLabel
      />
    </Row>
  );
}
