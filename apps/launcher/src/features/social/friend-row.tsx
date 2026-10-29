// One friend in the list: picture with presence, name, where they are, and "Katıl" when they are
// on projects.gg. The row opens their card (beside the panel; a click, Enter or a double click):
// join, "Birlikte oyna", "Mesaj" (chat, CHAT1), "Discord'dan mesaj at", favourite, the online
// notice, and in a quieter corner remove, block and "Rapor et" (MOD1), the first two confirmed in
// place with one sentence. The row's menu (a right click, Shift+F10 or the menu key, or its "…")
// is the person menu every place shows (person-menu.tsx, FIX2). With several crews (TEAM4) "Ekibe
// davet et" asks which crew (a menu of them, each with its mark). A friend whose world invite the
// player has (HOST1) reads "Dünyasında" with "2/8", and "Katıl" joins their world (only invitees
// ever see this; everyone else still sees "Tek oyunculu").
import {
  Bell,
  ChatCircle,
  DotsThree,
  Flag,
  PaperPlaneTilt,
  Play,
  Prohibit,
  Star,
  UserMinus,
  UsersThree,
} from "@phosphor-icons/react";
import {
  Button,
  ContextMenu,
  cn,
  easeOutExpo,
  IconButton,
  Menu,
  type MenuItem,
  returnFocusAfterKeys,
  Select,
  swallowDismissClick,
  Tooltip,
} from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { DropdownMenu, Popover } from "radix-ui";
import { type KeyboardEvent, type PointerEvent as ReactPointerEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import { announce } from "@/features/accounts/use-accounts";
import { PeopleCount, WorldJoinButton } from "@/features/host/parts";
import { inviteFrom, invitePeople } from "@/features/host/rules";
import { useHostUi } from "@/features/host/store";
import { api } from "@/lib/api";
import type { WorldInvite } from "@/lib/api/host-types";
import type { Friend, FriendGroup, SocialTeam } from "@/lib/api/social-types";
import { dateTimeFormat } from "@/lib/format";
import { attempt, inviteCoolingDown, inviteFriend } from "./actions";
import { directConversation } from "./chat/conversations";
import { BadgedName, badgeRowLabel, focusFirstAction, useShownBadge } from "./crew-badge";
import type { InviteState } from "./crews";
import { DiscordButton } from "./discord-button";
import { openInDiscord } from "./discord-link";
import { REST } from "./groups";
import { joinOrAsk, PROJECTS, useThere } from "./join-dialog";
import { openMenuFromKey } from "./menu-key";
import { isJoinable, notifiesOnline, presenceLine } from "./model";
import { PresenceText } from "./presence-text";
import { rise, swap, useSocialMotion } from "./motion";
import { usePersonItems } from "./person-menu";
import { canInvite } from "./play-together";
import { SocialAvatar } from "./social-face";
import { useSocialPrefs, useSocialUi } from "./store";
import { CrewMark } from "./team-emblem";

/** The friend's category and where else they may go (categories mode of the list). */
export interface RowCategories {
  groups: FriendGroup[];
  /** The friend's category id, or REST. */
  current: string;
  onMove: (group: string) => void;
  /** A new category with this friend in it (its name is asked in the list). */
  onNew: () => void;
}

/** The player's team, for "Takıma davet et". */
export interface RowTeam {
  /** With several crews, the best of them (one to invite to first). */
  state: InviteState;
  onInvite: () => void;
  /** Several crews the friend may be invited to (TEAM4): the row asks which. */
  crews?: Array<{ crew: SocialTeam; state: InviteState; onInvite: () => void }>;
}

/** "Ekibe davet et"'s crews, as menu rows: invited or in it (checked), full, or to invite. */
function crewInviteItems(
  crews: NonNullable<RowTeam["crews"]>,
  t: (key: string) => string,
): MenuItem[] {
  return crews.map(({ crew, state, onInvite }) => ({
    label: state === "full" ? `${crew.name} (${t("crews.invite.full")})` : crew.name,
    icon: <CrewMark look={crew} size={16} />,
    disabled: state !== "invitable",
    checked: state === "invited" || state === "member" ? true : undefined,
    onSelect: onInvite,
  }));
}

// In the interface's language, looked up at each call (the language can change live).
const dateFormat = () => dateTimeFormat({ day: "numeric", month: "long", year: "numeric" });

/** "Mesaj": a friend's conversation (chat on), or nothing to offer. */
function useOpenChat(friend: Pick<Friend, "id">): (() => void) | null {
  const chat = useSocialUi((s) => Boolean(s.view.chat));
  const me = useSocialUi((s) => s.view.me?.id);
  const openChat = useSocialUi((s) => s.openChat);
  if (!chat || !me) return null;
  return () => openChat(directConversation(me, friend.id), { from: "list" });
}

/** The friend's world invite to the player (HOST1), while world sharing is on. */
function useWorldInvite(friend: Pick<Friend, "id">): WorldInvite | null {
  return useHostUi((s) => (s.view.enabled ? inviteFrom(s.view.invites, friend.id) : null));
}

/** "Dünyasında" and "2/8" (the friend's world, the friend counted). */
function WorldLine({ invite }: { invite: WorldInvite }) {
  const { t } = useTranslation();
  const people = invitePeople(invite);
  return (
    <>
      {t("host.invite.inWorld")}{" "}
      <PeopleCount
        count={people.count}
        max={people.max}
        label={t("host.invite.count", people)}
        className="ml-1 text-ash-400"
      />
    </>
  );
}

/** "Rapor et" about a friend. */
function reportFriend(friend: Friend) {
  useSocialUi.getState().openReport({
    target: { kind: "player", id: friend.id },
    name: friend.name,
    person: friend,
  });
}

/**
 * "Birlikte oyna" is offered only from a server the invite will carry: projects.gg, or another
 * server while the player shares its address (play-together.ts). No hint otherwise: in the
 * launcher or in singleplayer there is nothing to join yet.
 */
function useCanInvite(): boolean {
  return useSocialUi((s) => canInvite(s.view.activity, s.view.me?.privacy.shareServer ?? false));
}

/**
 * "Katıl": disabled (with the reason) while the player's game is on projects.gg already; with a
 * game open elsewhere or opening it asks first and goes there (FIX2 "Bağlan"). A disabled button
 * takes no pointer, so the tooltip hangs on its wrapper, there all the time (and quiet while the
 * button works): the button stays the same and fades into its waiting look. The wrapper takes the
 * layout (`w-full` in the friend's card) and the button fills it.
 */
export function JoinButton({
  size = "sm",
  variant = "secondary",
  glyph = false,
  className,
}: {
  size?: "sm" | "md";
  variant?: "primary" | "secondary";
  /**
   * Home's "Katıl", in the event card's form: the gold play glyph before the word (SMALL10). The
   * panel's rows go without: a gold mark on every row of a list is noise.
   */
  glyph?: boolean;
  className?: string;
}) {
  const { t } = useTranslation();
  const here = useThere(PROJECTS);
  return (
    <Tooltip content={t("playTogether.already")} disabled={!here}>
      <span className={cn("inline-flex", className)}>
        <Button
          size={size}
          variant={variant}
          icon={glyph ? <Play size={13} weight="fill" className="text-gold-300" /> : undefined}
          disabled={here}
          className="w-full"
          onClick={(event) => {
            event.stopPropagation();
            joinOrAsk(PROJECTS);
          }}
        >
          {t("social.actions.join")}
        </Button>
      </span>
    </Tooltip>
  );
}

const menuItemClass =
  "flex h-10 cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-[13.5px] outline-none select-none data-[highlighted]:bg-white/8";

type Confirm = "remove" | "block";

function FriendCard({
  friend,
  initialConfirm,
  categories,
  team,
  onDone,
}: {
  friend: Friend;
  /** Opened from the right-click menu's remove or block: the card starts at that question. */
  initialConfirm: Confirm | null;
  categories?: RowCategories | null;
  team?: RowTeam | null;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const favorite = useSocialPrefs((s) => s.favorites.includes(friend.id));
  const toggleFavorite = useSocialPrefs((s) => s.toggleFavorite);
  const notify = useSocialPrefs((s) => notifiesOnline(s, friend.id));
  const setOnlineAlert = useSocialPrefs((s) => s.setOnlineAlert);
  const cooldown = useSocialUi((s) => s.inviteCooldown);
  const invitable = useCanInvite();
  const [confirm, setConfirm] = useState<Confirm | null>(initialConfirm);
  const [busy, setBusy] = useState(false);
  const world = useWorldInvite(friend);
  const joinable = friend.state === "playing" && isJoinable(friend.game);
  const coolingDown = inviteCoolingDown(friend.id, cooldown);
  const openChat = useOpenChat(friend);

  const run = async (action: "remove" | "block") => {
    setBusy(true);
    const done = await attempt(() =>
      action === "remove" ? api.social.removeFriend(friend.id) : api.social.block(friend.id),
    );
    setBusy(false);
    if (done === undefined) return;
    announce(
      t(action === "remove" ? "social.actions.removed" : "social.actions.blocked", {
        name: friend.name,
      }),
    );
    onDone();
  };

  return (
    <div className="w-[300px]">
      <div className="flex items-center gap-3.5">
        <SocialAvatar
          source={friend}
          size={56}
          state={friend.state}
          dnd={friend.dnd}
          surface="var(--color-obsidian-900)"
        />
        <div className="min-w-0">
          {/* The tag they wear before the name, a button of its own here (TEAM6). */}
          <BadgedName
            name={friend.name}
            badge={friend.badge}
            wearer={friend.id}
            mark={friend.mark}
            size="lg"
            chip="button"
            className="gap-2"
            nameClassName="font-display text-[18px] leading-tight font-semibold text-bone-50"
          />
          <p className="truncate text-[12.5px] text-ash-400" data-selectable>
            @{friend.username}
          </p>
          <p
            className={cn(
              "mt-1 truncate text-[13px]",
              joinable || world
                ? "text-jade-400"
                : friend.state === "offline"
                  ? "text-ash-400"
                  : "text-ash-300",
            )}
          >
            {world ? (
              <WorldLine invite={world} />
            ) : (
              <PresenceText friend={friend} t={t} joinable={joinable} />
            )}
            {friend.dnd ? (
              <span className="text-gold-300"> ({t("social.presence.dnd")})</span>
            ) : null}
          </p>
        </div>
      </div>
      {friend.since ? (
        <p className="mt-3 text-[12px] text-ash-400">
          {t("social.actions.friendsSince", { date: dateFormat().format(new Date(friend.since)) })}
        </p>
      ) : null}

      <div className="mt-4 flex flex-col gap-2">
        {world ? (
          <WorldJoinButton invite={world} size="md" variant="primary" className="w-full" />
        ) : joinable ? (
          <JoinButton size="md" variant="primary" className="w-full" />
        ) : null}
        {invitable ? (
          <Button
            variant="secondary"
            className="w-full"
            icon={<PaperPlaneTilt size={16} />}
            disabled={coolingDown}
            onClick={() => void inviteFriend(friend)}
          >
            {coolingDown ? t("social.actions.invited") : t("social.actions.invite")}
          </Button>
        ) : null}
        {openChat ? (
          <Button
            variant={joinable || world ? "secondary" : "primary"}
            className="w-full"
            icon={<ChatCircle size={16} weight={joinable || world ? "regular" : "fill"} />}
            onClick={() => {
              onDone();
              openChat();
            }}
          >
            {t("chat.actions.message")}
          </Button>
        ) : null}
        <DiscordButton className="w-full" onClick={() => void openInDiscord(friend)}>
          {t("social.actions.message")}
        </DiscordButton>
        {team?.crews && team.state !== "member" ? (
          <Menu
            items={crewInviteItems(team.crews, t)}
            trigger={
              <Button
                variant="secondary"
                className="w-full"
                icon={<UsersThree size={16} />}
                disabled={team.state !== "invitable"}
              >
                {team.state === "invited" ? t("teams.invite.sent") : t("teams.invite.toTeam")}
              </Button>
            }
          />
        ) : team && team.state !== "member" ? (
          <Button
            variant="secondary"
            className="w-full"
            icon={<UsersThree size={16} />}
            disabled={team.state !== "invitable"}
            onClick={team.onInvite}
          >
            {team.state === "invited" ? t("teams.invite.sent") : t("teams.invite.toTeam")}
          </Button>
        ) : null}
      </div>

      {categories && categories.groups.length > 0 ? (
        <div className="mt-3 flex items-center gap-3">
          <span className="shrink-0 text-[12.5px] text-ash-400">{t("friendGroups.category")}</span>
          <Select
            className="h-8 min-w-0 flex-1"
            label={t("friendGroups.category")}
            value={categories.current}
            onValueChange={(group) => categories.onMove(group)}
            options={[
              ...categories.groups.map((g) => ({ value: g.id, label: g.name })),
              { value: REST, label: t("friendGroups.rest") },
            ]}
          />
        </div>
      ) : null}

      <div className="mt-3 border-t border-white/8 pt-2">
        <AnimatePresence mode="popLayout" initial={false}>
          {confirm ? (
            <motion.div key={confirm} className="pt-1" {...rise(reduce, 4, 0.2)}>
              <p className="text-[13px] leading-snug text-bone-100">
                {confirm === "remove"
                  ? t("social.actions.removeAsk", { name: friend.name })
                  : t("social.actions.blockAsk")}
              </p>
              <div className="mt-2.5 flex justify-end gap-2">
                <Button size="sm" variant="ghost" onClick={() => setConfirm(null)}>
                  {t("social.actions.cancel")}
                </Button>
                <Button
                  size="sm"
                  variant="danger"
                  disabled={busy}
                  onClick={() => void run(confirm)}
                >
                  {confirm === "remove" ? t("social.actions.remove") : t("social.actions.block")}
                </Button>
              </div>
            </motion.div>
          ) : (
            <motion.div key="tools" className="flex items-center gap-1" {...rise(reduce, 4, 0.2)}>
              <button
                type="button"
                aria-pressed={favorite}
                onClick={() => toggleFavorite(friend.id)}
                className="flex h-9 cursor-pointer items-center gap-2 rounded-md px-2 text-[13px] text-ash-300 outline-none transition-colors hover:bg-white/6 hover:text-bone-100 focus-visible:ring-2 focus-visible:ring-gold-300"
              >
                <motion.span
                  key={String(favorite)}
                  initial={
                    reduce ? { opacity: 0 } : { opacity: 0, transform: "scale(0.7) rotate(-20deg)" }
                  }
                  animate={{
                    opacity: 1,
                    transform: "scale(1) rotate(0deg)",
                    transition: { type: "spring", stiffness: 520, damping: 22 },
                  }}
                  className="flex"
                >
                  <Star
                    size={16}
                    weight={favorite ? "fill" : "regular"}
                    className={favorite ? "text-gold-300" : undefined}
                  />
                </motion.span>
                <span className="relative inline-flex">
                  <AnimatePresence mode="popLayout" initial={false}>
                    <motion.span key={String(favorite)} {...swap(reduce)}>
                      {favorite ? t("social.actions.unfavorite") : t("social.actions.favorite")}
                    </motion.span>
                  </AnimatePresence>
                </span>
              </button>
              <IconButton
                label={t("friendMenus.notifyOnline")}
                aria-pressed={notify}
                size="sm"
                className="ml-auto"
                onClick={() => setOnlineAlert(friend.id, !notify)}
                icon={
                  <motion.span
                    key={String(notify)}
                    initial={
                      reduce
                        ? { opacity: 0 }
                        : { opacity: 0, transform: "scale(0.7) rotate(-16deg)" }
                    }
                    animate={{
                      opacity: 1,
                      transform: "scale(1) rotate(0deg)",
                      transition: { type: "spring", stiffness: 520, damping: 22 },
                    }}
                    className="flex"
                  >
                    <Bell
                      size={16}
                      weight={notify ? "fill" : "regular"}
                      className={notify ? "text-gold-300" : undefined}
                    />
                  </motion.span>
                }
              />
              <MoreMenu
                onPick={(action) => {
                  if (action === "report") {
                    onDone();
                    reportFriend(friend);
                  } else {
                    setConfirm(action);
                  }
                }}
              />
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}

function MoreMenu({ onPick }: { onPick: (action: "remove" | "block" | "report") => void }) {
  const { t } = useTranslation();
  const calm = useCalm();
  const [open, setOpen] = useState(false);
  return (
    <DropdownMenu.Root open={open} onOpenChange={setOpen} modal={false}>
      <DropdownMenu.Trigger asChild>
        <IconButton
          label={t("social.actions.more")}
          icon={<DotsThree size={20} weight="bold" />}
          size="sm"
        />
      </DropdownMenu.Trigger>
      <AnimatePresence>
        {open ? (
          <DropdownMenu.Portal forceMount>
            <DropdownMenu.Content
              onPointerDownOutside={swallowDismissClick}
              onCloseAutoFocus={returnFocusAfterKeys}
              forceMount
              asChild
              align="end"
              sideOffset={4}
              collisionPadding={12}
              loop
            >
              <motion.div
                className="z-[60] w-[220px] rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-1.5 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
                style={{ transformOrigin: "var(--radix-dropdown-menu-content-transform-origin)" }}
                initial={calm({ opacity: 0, scale: 0.96, y: -4 })}
                animate={calm({
                  opacity: 1,
                  scale: 1,
                  y: 0,
                  transition: { duration: 0.18, ease: easeOutExpo },
                })}
                exit={calm({ opacity: 0, scale: 0.98, transition: { duration: 0.12 } })}
              >
                <DropdownMenu.Item
                  onSelect={() => onPick("remove")}
                  className={cn(menuItemClass, "text-bone-100")}
                >
                  <UserMinus size={16} className="text-ash-300" aria-hidden />
                  {t("social.actions.remove")}
                </DropdownMenu.Item>
                <DropdownMenu.Item
                  onSelect={() => onPick("block")}
                  className={cn(menuItemClass, "text-flare-400")}
                >
                  <Prohibit size={16} aria-hidden />
                  {t("social.actions.block")}
                </DropdownMenu.Item>
                <DropdownMenu.Item
                  onSelect={() => onPick("report")}
                  className={cn(menuItemClass, "text-bone-100")}
                >
                  <Flag size={16} className="text-ash-300" aria-hidden />
                  {t("reports.action")}
                </DropdownMenu.Item>
              </motion.div>
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        ) : null}
      </AnimatePresence>
    </DropdownMenu.Root>
  );
}

export function FriendRow({
  friend,
  favorite,
  onKeyDown,
  categories,
  team,
  onPress,
}: {
  friend: Friend;
  favorite: boolean;
  onKeyDown?: (event: KeyboardEvent<HTMLButtonElement>) => void;
  /** Categories mode: the friend's category and the others (null: not offered). */
  categories?: RowCategories | null;
  /** The player's team, for "Takıma davet et" (null: no team, or teams off). */
  team?: RowTeam | null;
  /** A press that may become a drag into another category. */
  onPress?: (event: ReactPointerEvent<HTMLElement>) => void;
}) {
  const { t } = useTranslation();
  const calm = useCalm();
  const { reduce } = useSocialMotion();
  const [open, setOpen] = useState(false);
  const [intent, setIntent] = useState<Confirm | null>(null);
  const world = useWorldInvite(friend);
  const joinable = friend.state === "playing" && isJoinable(friend.game);
  const line = world ? t("host.invite.inWorld") : presenceLine(friend, t);
  const badge = useShownBadge(friend.badge);

  // Remove and block ask first, in the card: it opens at the question once the menu is gone.
  const openAt = (question: Confirm) =>
    window.setTimeout(() => {
      setIntent(question);
      setOpen(true);
    }, 0);

  // The friend's menu, the same wherever they show (FIX2): a right click, Shift+F10 or the menu
  // key, or "…" away on the row.
  const items = usePersonItems(friend, {
    categories,
    from: "list",
    ask: (question) => openAt(question === "block" ? "block" : "remove"),
  });

  return (
    <ContextMenu items={items}>
      <div
        onPointerDown={onPress}
        className="group/friend flex items-center gap-2 rounded-[var(--radius-panel)] pr-2 transition-colors duration-150 hover:bg-white/[0.045] data-[state=open]:bg-white/[0.06] has-[[data-state=open]]:bg-white/[0.06]"
      >
        <Popover.Root
          open={open}
          onOpenChange={(next) => {
            setOpen(next);
            if (!next) setIntent(null);
          }}
        >
          <Popover.Trigger asChild>
            <button
              type="button"
              data-friend-row
              onKeyDown={(event) => {
                if (!openMenuFromKey(event)) onKeyDown?.(event);
              }}
              // A double click opens the card and leaves it open (its second click would close it).
              onClick={(event) => {
                if (event.detail > 1 && open) event.preventDefault();
              }}
              aria-label={`${badgeRowLabel(friend.name, badge, t, friend.mark)}, ${line}`}
              className="flex min-w-0 flex-1 cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] py-2 pl-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
            >
              <SocialAvatar source={friend} size={36} state={friend.state} dnd={friend.dnd} />
              <span className="min-w-0 flex-1">
                <BadgedName
                  name={friend.name}
                  badge={friend.badge}
                  wearer={friend.id}
                  mark={friend.mark}
                  dim={friend.state === "offline"}
                  nameClassName={cn(
                    "text-[14px] font-semibold",
                    friend.state === "offline" ? "text-bone-100/70" : "text-bone-50",
                  )}
                >
                  {favorite ? (
                    <Star
                      size={12}
                      weight="fill"
                      className="shrink-0 text-gold-300"
                      aria-label={t("social.list.favorite")}
                    />
                  ) : null}
                </BadgedName>
                <span className="relative block h-[18px] overflow-hidden">
                  <AnimatePresence mode="popLayout" initial={false}>
                    <motion.span
                      key={line}
                      className={cn(
                        "absolute inset-x-0 truncate text-[12.5px] leading-[18px]",
                        joinable || world ? "text-jade-400" : "text-ash-400",
                      )}
                      {...swap(reduce)}
                    >
                      {world ? (
                        <WorldLine invite={world} />
                      ) : (
                        <PresenceText friend={friend} t={t} joinable={joinable} />
                      )}
                    </motion.span>
                  </AnimatePresence>
                </span>
              </span>
            </button>
          </Popover.Trigger>
          <AnimatePresence>
            {open ? (
              <Popover.Portal forceMount>
                <Popover.Content
                  onPointerDownOutside={swallowDismissClick}
                  forceMount
                  asChild
                  side="left"
                  align="start"
                  sideOffset={14}
                  collisionPadding={16}
                  aria-label={friend.name}
                  onOpenAutoFocus={focusFirstAction}
                >
                  <motion.div
                    className="z-50 rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-4 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_30px_80px_rgb(34_6_6/0.6)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
                    style={{ transformOrigin: "var(--radix-popover-content-transform-origin)" }}
                    initial={calm({ opacity: 0, scale: 0.96, x: 8 })}
                    animate={calm({
                      opacity: 1,
                      scale: 1,
                      x: 0,
                      transition: { duration: 0.22, ease: easeOutExpo },
                    })}
                    exit={calm({ opacity: 0, scale: 0.98, transition: { duration: 0.14 } })}
                  >
                    <FriendCard
                      // A question asked from the menu while the card is open starts it afresh.
                      key={intent ?? "card"}
                      friend={friend}
                      initialConfirm={intent}
                      categories={categories}
                      team={team}
                      onDone={() => {
                        setOpen(false);
                        setIntent(null);
                      }}
                    />
                  </motion.div>
                </Popover.Content>
              </Popover.Portal>
            ) : null}
          </AnimatePresence>
        </Popover.Root>
        <AnimatePresence initial={false}>
          {world ? (
            <motion.span key="world" className="flex" data-no-drag {...rise(reduce, 0, 0.2)}>
              <WorldJoinButton invite={world} />
            </motion.span>
          ) : joinable ? (
            <motion.span key="join" className="flex" data-no-drag {...rise(reduce, 0, 0.2)}>
              <JoinButton />
            </motion.span>
          ) : null}
        </AnimatePresence>
        <span className="flex" data-no-drag>
          <Menu
            items={items}
            trigger={
              <IconButton
                label={t("friendMenus.options", { name: friend.name })}
                icon={<DotsThree size={18} weight="bold" />}
                size="sm"
                className="opacity-0 transition-opacity duration-150 group-hover/friend:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
              />
            }
          />
        </span>
      </div>
    </ContextMenu>
  );
}
