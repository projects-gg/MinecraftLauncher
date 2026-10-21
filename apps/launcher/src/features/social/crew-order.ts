// The order of the player's crews (TEAM4): the first is the primary crew ("Ana ekip"). Dragged in
// the crew page's rail, or moved one place (Alt+←/→ there, Alt+↑/↓ on a crew's header in the list,
// and their menus). Shown at once by the backend; a failure puts the old order back and says so.
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import i18n from "@/lib/i18n";
import { attempt } from "./actions";
import { crewFirst, moveCrew } from "./crews";
import { useSocialUi } from "./store";

/** The crews' ids in the player's order. */
function currentOrder(): string[] {
  return useSocialUi.getState().view.crews.map((crew) => crew.id);
}

function nameOf(id: string): string {
  return useSocialUi.getState().view.crews.find((crew) => crew.id === id)?.name ?? "";
}

/** Saves a new order and says where `moved` went ("Gece Kartalları artık 2. sırada"). */
export async function saveCrewOrder(order: string[], moved: string): Promise<void> {
  const before = currentOrder();
  if (order.join() === before.join()) return;
  const done = await attempt(() => api.social.orderCrews(order));
  if (done === undefined) return;
  const position = order.indexOf(moved) + 1;
  announce(
    position === 1
      ? i18n.t("crews.madeMain", { name: nameOf(moved) })
      : i18n.t("crews.moved", { name: nameOf(moved), position }),
  );
}

/** One place earlier (-1) or later (1); nothing at either end. */
export function stepCrew(id: string, step: -1 | 1): void {
  const next = moveCrew(currentOrder(), id, step);
  if (next) void saveCrewOrder(next, id);
}

/** "Ana ekip yap": first in the order. */
export function makeMainCrew(id: string): void {
  const next = crewFirst(currentOrder(), id);
  if (next) void saveCrewOrder(next, id);
}
