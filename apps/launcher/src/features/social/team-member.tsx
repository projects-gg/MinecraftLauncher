// One crewmate: picture with presence, name (a crown for the leader, a starred shield for a
// moderator), where they are when the group above does not say it, and "Katıl" when they are on
// projects.gg. The row opens their card beside the panel: how long they have been in the crew, join,
// "Mesaj" (a friend, chat on), "Arkadaş ekle" (a crewmate who is not a friend, FIX2),
// "Discord'dan mesaj at", and in a quieter corner what the player may do about them (the leader
// names moderators, hands on the leadership and removes anyone; a moderator removes plain members
// where the crew lets them; anyone may block). Irreversible steps are confirmed in place with one
// sentence; naming a moderator is not (it is undone the same way). The row's menu (a right click,
// Shift+F10 or the menu key, or its "…") is the person menu every place shows (person-menu.tsx).
import {
  ChatCircle,
  Crown,
  DotsThree,
  Flag,
  Prohibit,
  ShieldStar,
  UserCheck,
  UserMinus,
  UserPlus,
} from "@phosphor-icons/react";
import {
  Button,
  ContextMenu,
  cn,
  easeOutExpo,
  IconButton,
  Menu,
  returnFocusAfterKeys,
  toast,
} from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { DropdownMenu, Popover } from "radix-ui";
import { type KeyboardEvent, type ReactElement, useState } from "react";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { SocialTeam, TeamMember } from "@/lib/api/social-types";
import { attempt } from "./actions";
import { directConversation } from "./chat/conversations";
import { BadgedName, badgeRowLabel, focusFirstAction, useShownBadge } from "./crew-badge";
import { DiscordButton } from "./discord-button";
import { openInDiscord } from "./discord-link";
import { JoinButton } from "./friend-row";
import { openMenuFromKey } from "./menu-key";
import { type PersonAction, personMenu } from "./menus";
import { isJoinable, presenceLine } from "./model";
import { rise, swap, useSocialMotion } from "./motion";
import { type PersonQuestion, usePersonItems } from "./person-menu";
import { SocialAvatar } from "./social-face";
import { useSocialUi } from "./store";
import { afterHandingOn, canRemove, daysSince, MAX_MODERATORS } from "./teams";
import { togetherTime } from "./together";

type Question = "founder" | "remove" | "removeFriend" | "block" | "unblock";

/** The card's question for the person menu's. */
const CARD_QUESTION: Record<PersonQuestion, Question> = {
  makeFounder: "founder",
  removeMember: "remove",
  removeFriend: "removeFriend",
  block: "block",
};

/** A crown for the leader, a starred shield for a moderator (gold, quieter). */
export function RoleBadge({
  member,
  size = 12,
}: {
  member: Pick<TeamMember, "role" | "moderator">;
  size?: number;
}) {
  const { t } = useTranslation();
  if (member.role === "founder") {
    return (
      <Crown
        size={size}
        weight="fill"
        className="shrink-0 text-gold-300"
        aria-label={t("teams.founder")}
      />
    );
  }
  if (!member.moderator) return null;
  return (
    <ShieldStar
      size={size + 1}
      weight="fill"
      className="shrink-0 text-gold-300/75"
      aria-label={t("teams.moderator")}
    />
  );
}

/** The crew's rows of the person menu. */
const CREW_ROWS = new Set<PersonAction>([
  "makeModerator",
  "unmakeModerator",
  "makeFounder",
  "removeMember",
]);

/** The card's quieter corner: the crew's and the relationship's rows of the person menu. */
const CARD_ACTIONS = new Set<PersonAction>([
  "makeModerator",
  "unmakeModerator",
  "makeFounder",
  "removeMember",
  "removeFriend",
  "block",
  "unblock",
  "report",
]);

function cardPlan(team: SocialTeam, member: TeamMember, roles: boolean) {
  return personMenu({
    friend: member.friend,
    blocked: member.blocked,
    report: !member.me,
    crew: member.me
      ? undefined
      : {
          roles,
          leader: team.role === "founder",
          moderator: member.moderator,
          moderatorsFull: team.members.filter((m) => m.moderator).length >= MAX_MODERATORS,
          removable: canRemove(team, member, roles),
        },
  }).filter((row): row is NonNullable<typeof row> => row !== null && CARD_ACTIONS.has(row.action));
}

/** "Mesaj" to a crewmate who is a friend (chat on), else nothing to offer. */
function useMemberChat(member: TeamMember): (() => void) | null {
  const chat = useSocialUi((s) => Boolean(s.view.chat));
  const me = useSocialUi((s) => s.view.me?.id);
  const openChat = useSocialUi((s) => s.openChat);
  if (!chat || !me || !member.friend || member.blocked) return null;
  return () => openChat(directConversation(me, member.id), { from: "team" });
}

/**
 * "Arkadaş ekle" to a crewmate who is not a friend (FIX2; the service lets crewmates add each other
 * without a code): may be sent, was sent and waits, or they asked first.
 */
function useAddFriend(member: TeamMember): "ready" | "sent" | "received" | null {
  return useSocialUi((s) => {
    if (member.friend || member.blocked || member.me || !s.view.crew.crewFriendRequests) {
      return null;
    }
    if (s.view.outgoing.some((r) => r.id === member.id)) return "sent";
    if (s.view.incoming.some((r) => r.id === member.id)) return "received";
    return "ready";
  });
}

/** "Rapor et" about a crewmate. */
function reportMember(member: TeamMember) {
  useSocialUi.getState().openReport({
    target: { kind: "player", id: member.id },
    name: member.name,
    person: member,
  });
}

/** Naming or un-naming a moderator: no question, the badge says it in place. */
async function setModerator(
  team: SocialTeam,
  member: TeamMember,
  moderator: boolean,
  said: string,
) {
  const done = await attempt(() => api.social.setModerator(team.id, member.id, moderator));
  if (done !== undefined) announce(said);
}

function MemberCard({
  member,
  team,
  initialQuestion,
  onDone,
}: {
  member: TeamMember;
  team: SocialTeam;
  /** Opened from the row's menu: the card starts at that question. */
  initialQuestion: Question | null;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const calm = useCalm();
  const roles = useSocialUi((s) => s.view.crew.roles);
  const [question, setQuestion] = useState<Question | null>(initialQuestion);
  const [menu, setMenu] = useState(false);
  const [busy, setBusy] = useState(false);
  const [asking, setAsking] = useState(false);
  // A friend's coarse "last seen" lives on the friend list.
  const friend = useSocialUi((s) =>
    member.friend ? s.view.friends.find((f) => f.id === member.id) : undefined,
  );
  const joinable = member.state === "playing" && isJoinable(member.game);
  const days = daysSince(member.joinedAt, Date.now());
  // "Bu ay birlikte 3 sa" (TEAM5): kept on this computer only, off unless the player keeps it.
  const together = togetherTime(member.togetherMonth ?? 0);
  const chat = useMemberChat(member);
  const addFriend = useAddFriend(member);
  const plan = cardPlan(team, member, roles);

  const run = async (action: Question) => {
    setBusy(true);
    const done = await attempt(() => {
      switch (action) {
        case "founder":
          return api.social.transferTeam(team.id, member.id);
        case "remove":
          return api.social.removeTeamMember(team.id, member.id);
        case "removeFriend":
          return api.social.removeFriend(member.id);
        case "block":
          return api.social.block(member.id);
        case "unblock":
          return api.social.unblock(member.id);
      }
    });
    setBusy(false);
    if (done === undefined) return;
    const said = {
      founder: t("teams.member.madeFounder", { name: member.name }),
      remove: t("teams.member.removed", { name: member.name }),
      removeFriend: t("social.actions.removed", { name: member.name }),
      block: t("social.actions.blocked", { name: member.name }),
      unblock: t("social.actions.unblocked", { name: member.name }),
    }[action];
    announce(said);
    onDone();
  };

  /** "Arkadaş ekle" or "İsteği kabul et" (FIX2): sent at once, said in place and aloud. */
  const befriend = async () => {
    setAsking(true);
    const outcome =
      addFriend === "received"
        ? await attempt(() => api.social.accept(member.id)).then((done) =>
            done === undefined ? undefined : ("friends" as const),
          )
        : await attempt(() => api.social.requestCrewmate(member.id));
    setAsking(false);
    if (outcome === undefined) return;
    const said =
      outcome === "friends"
        ? t("social.actions.accepted", { name: member.name })
        : t("fix2.menu.requestSentTo");
    toast.success(said);
    announce(said);
  };

  const asks: Record<Question, string> = {
    founder:
      afterHandingOn(team, member.id, roles) === "moderator"
        ? t("teams.member.makeFounderAskModerator", { name: member.name })
        : t("teams.member.makeFounderAsk", { name: member.name }),
    remove: t("teams.member.removeAsk", { name: member.name }),
    removeFriend: t("social.actions.removeAsk", { name: member.name }),
    block: t("social.actions.blockAsk"),
    unblock: t("social.actions.unblock"),
  };
  const labels: Record<Question, string> = {
    founder: t("teams.member.makeFounder"),
    remove: t("teams.member.remove"),
    removeFriend: t("social.actions.remove"),
    block: t("social.actions.block"),
    unblock: t("social.actions.unblock"),
  };

  const select = (action: PersonAction) => {
    switch (action) {
      case "makeModerator":
        void setModerator(
          team,
          member,
          true,
          t("teams.member.madeModerator", { name: member.name }),
        );
        return;
      case "unmakeModerator":
        void setModerator(
          team,
          member,
          false,
          t("teams.member.unmadeModerator", { name: member.name }),
        );
        return;
      case "makeFounder":
        setQuestion("founder");
        return;
      case "removeMember":
        setQuestion("remove");
        return;
      case "removeFriend":
        setQuestion("removeFriend");
        return;
      case "block":
        setQuestion("block");
        return;
      case "unblock":
        void run("unblock");
        return;
      case "report":
        onDone();
        reportMember(member);
        return;
      default:
        return;
    }
  };

  const itemClass =
    "flex h-10 cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-[13.5px] outline-none select-none data-[highlighted]:bg-white/8 data-[disabled]:cursor-default data-[disabled]:opacity-45";
  const words: Partial<Record<PersonAction, string>> = {
    makeModerator: t("teams.member.makeModerator"),
    unmakeModerator: t("teams.member.unmakeModerator"),
    makeFounder: t("teams.member.makeFounder"),
    removeMember: t("teams.member.remove"),
    removeFriend: t("social.actions.remove"),
    block: t("social.actions.block"),
    unblock: t("social.actions.unblock"),
    report: t("reports.action"),
  };
  const icons: Partial<Record<PersonAction, ReactElement>> = {
    makeModerator: <ShieldStar size={16} className="text-ash-300" aria-hidden />,
    unmakeModerator: <ShieldStar size={16} className="text-ash-300" aria-hidden />,
    makeFounder: <Crown size={16} className="text-ash-300" aria-hidden />,
    removeMember: <UserMinus size={16} className="text-ash-300" aria-hidden />,
    removeFriend: <UserMinus size={16} className="text-ash-300" aria-hidden />,
    block: <Prohibit size={16} aria-hidden />,
    unblock: <Prohibit size={16} aria-hidden />,
    report: <Flag size={16} className="text-ash-300" aria-hidden />,
  };

  return (
    <div className="w-[300px]">
      <div className="flex items-center gap-3.5">
        <SocialAvatar
          source={member}
          size={56}
          state={member.state}
          dnd={member.dnd}
          surface="var(--color-obsidian-900)"
        />
        <div className="min-w-0">
          {/* The tag they wear before the name, a button of its own here (TEAM6). */}
          <BadgedName
            name={member.name}
            badge={member.badge}
            wearer={member.id}
            size="lg"
            chip="button"
            className="gap-2"
            nameClassName="font-display text-[18px] leading-tight font-semibold text-bone-50"
          >
            <RoleBadge member={member} size={15} />
          </BadgedName>
          <p className="truncate text-[12.5px] text-ash-400" data-selectable>
            @{member.username}
          </p>
          <p
            className={cn(
              "mt-1 truncate text-[13px]",
              member.blocked
                ? "text-ash-400"
                : joinable
                  ? "text-jade-400"
                  : member.state === "offline"
                    ? "text-ash-400"
                    : "text-ash-300",
            )}
          >
            {member.blocked
              ? t("teams.member.blocked")
              : presenceLine(friend && member.state === "offline" ? friend : member, t)}
          </p>
        </div>
      </div>
      <p className="mt-3 text-[12.5px] text-ash-400">
        {days >= 1 ? t("teams.member.since", { count: days }) : t("teams.member.sinceToday")}
      </p>
      {together ? (
        <p className="mt-0.5 text-[12.5px] text-ash-400">
          {t("crewWave2.together.month", { time: together })}
        </p>
      ) : null}

      <div className="mt-3 flex flex-col gap-2">
        {joinable && !member.blocked ? (
          <JoinButton size="md" variant="primary" className="w-full" />
        ) : null}
        {chat ? (
          <Button
            variant={joinable && !member.blocked ? "secondary" : "primary"}
            className="w-full"
            icon={<ChatCircle size={16} weight={joinable ? "regular" : "fill"} />}
            onClick={() => {
              onDone();
              chat();
            }}
          >
            {t("chat.actions.message")}
          </Button>
        ) : null}
        {addFriend ? (
          <Button
            variant="secondary"
            className="w-full"
            icon={addFriend === "received" ? <UserCheck size={16} /> : <UserPlus size={16} />}
            disabled={addFriend === "sent" || asking}
            onClick={() => void befriend()}
          >
            {addFriend === "sent"
              ? t("fix2.menu.requestSent")
              : addFriend === "received"
                ? t("fix2.menu.acceptFriend")
                : t("fix2.menu.addFriend")}
          </Button>
        ) : null}
        <DiscordButton className="w-full" onClick={() => void openInDiscord(member)}>
          {t("social.actions.message")}
        </DiscordButton>
      </div>

      <div className="mt-3 border-t border-white/8 pt-2">
        <AnimatePresence mode="popLayout" initial={false}>
          {question ? (
            <motion.div key={question} className="pt-1" {...rise(reduce, 4, 0.2)}>
              <p className="text-[13px] leading-snug text-bone-100">{asks[question]}</p>
              <div className="mt-2.5 flex justify-end gap-2">
                <Button size="sm" variant="ghost" onClick={() => setQuestion(null)}>
                  {t("social.actions.cancel")}
                </Button>
                <Button
                  size="sm"
                  variant={
                    question === "founder" || question === "unblock" ? "secondary" : "danger"
                  }
                  disabled={busy}
                  onClick={() => void run(question)}
                >
                  {labels[question]}
                </Button>
              </div>
            </motion.div>
          ) : (
            <motion.div
              key="tools"
              className="flex items-center justify-between"
              {...rise(reduce, 4, 0.2)}
            >
              <span className="px-1 text-[12px] text-ash-400">
                {member.role === "founder"
                  ? t("teams.founder")
                  : member.moderator
                    ? t("teams.moderator")
                    : null}
              </span>
              <DropdownMenu.Root open={menu} onOpenChange={setMenu} modal={false}>
                <DropdownMenu.Trigger asChild>
                  <IconButton
                    label={t("social.actions.more")}
                    icon={<DotsThree size={20} weight="bold" />}
                    size="sm"
                  />
                </DropdownMenu.Trigger>
                <AnimatePresence>
                  {menu ? (
                    <DropdownMenu.Portal forceMount>
                      <DropdownMenu.Content
                        onCloseAutoFocus={returnFocusAfterKeys}
                        forceMount
                        asChild
                        align="end"
                        sideOffset={4}
                        collisionPadding={12}
                        loop
                      >
                        <motion.div
                          className="z-[60] w-[230px] rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-1.5 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
                          style={{
                            transformOrigin: "var(--radix-dropdown-menu-content-transform-origin)",
                          }}
                          initial={calm({ opacity: 0, scale: 0.96, y: -4 })}
                          animate={calm({
                            opacity: 1,
                            scale: 1,
                            y: 0,
                            transition: { duration: 0.18, ease: easeOutExpo },
                          })}
                          exit={calm({ opacity: 0, scale: 0.98, transition: { duration: 0.12 } })}
                        >
                          {plan.map((row, index) => (
                            <div key={row.action}>
                              {/* The crew's rows, then the relationship's, apart. */}
                              {index > 0 &&
                              !CREW_ROWS.has(row.action) &&
                              CREW_ROWS.has(plan[index - 1]?.action ?? "report") ? (
                                <DropdownMenu.Separator className="my-1.5 h-px bg-white/8" />
                              ) : null}
                              <DropdownMenu.Item
                                disabled={row.disabled}
                                onSelect={() => select(row.action)}
                                className={cn(
                                  itemClass,
                                  row.action === "block" ? "text-flare-400" : "text-bone-100",
                                )}
                              >
                                {icons[row.action]}
                                {words[row.action]}
                              </DropdownMenu.Item>
                            </div>
                          ))}
                        </motion.div>
                      </DropdownMenu.Content>
                    </DropdownMenu.Portal>
                  ) : null}
                </AnimatePresence>
              </DropdownMenu.Root>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}

/**
 * `line`: "presence" (the friend list: where they are, always) or "where" (the crew page, whose
 * group heading says it already: only another server, singleplayer or a friend's "last seen").
 */
export function TeamMemberRow({
  member,
  team,
  line = "presence",
  onKeyDown,
}: {
  member: TeamMember;
  team: SocialTeam;
  line?: "presence" | "where";
  onKeyDown?: (event: KeyboardEvent<HTMLButtonElement>) => void;
}) {
  const { t } = useTranslation();
  const calm = useCalm();
  const { reduce } = useSocialMotion();
  const [open, setOpen] = useState(false);
  const [intent, setIntent] = useState<Question | null>(null);
  // A friend's coarse "last seen" lives on the friend list (crewmates who are not friends never
  // share it).
  const friend = useSocialUi((s) =>
    member.friend ? s.view.friends.find((f) => f.id === member.id) : undefined,
  );
  const joinable = member.state === "playing" && isJoinable(member.game) && !member.blocked;
  const presence = member.blocked
    ? t("teams.member.blocked")
    : presenceLine(friend && member.state === "offline" ? friend : member, t);
  const badge = useShownBadge(member.badge);
  const shown =
    line === "presence" ||
    member.blocked ||
    (member.state === "playing" && !joinable) ||
    (member.state === "offline" && Boolean(friend?.lastSeen));
  const sub = shown ? presence : null;

  // Questions are asked in the card: it opens at the question once the menu is gone.
  const openAt = (question: Question) =>
    window.setTimeout(() => {
      setIntent(question);
      setOpen(true);
    }, 0);

  // The crewmate's menu, the same wherever they show (FIX2), with the crew's rows for this crew.
  const items = usePersonItems(member, {
    crew: team,
    from: "team",
    ask: (question) => openAt(CARD_QUESTION[question]),
  });

  return (
    <ContextMenu items={items} disabled={!items.length}>
      <div className="group/friend flex items-center gap-2 rounded-[var(--radius-panel)] pr-2 transition-colors duration-150 hover:bg-white/[0.045] has-[[data-state=open]]:bg-white/[0.06]">
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
              // A double click opens the card and leaves it open.
              onClick={(event) => {
                if (event.detail > 1 && open) event.preventDefault();
              }}
              aria-label={`${badgeRowLabel(member.name, badge, t)}, ${presence}`}
              className={cn(
                "flex min-w-0 flex-1 cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] pl-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
                line === "where" ? "py-1.5" : "py-2",
              )}
            >
              <SocialAvatar
                source={member}
                // The crew page keeps one size for everyone (most rows there have one line).
                size={line === "where" ? 32 : 36}
                state={member.blocked ? "offline" : member.state}
                dnd={member.dnd}
              />
              <span className="min-w-0 flex-1">
                <BadgedName
                  name={member.name}
                  badge={member.badge}
                  wearer={member.id}
                  dim={member.state === "offline" || member.blocked}
                  nameClassName={cn(
                    "text-[14px] font-semibold",
                    member.state === "offline" || member.blocked
                      ? "text-bone-100/70"
                      : "text-bone-50",
                  )}
                >
                  <RoleBadge member={member} />
                </BadgedName>
                {sub ? (
                  <span className="relative block h-[18px] overflow-hidden">
                    <AnimatePresence mode="popLayout" initial={false}>
                      <motion.span
                        key={sub}
                        className={cn(
                          "absolute inset-x-0 truncate text-[12.5px] leading-[18px]",
                          joinable ? "text-jade-400" : "text-ash-400",
                        )}
                        {...swap(reduce)}
                      >
                        {sub}
                      </motion.span>
                    </AnimatePresence>
                  </span>
                ) : null}
              </span>
            </button>
          </Popover.Trigger>
          <AnimatePresence>
            {open ? (
              <Popover.Portal forceMount>
                <Popover.Content
                  forceMount
                  asChild
                  side="left"
                  align="start"
                  sideOffset={14}
                  collisionPadding={16}
                  aria-label={member.name}
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
                    <MemberCard
                      // A question asked from the menu while the card is open starts it afresh.
                      key={intent ?? "card"}
                      member={member}
                      team={team}
                      initialQuestion={intent}
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
          {joinable ? (
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
                label={t("friendMenus.options", { name: member.name })}
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
