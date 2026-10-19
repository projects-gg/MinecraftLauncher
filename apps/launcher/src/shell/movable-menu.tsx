// The right-click menu of a movable shell entry (stores/placement.ts): the entry's own actions
// first, as on rows and tiles, then the move to its other place ("Sol menüye taşı", "Başlık
// çubuğuna taşı"). Opens with a right click and from the keyboard (Shift+F10 or the menu key).
// After a move from the keyboard the focus follows the entry to its new place.
import { AppWindow, SidebarSimple } from "@phosphor-icons/react";
import { ContextMenu, type MenuItem } from "@pl/ui";
import type { KeyboardEvent, ReactElement } from "react";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import {
  entrySelector,
  type MovableId,
  nextPlace,
  type Place,
  usePlace,
  usePlacement,
} from "@/stores/placement";

/** Whether the last input was a key (a keyboard move hands the focus to the moved entry). */
let keyboardLast = false;
let tracking = false;
function trackInput() {
  if (tracking || typeof document === "undefined") return;
  tracking = true;
  const options = { capture: true, passive: true } as const;
  document.addEventListener(
    "keydown",
    () => {
      keyboardLast = true;
    },
    options,
  );
  document.addEventListener(
    "pointerdown",
    () => {
      keyboardLast = false;
    },
    options,
  );
}

/**
 * Shift+F10 and the menu key open the entry's menu at the entry (not every engine sends a
 * `contextmenu` for them, and where they do it lands in a corner of the element): below it in the
 * title bar, beside it in the side menu.
 */
export function openMenuFromKeyboard(event: KeyboardEvent<HTMLElement>, place: Place) {
  const menuKey = event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey);
  if (!menuKey) return;
  event.preventDefault();
  const rect = event.currentTarget.getBoundingClientRect();
  const at =
    place === "rail"
      ? { clientX: rect.right, clientY: rect.top + 4 }
      : { clientX: rect.left + 4, clientY: rect.bottom };
  event.currentTarget.dispatchEvent(
    new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2, ...at }),
  );
}

export function MovableMenu({
  id,
  actions,
  wrapTrigger,
  children,
}: {
  id: MovableId;
  /** The entry's own actions, listed before the move. */
  actions: MenuItem[];
  /** Something that also wraps the trigger, such as its Tooltip. */
  wrapTrigger?: (trigger: ReactElement) => ReactElement;
  /** The entry's button; it carries `data-movable` and `data-place` (`entrySelector`). */
  children: ReactElement;
}) {
  const { t } = useTranslation();
  const place = usePlace(id);
  const move = usePlacement((s) => s.move);
  // Set by the move: where the focus should go once the menu has closed (or nowhere).
  const moved = useRef<{ to: Place; focus: boolean } | null>(null);
  trackInput();

  const to = nextPlace(id, place);
  const items: Array<MenuItem | null> = [
    ...actions,
    ...(actions.length ? [null] : []),
    {
      label: to === "rail" ? t("placement.moveToRail") : t("placement.moveToTitleBar"),
      icon: to === "rail" ? <SidebarSimple size={16} /> : <AppWindow size={16} />,
      onSelect: () => {
        moved.current = { to, focus: keyboardLast };
        move(id, to);
      },
    },
  ];

  return (
    <ContextMenu
      items={items}
      wrapTrigger={wrapTrigger}
      onCloseAutoFocus={(event) => {
        const done = moved.current;
        if (!done) return;
        moved.current = null;
        // The entry left this place: the focus must not go back to it.
        event.preventDefault();
        if (done.focus) document.querySelector<HTMLElement>(entrySelector(id, done.to))?.focus();
      }}
    >
      {children}
    </ContextMenu>
  );
}
