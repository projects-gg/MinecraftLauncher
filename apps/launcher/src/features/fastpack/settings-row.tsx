import { Switch } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { usePlayInstance } from "@/features/settings/graphics-section";
import { Row } from "@/features/settings/row";
import { api, type InstanceSummary } from "@/lib/api";
import { usePrefs } from "@/stores/prefs";
import { fastPackApi } from "./api";
import { fastPackLine, fastPackRow } from "./line";

/** Before the instance list is read. */
const NO_INSTANCES: InstanceSummary[] = [];

/**
 * Settings → Genel "Hızlı kaynak paketi yükleme" (FASTPACK, on by default): the official pack's
 * fast-pack mods (quick-pack) on the versions Studio lists them for, shown while the signed
 * config's `features.fastPacks` is on. Its one place: no mod list shows the mods. The line under
 * it says what it does; for the install Play opens (the command bar's rule) where it does
 * nothing, the reason takes the line's place and the switch is held off and dimmed (the stored
 * preference stays for the installs where it works).
 */
export function FastPackRow() {
  const { t } = useTranslation();
  const on = usePrefs((s) => s.fastPacks);
  const set = usePrefs((s) => s.set);
  const { data: instances } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
  });
  const { instance: play } = usePlayInstance(instances ?? NO_INSTANCES);
  const playId = play?.id ?? null;
  const { data: view } = useQuery({
    queryKey: ["fast-packs", playId],
    queryFn: () => fastPackApi.state(playId),
    staleTime: 60_000,
  });
  if (!view?.shown) return null;
  const row = fastPackRow(on, fastPackLine(view), play?.mcVersion);
  return (
    <Row title={t("fastPack.setting")} help={t(row.help, { version: row.version })}>
      <Switch
        checked={row.checked}
        onCheckedChange={(v) => set("fastPacks", v)}
        label={t("fastPack.setting")}
        hideLabel
        disabled={row.disabled}
      />
    </Row>
  );
}
