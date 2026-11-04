import { Switch } from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { TelemetryView } from "@/bindings";
import { telemetryApi, telemetryStateQuery } from "./api";

/**
 * Settings → Veri "Kullanım verisi gönder" (TELEMETRY, on by default), shown while Studio's
 * `features.telemetry` is on. Laid out like the section's other rows (title and help on the left,
 * the switch on the right); a click on its words toggles it, as in every settings row. The switch
 * answers at once and goes back if the backend could not keep the choice.
 */
export function UsageDataRow() {
  const { t } = useTranslation();
  const id = useId();
  const helpId = `${id}-help`;
  const client = useQueryClient();
  const { data: view } = useQuery(telemetryStateQuery);
  const configure = useMutation({
    mutationFn: (on: boolean) => telemetryApi.configure(on),
    onMutate: (on) => {
      const before = client.getQueryData<TelemetryView>(telemetryStateQuery.queryKey);
      if (before) client.setQueryData(telemetryStateQuery.queryKey, { ...before, enabled: on });
      return { before };
    },
    onError: (_error, _on, context) => {
      if (context?.before) client.setQueryData(telemetryStateQuery.queryKey, context.before);
    },
    onSuccess: (saved) => client.setQueryData(telemetryStateQuery.queryKey, saved),
  });
  if (!view?.shown) return null;
  return (
    <div className="switch-row border-b border-white/6 py-4 last:border-b-0">
      <div className="flex items-center justify-between gap-8">
        <label htmlFor={id} className="min-w-0 cursor-pointer select-none">
          <span className="block text-[14px] font-medium text-bone-50">
            {t("telemetry.setting")}
          </span>
          <span id={helpId} className="mt-0.5 block max-w-[56ch] text-[12.5px] text-ash-400">
            {t("telemetry.help")}
          </span>
        </label>
        <Switch
          id={id}
          describedBy={helpId}
          checked={view.enabled}
          onCheckedChange={(on) => configure.mutate(on)}
          label={t("telemetry.setting")}
          hideLabel
        />
      </div>
    </div>
  );
}
