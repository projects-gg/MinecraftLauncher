// AUTOJOIN: "Doğrudan sunucuya bağlan"'s address line as a quiet picker: where the game goes on
// Projects after the lobby's login. A bone caption like the address it replaces (OYNA stays the
// bar's one gold; design.md §3), our menu with the one gold check (keyboard and screen reader as
// every menu). Dim and closed while the switch is off: the pick only rides on a direct join.
import { CaretUpDown } from "@phosphor-icons/react";
import { Menu, type MenuItem } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { InstanceSummary } from "@/lib/api";
import { usePrefs } from "@/stores/prefs";
import type { RouteRow } from "./model";

export function RoutePicker({
  id,
  instance,
  rows,
  current,
  on,
}: {
  /** The switch is described by it (it names where the join goes). */
  id: string;
  instance: InstanceSummary;
  rows: readonly RouteRow[];
  current: RouteRow;
  /** "Doğrudan sunucuya bağlan" is on. */
  on: boolean;
}) {
  const { t } = useTranslation();
  const setRoute = usePrefs((s) => s.setAutoJoinRoute);
  const items: MenuItem[] = rows.map((row) => ({
    label: row.label,
    checked: row.id === current.id,
    radio: true,
    onSelect: () => setRoute(instance.id, row.id),
  }));
  return (
    <Menu
      title={t("autojoin.after")}
      side="top"
      align="end"
      sideOffset={10}
      items={items}
      trigger={
        <button
          type="button"
          id={id}
          disabled={!on}
          aria-label={`${t("autojoin.after")}: ${current.label}`}
          className="group -mx-1 flex max-w-[280px] cursor-pointer items-center gap-1 rounded-sm px-1 text-[12px] text-ash-400 transition-colors duration-150 ease-out hover:text-bone-100 disabled:cursor-default disabled:opacity-45 disabled:hover:text-ash-400 data-[state=open]:text-bone-100"
        >
          <span className="truncate @max-[560px]/bar:hidden">{current.label}</span>
          {current.short ? (
            <span className="hidden truncate @max-[560px]/bar:inline">{current.short}</span>
          ) : null}
          <CaretUpDown size={12} aria-hidden className="shrink-0" />
        </button>
      }
    />
  );
}
