import {
  ArrowsClockwise,
  BookOpenText,
  Gauge,
  Gear,
  House,
  Lightning,
  MagnifyingGlass,
  Newspaper,
  Plus,
  Robot,
  SquaresFour,
  Stop,
  UserPlus,
  UsersFour,
  UsersThree,
} from "@phosphor-icons/react";
import { Kbd, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Command } from "cmdk";
import { type ReactNode, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { AccountFace } from "@/features/accounts/account-face";
import { sortAccounts } from "@/features/accounts/account-order";
import { type CenterFocus, useAccountsUi } from "@/features/accounts/store";
import { ACCOUNTS_KEY, switchAccount } from "@/features/accounts/use-accounts";
import { useSocialUi } from "@/features/social/store";
import { api } from "@/lib/api";
import { fold } from "@/lib/search";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { useUi } from "@/stores/ui";
import { useUpdate } from "@/stores/update";
import { InstanceIcon } from "./instance-icon";

/** Every typed word must appear (folded, lib/search); word starts rank higher. */
function turkishFilter(value: string, search: string, keywords?: string[]): number {
  const haystack = ` ${fold([value, ...(keywords ?? [])].join(" "))}`;
  const terms = fold(search).split(/\s+/).filter(Boolean);
  if (terms.length === 0) return 1;
  let score = 0;
  for (const term of terms) {
    const at = haystack.indexOf(term);
    if (at < 0) return 0;
    score += haystack[at - 1] === " " ? 2 : 1;
  }
  return score / (terms.length * 2);
}

const itemClass =
  "flex cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-[13.5px] text-bone-100 data-[selected=true]:bg-white/8 data-[selected=true]:text-bone-50";

function Item({
  value,
  keywords,
  icon,
  hint,
  onSelect,
  children,
}: {
  value: string;
  keywords?: string[];
  icon: ReactNode;
  hint?: string;
  onSelect: () => void;
  children: ReactNode;
}) {
  return (
    <Command.Item value={value} keywords={keywords} onSelect={onSelect} className={itemClass}>
      <span className="flex size-6 shrink-0 items-center justify-center text-gold-300">{icon}</span>
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {hint ? <span className="shrink-0 text-[12px] text-ash-400">{hint}</span> : null}
    </Command.Item>
  );
}

/** Ctrl+K: play, jump anywhere, open news, run common actions. */
export function CommandPalette({ disabled }: { disabled?: boolean }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const open = useUi((s) => s.paletteOpen);
  const setOpen = useUi((s) => s.setPaletteOpen);
  const play = useLaunch((s) => s.play);
  const selectInstance = usePrefs((s) => s.selectInstance);
  const performanceMode = usePrefs((s) => s.performanceMode);
  const setPerformanceMode = usePrefs((s) => s.setPerformanceMode);
  const { data: instances = [] } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
    enabled: open,
  });
  const { data: home } = useQuery({
    queryKey: ["home-content"],
    queryFn: () => api.homeContent(),
    enabled: open,
  });
  const queryClient = useQueryClient();
  const { data: accounts = [] } = useQuery({
    queryKey: ACCOUNTS_KEY,
    queryFn: () => api.accounts(),
    enabled: open,
  });
  const openCenter = useAccountsUi((s) => s.openCenter);
  const friendsOn = useSocialUi((s) => s.view.enabled);
  const friendsSignedIn = useSocialUi((s) => s.view.auth === "signedIn");
  const openFriends = (page: "list" | "add") =>
    requestAnimationFrame(() => useSocialUi.getState().openPanel({ page }));
  // After the palette has closed, so focus returns to where the player was first.
  const openAccounts = (focus: CenterFocus) => requestAnimationFrame(() => openCenter({ focus }));

  useEffect(() => {
    if (disabled) return;
    const onKey = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setOpen(!useUi.getState().paletteOpen);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [disabled, setOpen]);

  const run = (action: () => void) => {
    setOpen(false);
    action();
  };

  const pages = [
    { to: "/", label: t("nav.home"), icon: <House size={18} /> },
    { to: "/library", label: t("nav.library"), icon: <SquaresFour size={18} /> },
    { to: "/news", label: t("nav.news"), icon: <Newspaper size={18} /> },
    { to: "/afk", label: t("nav.afk"), icon: <Robot size={18} /> },
    { to: "/settings", label: t("nav.settings"), icon: <Gear size={18} /> },
  ] as const;

  return (
    <Command.Dialog
      open={open && !disabled}
      onOpenChange={setOpen}
      label={t("palette.label")}
      filter={turkishFilter}
      loop
      overlayClassName="fixed inset-0 z-50 bg-obsidian-950/55"
      contentClassName="fixed top-[13vh] left-1/2 z-50 w-[min(600px,92vw)] -translate-x-1/2 overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-900/96 shadow-[0_24px_64px_rgb(63_10_10/0.45)] ring-1 ring-white/10 backdrop-blur-xl [html[data-perf=on]_&]:backdrop-blur-none"
    >
      <div className="flex items-center gap-3 border-b border-white/8 px-4">
        <MagnifyingGlass size={18} className="shrink-0 text-ash-400" aria-hidden />
        <Command.Input
          placeholder={t("palette.placeholder")}
          className="h-12 min-w-0 flex-1 bg-transparent text-[14.5px] text-bone-50 outline-none placeholder:text-ash-400"
        />
        <Kbd>Esc</Kbd>
      </div>
      <Command.List className="max-h-[min(420px,60vh)] overflow-y-auto overscroll-contain p-1.5 [&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:pt-2.5 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:text-[12px] [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-ash-400">
        <Command.Empty className="px-3 py-10 text-center text-[13.5px] text-ash-400">
          {t("palette.empty")}
        </Command.Empty>

        {instances.length ? (
          <Command.Group heading={t("palette.play")}>
            {instances.map((instance) => (
              <Item
                key={`play-${instance.id}`}
                value={`play:${instance.id}`}
                keywords={[
                  t("palette.playInstance", { name: instance.name }),
                  instance.mcVersion,
                  t(`loaders.${instance.loader}`),
                ]}
                icon={<InstanceIcon icon={instance.icon} size={24} />}
                hint={instance.mcVersion}
                onSelect={() =>
                  run(() => {
                    selectInstance(instance.id);
                    void navigate({ to: "/" });
                    void play(instance.id);
                  })
                }
              >
                {t("palette.playInstance", { name: instance.name })}
              </Item>
            ))}
          </Command.Group>
        ) : null}

        <Command.Group heading={t("palette.go")}>
          {pages.map((page) => (
            <Item
              key={page.to}
              value={`go:${page.to}`}
              keywords={[page.label]}
              icon={page.icon}
              onSelect={() => run(() => void navigate({ to: page.to }))}
            >
              {page.label}
            </Item>
          ))}
          {instances.map((instance) => (
            <Item
              key={`open-${instance.id}`}
              value={`open:${instance.id}`}
              keywords={[instance.name, t("palette.instances")]}
              icon={<InstanceIcon icon={instance.icon} size={24} />}
              hint={t("palette.instances")}
              onSelect={() =>
                run(
                  () =>
                    void navigate({
                      to: "/library/$instanceId",
                      params: { instanceId: instance.id },
                    }),
                )
              }
            >
              {instance.name}
            </Item>
          ))}
        </Command.Group>

        <Command.Group heading={t("palette.accounts")}>
          {sortAccounts(accounts)
            .filter((account) => !account.active)
            .map((account) => (
              <Item
                key={`account-${account.id}`}
                value={`account:${account.id}`}
                keywords={[
                  t("palette.switchAccount", { name: account.username }),
                  t(`accounts.kind.${account.kind}`),
                ]}
                icon={<AccountFace account={account} size={24} />}
                onSelect={() =>
                  run(() => void switchAccount(queryClient, account, { notify: true }))
                }
              >
                {t("palette.switchAccount", { name: account.username })}
              </Item>
            ))}
          <Item
            value="action:add-account"
            keywords={[t("palette.addAccount"), t("accounts.kind.microsoft")]}
            icon={<UserPlus size={18} />}
            onSelect={() => run(() => openAccounts("add"))}
          >
            {t("palette.addAccount")}
          </Item>
          <Item
            value="action:account-center"
            keywords={[t("palette.openAccountCenter"), t("nav.accounts")]}
            icon={<UsersThree size={18} />}
            onSelect={() => run(() => openAccounts("list"))}
          >
            {t("palette.openAccountCenter")}
          </Item>
          {friendsOn ? (
            <Item
              value="action:friends"
              keywords={[t("social.title"), "Discord"]}
              icon={<UsersFour size={18} />}
              onSelect={() => run(() => openFriends("list"))}
            >
              {t("social.title")}
            </Item>
          ) : null}
          {friendsOn && friendsSignedIn ? (
            <Item
              value="action:add-friend"
              keywords={[t("social.actions.add"), t("social.add.yourCode"), "PRJ"]}
              icon={<UserPlus size={18} />}
              onSelect={() => run(() => openFriends("add"))}
            >
              {t("social.actions.add")}
            </Item>
          ) : null}
        </Command.Group>

        {home?.news.length ? (
          <Command.Group heading={t("palette.news")}>
            {home.news.slice(0, 8).map((item) => (
              <Item
                key={item.id}
                value={`news:${item.id}`}
                keywords={[item.title, item.summary, t(`categories.${item.category}`)]}
                icon={<BookOpenText size={18} />}
                hint={t(`categories.${item.category}`)}
                onSelect={() => run(() => void navigate({ to: "/", search: { news: item.id } }))}
              >
                {item.title}
              </Item>
            ))}
          </Command.Group>
        ) : null}

        <Command.Group heading={t("palette.actions")}>
          <Item
            value="action:new-instance"
            keywords={[t("palette.newInstance")]}
            icon={<Plus size={18} />}
            onSelect={() => run(() => void navigate({ to: "/library", search: { create: true } }))}
          >
            {t("palette.newInstance")}
          </Item>
          <Item
            value="action:afk-all"
            keywords={[t("palette.afkConnectAll")]}
            icon={<Lightning size={18} />}
            onSelect={() =>
              run(() => {
                void api.afkConnectAll();
                toast(t("palette.afkConnecting"));
              })
            }
          >
            {t("palette.afkConnectAll")}
          </Item>
          <Item
            value="action:afk-stop"
            keywords={[t("palette.afkStopAll")]}
            icon={<Stop size={18} />}
            onSelect={() => run(() => void api.afkStopAll())}
          >
            {t("palette.afkStopAll")}
          </Item>
          <Item
            value="action:performance"
            keywords={[t("palette.performanceOn"), t("palette.performanceOff")]}
            icon={<Gauge size={18} />}
            onSelect={() => run(() => setPerformanceMode(!performanceMode))}
          >
            {performanceMode ? t("palette.performanceOff") : t("palette.performanceOn")}
          </Item>
          <Item
            value="action:updates"
            keywords={[t("palette.checkUpdates")]}
            icon={<ArrowsClockwise size={18} />}
            onSelect={() =>
              run(() => {
                toast(t("palette.checkingUpdates"));
                void useUpdate.getState().check();
              })
            }
          >
            {t("palette.checkUpdates")}
          </Item>
        </Command.Group>
      </Command.List>
    </Command.Dialog>
  );
}
