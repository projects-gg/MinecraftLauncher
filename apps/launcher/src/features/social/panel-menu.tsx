// The friends panel's own menu: a right click on its empty space or its header, Shift+F10 or the
// menu key there, and the header's "…" button. Adding a friend, the player's code, a new category,
// the team and the status; each one is also on the panel itself (header buttons, the "Yeni
// kategori" row, the status under the player's name). Rows of the list have menus of their own.
import { Copy, DotsThree, FolderSimplePlus, UserPlus, UsersThree } from "@phosphor-icons/react";
import { ContextMenu, IconButton, keepsNativeMenu, Menu, type MenuItem, toast } from "@pl/ui";
import type { KeyboardEvent, MouseEvent, ReactNode, Ref } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { copyText } from "./actions";
import { isMenuKey, openMenuAt } from "./menu-key";
import { panelMenu, rowsOf } from "./menus";
import { PresenceDot } from "./social-face";
import { changeStatus, dotOf, STATUSES } from "./status-menu";
import { useSocialUi } from "./store";

function usePanelItems(): Array<MenuItem | null> {
  const { t } = useTranslation();
  const view = useSocialUi((s) => s.view);
  const pending = useSocialUi((s) => s.statusPending);
  const goTo = useSocialUi((s) => s.goTo);
  const openTeam = useSocialUi((s) => s.openTeam);
  const setCategoryDraft = useSocialUi((s) => s.setCategoryDraft);
  const me = view.me;
  const status = pending ?? me?.status ?? "online";

  const copyCode = (code: string) =>
    void copyText(code).then((ok) => {
      if (!ok) return;
      toast.success(t("friendMenus.codeCopied"));
      announce(t("friendMenus.codeCopied"));
    });

  return rowsOf(
    panelMenu({
      categories: view.groups !== undefined,
      categoryCount: view.groups?.groups.length ?? 0,
      hasFriends: view.friends.length > 0,
      teams: view.teams,
      inTeam: Boolean(view.team),
      hasCode: Boolean(me?.friendCode),
    }),
    (row): MenuItem | null => {
      switch (row.action) {
        case "addFriend":
          return {
            label: t("social.actions.add"),
            icon: <UserPlus size={16} />,
            onSelect: () => goTo("add"),
          };
        case "copyCode":
          return {
            label: t("friendMenus.copyCode"),
            icon: <Copy size={16} />,
            onSelect: () => {
              if (me) copyCode(me.friendCode);
            },
          };
        case "newCategory":
          return {
            label: t("friendGroups.addMenu"),
            icon: <FolderSimplePlus size={16} />,
            disabled: row.disabled,
            // Once the menu is gone: the name field takes the focus (and scrolls into view).
            onSelect: () => window.setTimeout(() => setCategoryDraft({}), 0),
          };
        case "createTeam":
          return {
            label: t("teams.create.title"),
            icon: <UsersThree size={16} />,
            onSelect: () => openTeam(),
          };
        case "openTeam":
          return {
            label: t("teams.open"),
            icon: <UsersThree size={16} />,
            onSelect: () => openTeam(),
          };
        case "status":
          return {
            label: t("friendMenus.status"),
            icon: <PresenceDot kind={dotOf(status)} size={9} />,
            onSelect: () => {},
            items: STATUSES.map((option) => ({
              label: t(`social.status.${option}`),
              icon: <PresenceDot kind={dotOf(option)} size={9} />,
              checked: option === status,
              radio: true,
              onSelect: () => changeStatus(option),
            })),
          };
      }
    },
  );
}

/** The header's "…": the panel's menu where everyone can see it. */
export function PanelMenuButton() {
  const { t } = useTranslation();
  const items = usePanelItems();
  return (
    <Menu
      items={items}
      trigger={
        <IconButton label={t("friendMenus.more")} icon={<DotsThree size={20} weight="bold" />} />
      }
    />
  );
}

/** Text fields keep the system's menu (cut, copy, paste) inside the panel. */
function keepFieldMenus(event: MouseEvent) {
  if (keepsNativeMenu(event.target, "")) event.stopPropagation();
}

/**
 * Shift+F10 or the menu key anywhere in the panel (rows open their own menus first): the menu of
 * what has the focus, or the panel's own when the panel itself has it. Not in text fields.
 */
export function openPanelMenuFromKey(
  event: KeyboardEvent<HTMLElement>,
  enabled: boolean,
  body: HTMLElement | null,
) {
  if (!enabled || !body || event.defaultPrevented || !isMenuKey(event)) return;
  const target = event.target as HTMLElement;
  if (keepsNativeMenu(target, "")) return;
  event.preventDefault();
  if (body.contains(target)) openMenuAt(target);
  else openMenuAt(body, "inside");
}

/**
 * Wraps the panel's header and pages: a right click on anything without a menu of its own opens
 * the panel's. Only on the list (`enabled`).
 */
export function PanelContextMenu({
  enabled,
  bodyRef,
  children,
}: {
  enabled: boolean;
  bodyRef: Ref<HTMLDivElement>;
  children: ReactNode;
}) {
  const items = usePanelItems();
  return (
    <ContextMenu items={items} disabled={!enabled}>
      <div
        ref={bodyRef}
        className="flex min-h-0 flex-1 flex-col"
        onContextMenuCapture={keepFieldMenus}
      >
        {children}
      </div>
    </ContextMenu>
  );
}
