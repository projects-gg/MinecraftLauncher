import {
  ArrowsClockwise,
  BookOpenText,
  Broom,
  CaretLeft,
  CaretRight,
  Coffee,
  CopySimple,
  Cube,
  FileZip,
  FolderOpen,
  Gauge,
  Gear,
  House,
  ImageSquare,
  Images,
  Keyboard,
  Lifebuoy,
  LinkSimple,
  MagnifyingGlass,
  Memory,
  Mountains,
  Newspaper,
  Package,
  Play,
  Plus,
  Robot,
  Scroll,
  SquaresFour,
  StopCircle,
  Sun,
  TShirt,
  UserCircle,
  UserPlus,
  UsersFour,
  UsersThree,
  X,
} from "@phosphor-icons/react";
import { Button, Kbd, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Command } from "cmdk";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AccountFace } from "@/features/accounts/account-face";
import { sortAccounts } from "@/features/accounts/account-order";
import { type CenterFocus, useAccountsUi } from "@/features/accounts/store";
import { ACCOUNTS_KEY, switchAccount } from "@/features/accounts/use-accounts";
import { useOwnLaunchInstance, useOwnLaunchKind } from "@/features/accounts/use-own-launch";
import type { CloudPageData } from "@/features/afk/cloud/api";
import { foldedIds } from "@/features/afk/cloud/twins";
import { CLOUD_KEY, useConnectAll } from "@/features/afk/cloud/use-cloud";
import { runAfk } from "@/features/afk/errors";
import { showError } from "@/features/content/jobs";
import { useHostUi } from "@/features/host/store";
import type { InstanceTab } from "@/features/library/instance-page";
import { useProjectsSkinOverview } from "@/features/projects-skin/queries";
import { useSocialUi } from "@/features/social/store";
import { api, type ServerStatus } from "@/lib/api";
import { hostApi } from "@/lib/api/host-api";
import { copyText } from "@/lib/clipboard";
import { liveMinecraft, minecraftText } from "@/lib/minecraft";
import { detectOs } from "@/lib/platform";
import {
  ESCAPE_KEYS,
  type PageShortcut,
  pressPlayButton,
  SHORTCUT_PAGES,
  type ShortcutId,
  shortcutKeys,
} from "@/lib/shortcuts";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { useUi } from "@/stores/ui";
import { useUpdate } from "@/stores/update";
import { InstanceIcon } from "./instance-icon";
import {
  afkPaletteActions,
  DEFAULT_SERVER_ADDRESS,
  PALETTE_WORDS,
  type PaletteWords,
  paletteFilter,
  paletteKeywords,
  rankGroups,
  updateReport,
} from "./palette-actions";
import { KeyCaps } from "./shortcut-keys";

const itemClass =
  "flex cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-[13.5px] text-bone-100 data-[selected=true]:bg-white/8 data-[selected=true]:text-bone-50";

interface Entry {
  value: string;
  /** Shown, and always searched. */
  label: string;
  /** Everything searched: the label, extra text and the item's words (palette-actions.ts). */
  keywords: string[];
  icon: ReactNode;
  hint?: string;
  /** Key caps at the row's end: the shortcut that does the same from anywhere. */
  keys?: readonly string[];
  /** The row opens a list inside the palette (a caret at its end). */
  opens?: boolean;
  onSelect: () => void;
}

interface Group {
  id: string;
  heading: string;
  entries: Entry[];
}

/** What the palette lists: the commands, or the keyboard shortcuts. */
type View = "commands" | "shortcuts";

function entry(
  value: string,
  label: string,
  icon: ReactNode,
  onSelect: () => void,
  more: {
    words?: PaletteWords;
    extra?: string[];
    hint?: string;
    shortcut?: ShortcutId;
    keys?: readonly string[];
    opens?: boolean;
  } = {},
): Entry {
  return {
    value,
    label,
    icon,
    onSelect,
    hint: more.hint,
    keys: more.shortcut ? shortcutKeys(more.shortcut, detectOs()) : more.keys,
    opens: more.opens,
    keywords: paletteKeywords(label, more.words, more.extra),
  };
}

/**
 * Ctrl+K: play, jump anywhere, open news, run common actions, and find a setting by the problem
 * ("kasıyor", "ram", "ip"). The selected instance's shortcuts join only while searching, so the
 * first view stays short; while searching, the group with the best match comes first. Rows with
 * a global shortcut show its keys, and "Klavye kısayolları" lists them all (Backspace in the
 * empty field goes back). The key itself is handled with the others in app-shell.tsx.
 */
export function CommandPalette({ disabled }: { disabled?: boolean }) {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const open = useUi((s) => s.paletteOpen);
  const setOpen = useUi((s) => s.setPaletteOpen);
  const play = useLaunch((s) => s.play);
  // The command bar's view: another account's game leaves this account's OYNA free (PLAY-ACCT);
  // the instance is this account's launch's or game's (PLAY-AGAIN: other games may run).
  const phaseKind = useOwnLaunchKind();
  const launchId = useOwnLaunchInstance();
  const selectedId = usePrefs((s) => s.selectedInstanceId);
  const selectInstance = usePrefs((s) => s.selectInstance);
  const performanceMode = usePrefs((s) => s.performanceMode);
  const setPerformanceMode = usePrefs((s) => s.setPerformanceMode);
  const [search, setSearch] = useState("");
  const [view, setView] = useState<View>("commands");
  // cmdk keeps its selection when every row changes at once (it picks the first row again only
  // when the search changes), so a new list would start with no row selected. For one render
  // the selection is emptied, cmdk selects the first row, and its answer ends the override.
  const [freshList, setFreshList] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const searching = search.trim().length > 0;
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
  // AFK sessions (the AFK page's cache), read again at every opening: nothing follows them while
  // the palette is closed, and its AFK actions depend on them.
  const { data: afkSessions = [] } = useQuery({
    queryKey: ["afk"],
    queryFn: () => api.afkAccounts(),
    enabled: open,
    staleTime: 0,
  });
  // "Bu bilgisayardaki" as the AFK page's "Buradakileri bağla": an account also in 7/24 Bulut is
  // its cloud card there, so connecting leaves it out (the page's cache, as useConnectAll reads it).
  const afk = afkPaletteActions(
    afkSessions,
    foldedIds(queryClient.getQueryData<CloudPageData>(CLOUD_KEY)),
  );
  const connectAll = useConnectAll();
  const openCenter = useAccountsUi((s) => s.openCenter);
  const friendsOn = useSocialUi((s) => s.view.enabled);
  const friendsSignedIn = useSocialUi((s) => s.view.auth === "signedIn");
  const projectsSkin = useProjectsSkinOverview().data;
  // The command bar's instance: the one a game is prepared or played for, else the selected.
  const busy = phaseKind === "working" || phaseKind === "launching" || phaseKind === "running";
  const selected =
    (busy ? instances.find((i) => i.id === launchId) : undefined) ??
    instances.find((i) => i.id === selectedId) ??
    instances[0];
  // World sharing (HOST1): the open world's sheet, else the Worlds tab of the command bar's
  // instance while it may share its worlds (asked at every opening, like the AFK accounts).
  const worldsOn = useHostUi((s) => s.view.enabled) && friendsSignedIn;
  const worldOpen = useHostUi((s) => s.view.session !== null);
  const { data: hostPlace } = useQuery({
    queryKey: ["host-place", selected?.id, "palette"],
    queryFn: () => hostApi.place(selected?.id ?? ""),
    enabled: open && worldsOn && !worldOpen && Boolean(selected),
    staleTime: 0,
  });

  useEffect(() => {
    if (!open) {
      // Every opening starts empty, on the commands.
      setSearch("");
      setView("commands");
      setFreshList(false);
      return;
    }
    return api.onAfkChange(() => void queryClient.invalidateQueries({ queryKey: ["afk"] }));
  }, [open, queryClient]);

  const showView = (next: View) => {
    setView(next);
    setSearch("");
    setFreshList(true);
    // The field keeps the keys (the back chip may have taken the focus).
    inputRef.current?.focus();
  };

  const run = (action: () => void) => {
    setOpen(false);
    action();
  };
  // After the palette has closed, so focus returns to where the player was first.
  const afterClose = (action: () => void) => requestAnimationFrame(action);
  const openAccounts = (focus: CenterFocus) => afterClose(() => openCenter({ focus }));
  const openFriends = (page: "list" | "add") =>
    afterClose(() => useSocialUi.getState().openPanel({ page }));
  const openSettings = (section: string) => void navigate({ to: "/settings", search: { section } });
  // `focus`: the settings group to land on (its control takes the focus).
  const openInstance = (instanceId: string, tab?: InstanceTab, focus?: "graphics" | "memory") =>
    void navigate({
      to: "/library/$instanceId",
      params: { instanceId },
      search: tab ? { tab, focus } : {},
    });

  const copyAddress = () => {
    const status = queryClient.getQueryData<ServerStatus>(["server-status"]);
    copyText(status?.address || DEFAULT_SERVER_ADDRESS).then(
      () => toast.success(t("instance.servers.copied")),
      () => toast.error(t("common.copyFailed")),
    );
  };

  // A manual check (it downloads what it finds), answered in the same toast like Settings does.
  const checkUpdates = async () => {
    const id = toast(t("palette.checkingUpdates"));
    const report = updateReport(await useUpdate.getState().check(true));
    if (!report) return;
    const text = t(report.key);
    if (report.tone === "success") toast.success(text, { id });
    else if (report.tone === "error") toast.error(text, { id });
    else toast(text, { id });
  };

  const groups = (): Group[] => {
    const activeAccount = accounts.find((a) => a.active);
    const tab = (id: InstanceTab) => () => selected && run(() => openInstance(selected.id, id));
    const setting = (focus: "graphics" | "memory") => () =>
      selected && run(() => openInstance(selected.id, "settings", focus));

    return [
      {
        id: "play",
        heading: t("palette.play"),
        entries: instances.map((instance) =>
          entry(
            `play:${instance.id}`,
            t("palette.playInstance", { name: instance.name }),
            <InstanceIcon icon={instance.icon} size={24} />,
            () =>
              run(() => {
                selectInstance(instance.id);
                void navigate({ to: "/" });
                void play(instance.id);
              }),
            {
              extra: [instance.mcVersion, t(`loaders.${instance.loader}`)],
              hint: instance.mcVersion,
              // Ctrl+Enter presses OYNA: the command bar's instance, while nothing runs.
              shortcut: !busy && instance.id === selected?.id ? "play" : undefined,
            },
          ),
        ),
      },
      {
        id: "go",
        heading: t("palette.go"),
        entries: [
          entry(
            "go:/",
            t("nav.home"),
            <House size={18} />,
            () => run(() => void navigate({ to: "/" })),
            {
              words: PALETTE_WORDS.home,
              shortcut: "home",
            },
          ),
          entry(
            "go:/library",
            t("nav.library"),
            <SquaresFour size={18} />,
            () => run(() => void navigate({ to: "/library" })),
            { words: PALETTE_WORDS.library, shortcut: "library" },
          ),
          entry(
            "go:/news",
            t("nav.news"),
            <Newspaper size={18} />,
            () => run(() => void navigate({ to: "/news" })),
            { words: PALETTE_WORDS.news, shortcut: "news" },
          ),
          entry(
            "go:/afk",
            t("nav.afk"),
            <Robot size={18} />,
            () => run(() => void navigate({ to: "/afk" })),
            {
              words: PALETTE_WORDS.afk,
              shortcut: "afk",
            },
          ),
          entry(
            "go:/settings",
            t("nav.settings"),
            <Gear size={18} />,
            () => run(() => void navigate({ to: "/settings" })),
            { words: PALETTE_WORDS.settings, shortcut: "settings" },
          ),
          ...instances.map((instance) =>
            entry(
              `open:${instance.id}`,
              instance.name,
              <InstanceIcon icon={instance.icon} size={24} />,
              () => run(() => openInstance(instance.id)),
              { extra: [t("palette.instances")], hint: t("palette.instances") },
            ),
          ),
        ],
      },
      // The selected instance's corners, by the words players use for them.
      {
        id: "instance",
        heading: selected?.name ?? "",
        entries:
          searching && selected
            ? [
                entry(
                  "instance:memory",
                  t("tuning.palette.memory"),
                  <Memory size={18} />,
                  setting("memory"),
                  {
                    words: PALETTE_WORDS.memory,
                  },
                ),
                ...(selected.loader === "vanilla"
                  ? []
                  : [
                      entry(
                        "instance:mods",
                        t("instance.tabs.mods"),
                        <Package size={18} />,
                        tab("mods"),
                        {
                          words: PALETTE_WORDS.mods,
                        },
                      ),
                    ]),
                entry(
                  "instance:resource-packs",
                  t("instance.tabs.resourcePacks"),
                  <Images size={18} />,
                  tab("resourcePacks"),
                  { words: PALETTE_WORDS.resourcePacks },
                ),
                entry(
                  "instance:shaders",
                  t("instance.tabs.shaders"),
                  <Sun size={18} />,
                  tab("shaders"),
                  {
                    words: PALETTE_WORDS.shaders,
                  },
                ),
                entry(
                  "instance:worlds",
                  t("instance.tabs.worlds"),
                  // A world is a block here as on its tab (ICON1: a globe reads "internet").
                  <Cube size={18} />,
                  tab("worlds"),
                  {
                    words: PALETTE_WORDS.worlds,
                  },
                ),
                entry(
                  "instance:screenshots",
                  t("instance.tabs.screenshots"),
                  <ImageSquare size={18} />,
                  tab("screenshots"),
                  { words: PALETTE_WORDS.screenshots },
                ),
                entry("instance:logs", t("instance.tabs.logs"), <Scroll size={18} />, tab("logs"), {
                  words: PALETTE_WORDS.logs,
                }),
                entry(
                  "instance:folder",
                  t("tuning.palette.openFolder"),
                  <FolderOpen size={18} />,
                  () => run(() => void api.openInstanceFolder(selected.id).catch(showError)),
                  { words: PALETTE_WORDS.folder },
                ),
              ]
            : [],
      },
      {
        id: "accounts",
        heading: t("palette.accounts"),
        entries: [
          ...sortAccounts(accounts)
            .filter((account) => !account.active)
            .map((account) =>
              entry(
                `account:${account.id}`,
                t("palette.switchAccount", { name: account.username }),
                <AccountFace account={account} size={24} />,
                () => run(() => void switchAccount(queryClient, account, { notify: true })),
                { extra: account.kind === "microsoft" ? [t("accounts.kind.microsoft")] : [] },
              ),
            ),
          ...(activeAccount?.kind === "microsoft"
            ? [
                entry(
                  "action:appearance",
                  t("tuning.palette.appearance"),
                  <TShirt size={18} />,
                  () =>
                    run(() =>
                      afterClose(() => useAccountsUi.getState().openAppearance(activeAccount.id)),
                    ),
                  { words: PALETTE_WORDS.appearance },
                ),
              ]
            : activeAccount
              ? // Offline accounts: their Projects skin while it is offered (SKIN1), and their
                // picture, chosen in the account center.
                [
                  ...(projectsSkin?.shown
                    ? [
                        entry(
                          "action:projects-skin",
                          t("projectsSkin.palette.skin"),
                          <TShirt size={18} />,
                          () =>
                            run(() =>
                              afterClose(() =>
                                useAccountsUi.getState().openAppearance(activeAccount.id),
                              ),
                            ),
                          { words: PALETTE_WORDS.projectsSkin },
                        ),
                      ]
                    : []),
                  entry(
                    "action:avatar",
                    t("accounts.avatar.change"),
                    <UserCircle size={18} />,
                    () => run(() => openAccounts("list")),
                    { words: PALETTE_WORDS.avatar },
                  ),
                ]
              : []),
          entry(
            "action:add-account",
            t("palette.addAccount"),
            <UserPlus size={18} />,
            () => run(() => openAccounts("add")),
            {
              words: PALETTE_WORDS.addAccount,
              extra: [t("accounts.kind.microsoft")],
            },
          ),
          entry(
            "action:account-center",
            t("palette.openAccountCenter"),
            <UsersThree size={18} />,
            () => run(() => openAccounts("list")),
            { words: PALETTE_WORDS.accountCenter, extra: [t("nav.accounts")] },
          ),
          ...(friendsOn
            ? [
                entry(
                  "action:friends",
                  t("social.title"),
                  <UsersFour size={18} />,
                  () => run(() => openFriends("list")),
                  {
                    words: PALETTE_WORDS.friends,
                  },
                ),
              ]
            : []),
          ...(friendsOn && friendsSignedIn
            ? [
                entry(
                  "action:add-friend",
                  t("social.actions.add"),
                  <UserPlus size={18} />,
                  () => run(() => openFriends("add")),
                  { words: PALETTE_WORDS.addFriend, extra: [t("social.add.yourCode")] },
                ),
              ]
            : []),
          // The game account's link on Projects (SKIN1): the account center, at its row.
          ...(friendsSignedIn && projectsSkin?.shown && projectsSkin.service?.link
            ? [
                entry(
                  "action:projects-link",
                  projectsSkin.link?.state === "linked"
                    ? t("projectsSkin.palette.linked")
                    : t("projectsSkin.palette.link"),
                  <LinkSimple size={18} />,
                  () =>
                    run(() => {
                      useAccountsUi.getState().wantLinkRow(true);
                      openAccounts("list");
                    }),
                  { words: PALETTE_WORDS.projectsLink },
                ),
              ]
            : []),
        ],
      },
      {
        id: "settings",
        heading: t("nav.settings"),
        entries: [
          // In-game lag: the one-tap graphics preset of the instance Play launches (HOME1).
          entry(
            "setting:graphics",
            t("fps.title"),
            <Mountains size={18} />,
            () => run(() => openSettings("graphics")),
            { words: PALETTE_WORDS.graphics },
          ),
          // The game that won't start or connect, a crash, an error (HELP1).
          entry(
            "setting:help",
            t("help.title"),
            <Lifebuoy size={18} />,
            () => run(() => openSettings("help")),
            { words: PALETTE_WORDS.help },
          ),
          entry(
            "setting:performance",
            performanceMode ? t("palette.performanceOff") : t("palette.performanceOn"),
            <Gauge size={18} />,
            () => run(() => setPerformanceMode(!performanceMode)),
            {
              words: PALETTE_WORDS.performance,
              extra: [t("palette.performanceOn"), t("palette.performanceOff")],
            },
          ),
          entry(
            "setting:java",
            t("tuning.palette.java"),
            <Coffee size={18} />,
            () => run(() => openSettings("java")),
            {
              words: PALETTE_WORDS.java,
            },
          ),
          entry(
            "setting:cleanup",
            t("tuning.palette.cleanup"),
            <Broom size={18} />,
            () => run(() => openSettings("data")),
            { words: PALETTE_WORDS.cleanup },
          ),
          ...(searching
            ? [
                entry(
                  "setting:support",
                  t("tuning.palette.support"),
                  <FileZip size={18} />,
                  () => run(() => openSettings("data")),
                  { words: PALETTE_WORDS.support },
                ),
              ]
            : []),
        ],
      },
      {
        id: "actions",
        heading: t("palette.actions"),
        entries: [
          entry(
            "action:new-instance",
            t("palette.newInstance"),
            <Plus size={18} />,
            () => run(() => void navigate({ to: "/library", search: { create: true } })),
            { words: PALETTE_WORDS.newInstance },
          ),
          entry(
            "action:copy-address",
            t("tuning.palette.copyAddress"),
            <CopySimple size={18} />,
            () => run(copyAddress),
            { words: PALETTE_WORDS.copyAddress },
          ),
          ...(worldsOn && worldOpen
            ? [
                entry(
                  "action:host-world",
                  t("small9.palette.manageWorld"),
                  <UsersThree size={18} />,
                  () => run(() => afterClose(() => useHostUi.getState().openSheet())),
                  { words: PALETTE_WORDS.hostWorld },
                ),
              ]
            : worldsOn && hostPlace?.shown
              ? [
                  entry(
                    "action:host-world",
                    t("small9.palette.openWorld"),
                    <UsersThree size={18} />,
                    tab("worlds"),
                    { words: PALETTE_WORDS.hostWorld },
                  ),
                ]
              : []),
          // Like the AFK page: only with AFK accounts, "durdur" only while one runs.
          ...(afk.connectAll
            ? [
                entry(
                  "action:afk-all",
                  t("palette.afkConnectAll"),
                  // The page's "Buradakileri bağla" mark and its way (twins left out).
                  <Play size={18} />,
                  () =>
                    run(() => {
                      connectAll();
                      toast(t("palette.afkConnecting"));
                    }),
                  { words: PALETTE_WORDS.afkConnectAll },
                ),
              ]
            : []),
          ...(afk.stopAll
            ? [
                entry(
                  "action:afk-stop",
                  t("palette.afkStopAll"),
                  <StopCircle size={18} />,
                  () => run(() => runAfk(api.afkStopAll(), t("afk.stopFailed"))),
                  { words: PALETTE_WORDS.afkStopAll },
                ),
              ]
            : []),
          entry(
            "action:updates",
            t("palette.checkUpdates"),
            <ArrowsClockwise size={18} />,
            () => run(() => void checkUpdates()),
            { words: PALETTE_WORDS.updates },
          ),
          entry(
            "action:shortcuts",
            t("shortcuts.title"),
            <Keyboard size={18} />,
            () => showView("shortcuts"),
            { words: PALETTE_WORDS.shortcuts, opens: true },
          ),
        ],
      },
      {
        id: "news",
        heading: t("palette.news"),
        entries: [
          ...(home?.news ?? []).map((item) =>
            entry(
              `news:${item.id}`,
              item.title,
              <BookOpenText size={18} />,
              () => run(() => void navigate({ to: "/", search: { news: item.id } })),
              {
                // A summary's words rank below the items they name ("FPS" in a patch note).
                words: { keywords: [t(`categories.${item.category}`)], related: [item.summary] },
                hint: t(`categories.${item.category}`),
              },
            ),
          ),
          // Minecraft's updates only take places Projects news leaves, here too.
          ...liveMinecraft(home?.minecraft, Date.now()).map((item) => {
            const text = minecraftText(item, i18n.language === "en" ? "en" : "tr");
            return entry(
              `news:${item.id}`,
              text.title,
              <BookOpenText size={18} />,
              () => run(() => void navigate({ to: "/", search: { news: item.id } })),
              {
                words: { keywords: [t("minecraft.label"), item.version], related: [text.summary] },
                hint: t("minecraft.label"),
              },
            );
          }),
        ].slice(0, 8),
      },
    ];
  };

  // Every global shortcut with its keys (lib/shortcuts.ts); a row does what its keys do.
  const shortcutGroups = (): Group[] => {
    const keysOf = (id: ShortcutId) => shortcutKeys(id, detectOs());
    const goTo = (id: PageShortcut, label: string, icon: ReactNode, words: PaletteWords) =>
      entry(
        `shortcut:${id}`,
        label,
        icon,
        () => run(() => void navigate({ to: SHORTCUT_PAGES[id] })),
        { words, shortcut: id, extra: keysOf(id) },
      );
    return [
      {
        id: "shortcuts",
        heading: t("shortcuts.title"),
        entries: [
          entry(
            "shortcut:palette",
            t("palette.label"),
            <MagnifyingGlass size={18} />,
            () => showView("commands"),
            { shortcut: "palette", extra: [t("palette.open"), ...keysOf("palette")] },
          ),
          entry(
            "shortcut:play",
            t("play.play"),
            <Play size={18} />,
            () => run(() => pressPlayButton(document)),
            { shortcut: "play", extra: keysOf("play") },
          ),
          goTo("home", t("nav.home"), <House size={18} />, PALETTE_WORDS.home),
          goTo("library", t("nav.library"), <SquaresFour size={18} />, PALETTE_WORDS.library),
          goTo("news", t("nav.news"), <Newspaper size={18} />, PALETTE_WORDS.news),
          goTo("afk", t("nav.afk"), <Robot size={18} />, PALETTE_WORDS.afk),
          goTo("settings", t("nav.settings"), <Gear size={18} />, PALETTE_WORDS.settings),
          entry("shortcut:close", t("common.close"), <X size={18} />, () => setOpen(false), {
            keys: ESCAPE_KEYS,
            extra: [...ESCAPE_KEYS, "escape"],
          }),
        ],
      },
    ];
  };

  // Nothing to build while closed: the dialog renders no content then.
  const shown = open
    ? rankGroups(view === "shortcuts" ? shortcutGroups() : groups(), search).filter(
        (g) => g.entries.length > 0,
      )
    : [];

  return (
    <Command.Dialog
      open={open && !disabled}
      onOpenChange={setOpen}
      label={t("palette.label")}
      filter={paletteFilter}
      loop
      value={freshList ? "" : undefined}
      onValueChange={freshList ? () => setFreshList(false) : undefined}
      onKeyDown={(event) => {
        // Backspace in the empty field leaves a list (Esc closes the palette, as everywhere).
        if (view !== "commands" && event.key === "Backspace" && search === "") {
          event.preventDefault();
          showView("commands");
        }
      }}
      overlayClassName="fixed inset-0 z-50 bg-obsidian-950/55"
      contentClassName="fixed top-[13vh] left-1/2 z-50 w-[min(600px,92vw)] -translate-x-1/2 overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-900/96 shadow-[0_24px_64px_rgb(63_10_10/0.45)] ring-1 ring-white/10 backdrop-blur-xl [html[data-perf=on]_&]:backdrop-blur-none"
    >
      <div className="flex items-center gap-3 border-b border-white/8 px-4">
        <MagnifyingGlass size={18} className="shrink-0 text-ash-400" aria-hidden />
        {view === "shortcuts" ? (
          <Button
            variant="secondary"
            size="sm"
            icon={<CaretLeft size={12} weight="bold" />}
            aria-label={t("shortcuts.back")}
            // A click leaves the focus in the field.
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => showView("commands")}
          >
            {t("shortcuts.page")}
          </Button>
        ) : null}
        <Command.Input
          ref={inputRef}
          value={search}
          onValueChange={setSearch}
          placeholder={view === "shortcuts" ? t("shortcuts.placeholder") : t("palette.placeholder")}
          className="h-12 min-w-0 flex-1 bg-transparent text-[14.5px] text-bone-50 outline-none placeholder:text-ash-400"
        />
        <Kbd>Esc</Kbd>
      </div>
      <Command.List className="max-h-[min(420px,60vh)] overflow-y-auto overscroll-contain p-1.5 [&_[cmdk-group-heading]]:truncate [&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:pt-2.5 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:text-[12px] [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-ash-400">
        <Command.Empty className="px-3 py-10 text-center text-[13.5px] text-ash-400">
          {t("palette.empty")}
        </Command.Empty>
        {shown.map((group) => (
          <Command.Group key={group.id} heading={group.heading}>
            {group.entries.map((item) => (
              <Command.Item
                key={item.value}
                value={item.value}
                keywords={item.keywords}
                onSelect={item.onSelect}
                className={itemClass}
              >
                <span className="flex size-6 shrink-0 items-center justify-center text-gold-300">
                  {item.icon}
                </span>
                <span className="min-w-0 flex-1 truncate">{item.label}</span>
                {item.hint ? (
                  <span className="shrink-0 text-[12px] text-ash-400">{item.hint}</span>
                ) : null}
                {item.keys ? <KeyCaps keys={item.keys} /> : null}
                {item.opens ? (
                  <CaretRight size={14} aria-hidden className="shrink-0 text-ash-400" />
                ) : null}
              </Command.Item>
            ))}
          </Command.Group>
        ))}
      </Command.List>
    </Command.Dialog>
  );
}
