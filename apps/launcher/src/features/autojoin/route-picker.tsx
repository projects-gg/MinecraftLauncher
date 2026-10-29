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
    // The lobby row says so too: the narrow bar calls that pick "Lobi".
    label: row.id === null ? `${row.label} (${t("autojoin.lobby")})` : row.label,
    checked: row.id === current.id,
    radio: true,
    onSelect: () => setRoute(instance.id, row.id),
  }));
  return (
    <Menu
      title={t("autojoin.title")}
      side="top"
      align="end"
      // Above the bar, where the instance picker's menu opens (its bottom on the bar's top edge),
      // not over the switch's own words: the trigger sits 37 px under that edge (the label line
      // and the bar's padding above it).
      sideOffset={37}
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
          {/* The narrow bar drops the address: "SkyBlock", or "Lobi" for the lobby. */}
          <span className="hidden truncate @max-[560px]/bar:inline">
            {current.short ?? t("autojoin.lobby")}
          </span>
          {/* No picking mark while the switch is off (the pick rides only on a direct join); its
              room stays, so the words do not move. */}
          <CaretUpDown size={12} aria-hidden className={on ? "shrink-0" : "invisible shrink-0"} />
        </button>
      }
    />
  );
}
