import { Switch } from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";

const autoKey = ["cleanup-auto"] as const;

/**
 * Settings → Veri "Otomatik Temizlik" (LAUNCHER-CLEAN, on by default): the daily round deletes
 * what the launcher can make or download again once it has gone unused (logs, crash reports,
 * caches, game versions and Java no instance needs). Laid out like the section's other rows; a
 * click on its words toggles it. No help line: the name says it. The switch answers at once and
 * goes back if the choice could not be kept.
 */
export function AutoCleanupRow() {
  const { t } = useTranslation();
  const id = useId();
  const client = useQueryClient();
  const { data: on } = useQuery({ queryKey: autoKey, queryFn: () => api.cleanup.auto() });
  const turn = useMutation({
    mutationFn: (next: boolean) => api.cleanup.setAuto(next),
    onMutate: (next) => {
      const before = client.getQueryData<boolean>(autoKey);
      client.setQueryData(autoKey, next);
      return { before };
    },
    onError: (_error, _next, context) => {
      if (context?.before !== undefined) client.setQueryData(autoKey, context.before);
    },
    onSuccess: (kept) => client.setQueryData(autoKey, kept),
  });
  if (on === undefined) return null;
  return (
    <div className="switch-row border-b border-white/6 py-4 last:border-b-0">
      <div className="flex items-center justify-between gap-8">
        <label htmlFor={id} className="min-w-0 cursor-pointer select-none">
          <span className="block text-[14px] font-medium text-bone-50">{t("clean.auto")}</span>
        </label>
        <Switch
          id={id}
          checked={on}
          onCheckedChange={(next) => turn.mutate(next)}
          label={t("clean.auto")}
          hideLabel
        />
      </div>
    </div>
  );
}
