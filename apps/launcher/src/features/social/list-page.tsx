// The friend list: waiting invites (to play and to teams), requests, the player's team, then
// friends, and the requests the player sent. Friends are in game / in the launcher / offline, or,
// once the player made categories ("Okul", "PvP"), in those categories and "Arkadaşlar" for the
// rest, each in presence order. One list, so a friend who comes online (or moves to another
// category) glides to the new place instead of vanishing and reappearing; section headers glide
// with them and fade when their section empties. Friends and categories are dragged to move them;
// menus and Alt+↑/↓ do the same from the keyboard. With several crews (TEAM4) each crew is a
// section of its own, in the player's order; a call sits under the first crew it names.
import {
  ArrowClockwise,
  CaretDown,
  Check,
  Copy,
  Play,
  UserPlus,
  WifiSlash,
  X,
} from "@phosphor-icons/react";
import {
  Button,
  ContentIcon,
  ContextMenu,
  IconButton,
  type MenuItem,
  Progress,
  SearchInput,
  Skeleton,
  toast,
} from "@pl/ui";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";
import { type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { inviteKeys, inviteList } from "@/features/host/rules";
import { useHostUi } from "@/features/host/store";
import { WorldInviteRow } from "@/features/host/world-invite";
import { api } from "@/lib/api";
import type {
  CrewCall,
  Friend,
  FriendGroups,
  FriendRequest,
  SocialInvite,
  SocialTeam,
  TeamInvite,
  TeamMember,
} from "@/lib/api/social-types";
import { formatRelative } from "@/lib/format";
import { matchesQuery } from "@/lib/search";
import { attempt, copyText } from "./actions";
import { crewsOfCall, theirCalls } from "./calls";
import {
  CategoryHeader,
  DragGhost,
  DragSlot,
  EmptyCategory,
  NewCategoryRow,
} from "./category-header";
import { BadgedName } from "./crew-badge";
import { CallRow } from "./crew-call";
import { waitingRequests } from "./crew-joining";
import { JoinsRow } from "./crew-requests";
import { type InviteState, inviteTargets } from "./crews";
import { FriendRow, type RowTeam } from "./friend-row";
import {
  addGroup,
  groupOf,
  groupSections,
  MAX_GROUPS,
  moveFriend,
  newGroupId,
  pruneMembers,
  REST,
  removeGroup,
  renameGroup,
  reorderGroups,
  sameGroups,
  stepFriend,
  stepGroup,
} from "./groups";
import { canJoinInvite, joinInvite } from "./join-dialog";
import { openMenuFromKey } from "./menu-key";
import { groupFriends, presenceLine } from "./model";
import { enter, glide, leave, rise, swap, useSocialMotion } from "./motion";
import { NameWithMark } from "./person-mark";
import { PersonContextMenu, PersonMoreButton, usePeopleItems } from "./person-menu";
import { SocialAvatar } from "./social-face";
import { useSocialPrefs, useSocialUi } from "./store";
import { TeamMemberRow } from "./team-member";
import { TeamHeader, TeamInviteRow } from "./team-section";
import { teammates } from "./teams";
import { useCallClock } from "./use-call-clock";
import { useListDrag } from "./use-list-drag";

const ago = (unixSeconds: number) => formatRelative(new Date(unixSeconds * 1000).toISOString());

/** The team section's id among folded sections (category ids are 1-12 letters and digits); with
 * several crews each folds on its own, as "team:" and its id. */
const TEAM_SECTION = "team:";

const NO_CREWS: SocialTeam[] = [];

/** A crew's id among folded sections: "team:" with one crew per player, else with its id. */
function foldOf(crew: Pick<SocialTeam, "id">, multi: boolean): string {
  return multi ? `${TEAM_SECTION}${crew.id}` : TEAM_SECTION;
}

/** The best of a friend's states across the player's crews: one to invite to first. */
const INVITE_ORDER: readonly InviteState[] = ["invitable", "invited", "full", "member"];

function Header({ label, count }: { label: string; count: number }) {
  const { reduce } = useSocialMotion();
  return (
    <h3 className="flex items-center gap-2 px-2 pt-4 pb-1.5 text-[12.5px] font-medium text-ash-400">
      {label}
      <span className="relative inline-flex font-display tabular-nums text-ash-400">
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span key={count} {...swap(reduce, 0.18)}>
            {count}
          </motion.span>
        </AnimatePresence>
      </span>
    </h3>
  );
}

/** A banner for the connection, when it is not simply online. */
function ConnectionBanner() {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const connection = useSocialUi((s) => s.view.connection);
  const [busy, setBusy] = useState(false);
  const retry = async () => {
    setBusy(true);
    await attempt(() => api.social.refresh());
    // The backend throttles retries; a short pause keeps the button honest.
    setTimeout(() => setBusy(false), 1200);
  };
  const content: ReactNode =
    connection === "unavailable" ? (
      <div className="flex items-center gap-3 rounded-[var(--radius-panel)] bg-obsidian-950/45 px-3.5 py-3 ring-1 ring-white/8">
        <WifiSlash size={18} className="shrink-0 text-gold-300" aria-hidden />
        <p className="min-w-0 flex-1 text-[13px] text-bone-100">
          {t("social.service.unavailable")}
        </p>
        <Button
          size="sm"
          variant="ghost"
          disabled={busy}
          icon={<ArrowClockwise size={14} />}
          onClick={() => void retry()}
        >
          {t("social.service.retry")}
        </Button>
      </div>
    ) : connection === "replaced" ? (
      <div className="flex items-center gap-3 rounded-[var(--radius-panel)] bg-obsidian-950/45 px-3.5 py-3 ring-1 ring-white/8">
        <p className="min-w-0 flex-1 text-[13px] text-bone-100">{t("social.service.replaced")}</p>
        <Button size="sm" variant="secondary" disabled={busy} onClick={() => void retry()}>
          {t("social.service.reconnect")}
        </Button>
      </div>
    ) : connection === "reconnecting" || connection === "connecting" ? (
      <div className="px-2 pt-1" role="status">
        <p className="mb-1.5 text-[12px] text-ash-400">{t("social.service.reconnecting")}</p>
        <Progress label={t("social.service.reconnecting")} className="h-[3px]" />
      </div>
    ) : null;
  return (
    <AnimatePresence initial={false}>
      {content ? (
        <motion.div key={connection} className="mb-2" {...rise(reduce, 4)}>
          {content}
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

function RequestRow({ request }: { request: FriendRequest }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const act = async (action: "accept" | "decline") => {
    setBusy(true);
    const done = await attempt(() =>
      action === "accept" ? api.social.accept(request.id) : api.social.decline(request.id),
    );
    setBusy(false);
    if (done !== undefined && action === "accept") {
      announce(t("social.actions.accepted", { name: request.name }));
    }
  };
  // The request's answers lead the person menu (FIX2): the right click and "…" say the same.
  const place = { request: "incoming" as const, from: "list" as const };
  return (
    <PersonContextMenu person={request} place={place}>
      <div className="flex items-center gap-3 rounded-[var(--radius-panel)] py-2 pr-1.5 pl-2">
        <SocialAvatar source={request} size={36} />
        <span className="min-w-0 flex-1">
          <NameWithMark
            name={request.name}
            mark={request.mark}
            className="text-[14px] font-semibold text-bone-50"
          />
          <span className="block truncate text-[12.5px] text-ash-400">
            {t("social.list.wantsToAdd")}
          </span>
        </span>
        <Button
          size="sm"
          variant="secondary"
          icon={<Check size={14} weight="bold" />}
          disabled={busy}
          onClick={() => void act("accept")}
        >
          {t("social.actions.accept")}
        </Button>
        <IconButton
          label={t("social.actions.decline")}
          icon={<X size={16} />}
          size="sm"
          disabled={busy}
          onClick={() => void act("decline")}
        />
        <PersonMoreButton person={request} place={place} />
      </div>
    </PersonContextMenu>
  );
}

function SentRow({ request }: { request: FriendRequest }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  return (
    <PersonContextMenu person={request} place={{ request: "outgoing", from: "list" }}>
      <div className="flex items-center gap-3 rounded-[var(--radius-panel)] py-2 pr-1.5 pl-2">
        <SocialAvatar source={request} size={36} />
        <span className="min-w-0 flex-1">
          <NameWithMark
            name={request.name}
            mark={request.mark}
            className="text-[14px] font-semibold text-bone-100"
          />
          <span className="block truncate text-[12.5px] text-ash-400">
            {t("social.list.waiting")}
          </span>
        </span>
        <Button
          size="sm"
          variant="ghost"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            void attempt(() => api.social.cancelRequest(request.id)).then(() => setBusy(false));
          }}
          onKeyDown={(event) => {
            openMenuFromKey(event);
          }}
        >
          {t("social.actions.cancelRequest")}
        </Button>
      </div>
    </PersonContextMenu>
  );
}

/** An invite's "Katıl" is its sender's too. */
const SKIP_JOIN = ["join"] as const;

function InviteRow({ invite }: { invite: SocialInvite }) {
  const { t } = useTranslation();
  const name = invite.name ?? t("social.notices.someone");
  const joinable = canJoinInvite(invite);
  // "Katıl" and "Daveti kapat" first (the ×'s name), then who sent it with their rows, without a
  // second "Katıl" (it joins the same Projects; FIX2, SMALL10).
  const people = usePeopleItems(
    invite.name
      ? [{ id: invite.from, name, avatar: invite.avatar, avatarUrl: invite.avatarUrl }]
      : [],
    { from: "list", skip: SKIP_JOIN },
  );
  const items: Array<MenuItem | null> = [
    ...(joinable
      ? [
          {
            label: t("social.actions.join"),
            icon: <Play size={16} />,
            onSelect: () => joinInvite(invite),
          },
        ]
      : []),
    {
      label: t("small10.menu.dismissInvite"),
      icon: <X size={16} />,
      onSelect: () => void api.social.dismissInvite(invite.id),
    },
    ...(people.length ? [null, ...people] : []),
  ];
  return (
    <ContextMenu items={items}>
      <div className="mb-1.5 flex items-center gap-3 rounded-[var(--radius-panel)] bg-gold-500/[0.07] py-2 pr-1.5 pl-2 ring-1 ring-gold-400/15">
        <SocialAvatar
          source={{ name, avatar: invite.avatar, avatarUrl: invite.avatarUrl }}
          size={36}
        />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[14px] font-semibold text-bone-50">{name}</span>
          <span className="block truncate text-[12.5px] text-ash-300">
            {t("social.list.invitedYou")}, {ago(invite.at)}
          </span>
        </span>
        {joinable ? (
          <Button size="sm" variant="secondary" onClick={() => joinInvite(invite)}>
            {t("social.actions.join")}
          </Button>
        ) : null}
        <IconButton
          label={t("small10.menu.dismissInvite")}
          icon={<X size={16} />}
          size="sm"
          onClick={() => void api.social.dismissInvite(invite.id)}
          onKeyDown={(event) => {
            openMenuFromKey(event);
          }}
        />
      </div>
    </ContextMenu>
  );
}

function ListSkeleton() {
  return (
    <div aria-hidden className="space-y-1 pt-3">
      <Skeleton className="mx-2 mb-3 h-3.5 w-20" />
      {[0, 1, 2, 3, 4].map((i) => (
        <div
          key={i}
          className="flex items-center gap-3 px-2 py-2"
          style={{ opacity: 1 - i * 0.16 }}
        >
          <Skeleton className="size-9 rounded-[var(--radius-head)]" />
          <div className="flex-1 space-y-1.5">
            <Skeleton className="h-3.5 w-28" />
            <Skeleton className="h-3 w-20" />
          </div>
        </div>
      ))}
    </div>
  );
}

/** The player's own code, one click to copy: the first step to a friend list. */
function CodeChip({ code }: { code: string }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      aria-label={`${t("social.add.copy")}, ${code}`}
      onClick={() =>
        void copyText(code).then((ok) => {
          if (!ok) return;
          setCopied(true);
          announce(t("social.add.copied"));
          window.setTimeout(() => setCopied(false), 1600);
        })
      }
      className="group mt-4 inline-flex cursor-pointer items-center gap-2.5 rounded-md bg-obsidian-950/50 py-2 pr-2.5 pl-3 ring-1 ring-white/8 outline-none transition-[box-shadow,transform,scale] duration-150 hover:ring-white/16 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.98]"
    >
      <span className="text-[12px] text-ash-400">{t("social.list.yourCode")}</span>
      <span
        className="font-display text-[15px] font-semibold tracking-[0.05em] text-bone-50 tabular-nums"
        data-selectable
      >
        {code}
      </span>
      <span className="relative flex size-4 items-center justify-center">
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span key={String(copied)} className="flex" {...swap(reduce, 0.16)}>
            {copied ? (
              <Check size={15} weight="bold" className="text-jade-400" aria-hidden />
            ) : (
              <Copy size={15} className="text-ash-300 group-hover:text-bone-50" aria-hidden />
            )}
          </motion.span>
        </AnimatePresence>
      </span>
    </button>
  );
}

function EmptyFriends() {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const goTo = useSocialUi((s) => s.goTo);
  const code = useSocialUi((s) => s.view.me?.friendCode);
  return (
    <motion.div
      className="flex flex-col items-center px-6 pt-12 pb-6 text-center"
      {...rise(reduce, 8, 0.32)}
    >
      <span className="flex size-16 items-center justify-center rounded-[var(--radius-head)] bg-gold-700/25 ring-1 ring-gold-400/25">
        <ContentIcon kind="friends" />
      </span>
      <h3 className="mt-4 font-display text-[19px] font-semibold text-bone-50">
        {t("social.list.emptyTitle")}
      </h3>
      <p className="mt-1.5 max-w-[30ch] text-[13.5px] text-ash-400">{t("social.list.emptyBody")}</p>
      {code ? <CodeChip code={code} /> : null}
      <Button
        className="mt-5"
        variant="primary"
        icon={<UserPlus size={16} />}
        onClick={() => goTo("add")}
      >
        {t("social.actions.add")}
      </Button>
    </motion.div>
  );
}

/** How a dragged friend looks while lifted: their row, without the buttons. */
function FriendGhost({ friend }: { friend: Friend }) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-3 py-2 pr-3 pl-2">
      <SocialAvatar source={friend} size={36} state={friend.state} dnd={friend.dnd} />
      <span className="min-w-0 flex-1">
        <BadgedName
          name={friend.name}
          badge={friend.badge}
          wearer={friend.id}
          mark={friend.mark}
          chip="still"
          nameClassName="text-[14px] font-semibold text-bone-50"
        />
        <span className="block truncate text-[12.5px] leading-[18px] text-ash-400">
          {presenceLine(friend, t)}
        </span>
      </span>
    </div>
  );
}

/** How a lifted category looks: its header, with the faces of the friends it carries. */
function GroupGhost({ name, friends }: { name: string; friends: Friend[] }) {
  const online = friends.filter((f) => f.state !== "offline").length;
  return (
    <div className="flex items-center gap-1.5 px-3 py-2">
      <CaretDown
        size={12}
        weight="bold"
        aria-hidden
        className="shrink-0 -rotate-90 text-gold-300"
      />
      <span className="min-w-0 truncate text-[12.5px] font-semibold text-bone-50">{name}</span>
      <span className="font-display text-[12px] tabular-nums">
        <span className={online > 0 ? "text-jade-400" : "text-ash-400"}>{online}</span>
        <span className="text-ash-400">/{friends.length}</span>
      </span>
      <span className="ml-auto flex -space-x-1.5">
        {friends.slice(0, 4).map((friend) => (
          <SocialAvatar key={friend.id} source={friend} size={20} />
        ))}
      </span>
    </div>
  );
}

type Item =
  | { key: string; kind: "header"; label: string; count: number }
  | { key: string; kind: "friend"; friend: Friend; group: string | null }
  | { key: string; kind: "request"; request: FriendRequest }
  | { key: string; kind: "sent"; request: FriendRequest }
  | { key: string; kind: "invite"; invite: SocialInvite }
  | { key: string; kind: "worldInvite"; session: string }
  | { key: string; kind: "teamInvite"; invite: TeamInvite }
  | { key: string; kind: "teamHeader"; team: SocialTeam; folded: boolean }
  | { key: string; kind: "call"; call: CrewCall; team: SocialTeam }
  | { key: string; kind: "joins"; team: SocialTeam; count: number }
  | { key: string; kind: "teammate"; member: TeamMember; team: SocialTeam }
  | {
      key: string;
      kind: "group";
      id: string;
      name: string | null;
      online: number;
      count: number;
      folded: boolean;
      index: number;
      total: number;
    }
  | { key: string; kind: "groupEmpty"; id: string }
  | { key: string; kind: "slot"; height: number; folded: boolean; origin?: boolean }
  | { key: string; kind: "newGroup" };

/** From this many friends the list gets a search field (a short list reads at a glance). */
const SEARCH_FROM = 8;

export function ListPage() {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const view = useSocialUi((s) => s.view);
  // Friends' world invites (HOST1) wait with the other invites.
  const worldKeys = useHostUi((s) => (s.view.enabled ? inviteKeys(s.view.invites) : ""));
  const worldInvites = useMemo(() => inviteList(worldKeys), [worldKeys]);
  const favorites = useSocialPrefs((s) => s.favorites);
  const collapsed = useSocialPrefs((s) => s.collapsed);
  const toggleCollapsed = useSocialPrefs((s) => s.toggleCollapsed);
  const openTeam = useSocialUi((s) => s.openTeam);
  const listRef = useRef<HTMLUListElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const searchable = view.friends.length >= SEARCH_FROM;
  const q = searchable ? query.trim() : "";
  // Typing filters at once (no fades or glides per key); the render that clears the search
  // brings the rows back the same way, so the whole list does not fade in again.
  const searching = useRef(false);
  const quiet = Boolean(q) || searching.current;
  useEffect(() => {
    searching.current = Boolean(q);
  });

  // The categories: what the service last said, or what was just changed here (shown at once,
  // until the snapshot catches up).
  const [pending, setPending] = useState<FriendGroups | null>(null);
  const doc = pending ?? view.groups ?? null;
  useEffect(() => {
    if (pending && view.groups && sameGroups(pending, view.groups)) setPending(null);
  }, [pending, view.groups]);
  const save = (next: FriendGroups) => {
    const clean = pruneMembers(next, view.friends);
    setPending(clean);
    void api.social.saveGroups(clean.groups, clean.members).catch(() => {
      setPending(null);
      toast.error(t("friendGroups.failed"));
    });
  };
  /** A new category's name is asked here (the panel's and the headers' menus too); `friend` goes
   * into it once named. */
  const creating = useSocialUi((s) => s.categoryDraft);
  const setCreating = useSocialUi((s) => s.setCategoryDraft);
  // The name field sits under the whole list: a search would hide it.
  useEffect(() => {
    if (creating) setQuery("");
  }, [creating]);

  const categoriesOn = doc !== null;
  const grouped = Boolean(doc && doc.groups.length > 0) && !q;

  const nameOf = (group: string) =>
    group === REST
      ? t("friendGroups.rest")
      : (doc?.groups.find((g) => g.id === group)?.name ?? t("friendGroups.rest"));
  const friendName = (id: string) => view.friends.find((f) => f.id === id)?.name ?? "";

  const moveTo = (friendId: string, group: string) => {
    if (!doc || groupOf(doc, friendId) === group) return;
    save(moveFriend(doc, friendId, group));
    announce(t("friendGroups.moved", { name: friendName(friendId), group: nameOf(group) }));
  };

  const { drag, press, ghost } = useListDrag({
    listRef,
    reduce,
    onDropFriend: moveTo,
    onDropGroup: (id, order) => {
      if (!doc) return;
      save(reorderGroups(doc, order));
      announce(t("friendGroups.groupMoved", { name: nameOf(id), position: order.indexOf(id) + 1 }));
    },
  });

  const crews = view.teams ? view.crews : NO_CREWS;
  const multi = view.crew.multi;
  // Teammates' calls sit under the crew's header (the player's own is on the crew page).
  const now = useCallClock((s) => s.now);
  const calls = useMemo(
    () => (crews.length ? theirCalls(view.calls, now) : []),
    [crews, view.calls, now],
  );
  const teamRowFor = (friendId: string): RowTeam | null => {
    // Only crews that let the player invite (everyone on a service without roles).
    const targets = inviteTargets(crews, friendId, view.crew);
    if (!targets.length) return null;
    const invite = (crew: SocialTeam) => () => {
      void attempt(() => api.social.inviteToTeam(crew.id, friendId)).then((done) => {
        if (done !== undefined) {
          announce(t("teams.invite.sent"));
          toast.success(t("teams.invite.sent"));
        }
      });
    };
    const best = [...targets].sort(
      (a, b) => INVITE_ORDER.indexOf(a.state) - INVITE_ORDER.indexOf(b.state),
    )[0];
    if (!best) return null;
    return {
      state: best.state,
      onInvite: invite(best.crew),
      crews:
        multi && targets.length > 1
          ? targets.map(({ crew, state }) => ({ crew, state, onInvite: invite(crew) }))
          : undefined,
    };
  };

  const items = useMemo<Item[]>(() => {
    const list: Item[] = [];
    const hit = (name: string | undefined, username?: string) =>
      matchesQuery(name ?? "", q) || (username ? matchesQuery(username, q) : false);
    const section = (key: string, label: string, entries: Item[]) => {
      if (!entries.length) return;
      list.push({ key: `h:${key}`, kind: "header", label, count: entries.length }, ...entries);
    };
    section("invites", t("social.list.invites"), [
      ...worldInvites
        .filter((invite) => hit(invite.name))
        .map(
          (invite): Item => ({
            key: `w:${invite.session}`,
            kind: "worldInvite",
            session: invite.session,
          }),
        ),
      ...view.invites
        .filter((invite) => hit(invite.name))
        .map((invite): Item => ({ key: `i:${invite.id}`, kind: "invite", invite })),
    ]);
    if (view.teams) {
      section(
        "teamInvites",
        t("teams.invites"),
        view.teamInvites
          .filter((invite) => hit(invite.name))
          .map((invite) => ({ key: `ti:${invite.teamId}`, kind: "teamInvite", invite })),
      );
    }
    section(
      "requests",
      t("social.list.requests"),
      view.incoming
        .filter((request) => hit(request.name))
        .map((request) => ({ key: `r:${request.id}`, kind: "request", request })),
    );

    for (const team of crews) {
      const mates = teammates(team).filter((m) => hit(m.name, m.username));
      const folded = collapsed.includes(foldOf(team, multi));
      if (!q || mates.length) {
        list.push({ key: `t:${team.id}`, kind: "teamHeader", team, folded: folded && !q });
        // A call waits for an answer even while the crew is folded; under the first crew it names.
        if (!q) {
          for (const call of calls) {
            if (crewsOfCall(call, crews)[0]?.id !== team.id) continue;
            list.push({ key: `c:${call.id}`, kind: "call", call, team });
          }
          // Join requests wait for the player's answer (TEAM5), folded or not.
          const joins = waitingRequests(team, now).length;
          if (joins) list.push({ key: `j:${team.id}`, kind: "joins", team, count: joins });
        }
        if (!folded || q) {
          for (const member of mates) {
            list.push({ key: `m:${team.id}:${member.id}`, kind: "teammate", member, team });
          }
        }
      }
    }

    const friends = view.friends.filter((friend) => hit(friend.name, friend.username));
    const rows = (entries: Friend[], group: string | null): Item[] =>
      entries.map((friend) => ({ key: `f:${friend.id}`, kind: "friend", friend, group }));
    if (grouped && doc) {
      // While dragging, the list shows where the dragged one would land.
      let shown = doc;
      if (drag?.kind === "friend") shown = moveFriend(doc, drag.id, drag.over);
      if (drag?.kind === "group") shown = reorderGroups(doc, drag.order);
      const sections = groupSections(friends, shown, favorites);
      const custom = sections.filter((s) => s.id !== REST);
      // A friend over another category leaves its place open until the drop: closing it would
      // move everything below it, the target too, from under the pointer (FIX2).
      const away =
        drag?.kind === "friend" && drag.over !== drag.from && !drag.settling ? drag : null;
      const origin = away
        ? (groupSections(friends, doc, favorites)
            .find((s) => s.id === away.from)
            ?.friends.findIndex((f) => f.id === away.id) ?? -1)
        : -1;
      sections.forEach((s) => {
        if (drag?.kind === "group" && drag.id === s.id) {
          // The lifted category: a slot where it would land, its friends folded into it.
          list.push({ key: `g:${s.id}`, kind: "slot", height: drag.height, folded: false });
          return;
        }
        const folded = collapsed.includes(s.id);
        list.push({
          key: `g:${s.id}`,
          kind: "group",
          id: s.id,
          name: s.name,
          online: s.online,
          count: s.friends.length,
          folded,
          index: custom.indexOf(s),
          total: custom.length,
        });
        if (folded) {
          // A friend dragged over a folded category waits right under its header.
          const waiting = drag?.kind === "friend" && drag.over === s.id && drag.over !== drag.from;
          if (waiting) {
            list.push({ key: `f:${drag.id}`, kind: "slot", height: drag.height, folded: true });
          }
          return;
        }
        const entries = rows(s.friends, s.id);
        if (away && s.id === away.from && origin >= 0) {
          entries.splice(origin, 0, {
            key: `o:${away.id}`,
            kind: "slot",
            height: away.height,
            folded: false,
            origin: true,
          });
        }
        if (!entries.length) {
          list.push({ key: `e:${s.id}`, kind: "groupEmpty", id: s.id });
          return;
        }
        list.push(...entries);
      });
    } else {
      const groups = groupFriends(friends, favorites);
      section("playing", t("social.list.playing"), rows(groups.playing, null));
      section("online", t("social.list.online"), rows(groups.online, null));
      section("offline", t("social.list.offline"), rows(groups.offline, null));
    }
    if (categoriesOn && !q && view.friends.length > 0) {
      list.push({ key: "new", kind: "newGroup" });
    }
    section(
      "sent",
      t("social.list.sent"),
      view.outgoing
        .filter((request) => hit(request.name))
        .map((request) => ({ key: `s:${request.id}`, kind: "sent", request })),
    );
    return list;
  }, [
    view.invites,
    worldInvites,
    view.teams,
    view.teamInvites,
    view.incoming,
    view.friends,
    view.outgoing,
    crews,
    multi,
    calls,
    now,
    favorites,
    collapsed,
    doc,
    grouped,
    categoriesOn,
    drag,
    t,
    q,
  ]);

  // Ctrl+F finds a friend while the list is open.
  useEffect(() => {
    if (!searchable) return;
    const onKey = (event: globalThis.KeyboardEvent) => {
      const field = searchRef.current;
      // ↓ from the field goes on to the first friend, like the rows among themselves.
      if (event.key === "ArrowDown" && field && document.activeElement === field) {
        const first = listRef.current?.querySelector<HTMLElement>("[data-friend-row]");
        if (!first) return;
        event.preventDefault();
        first.focus();
        return;
      }
      // Not with Alt: Windows reports AltGr as Ctrl+Alt (Turkish Q types with it).
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.key.toLowerCase() !== "f") {
        return;
      }
      event.preventDefault();
      field?.focus();
      field?.select();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [searchable]);

  /** Keeps the focus on what it was on while the list moves around it (Alt+↑/↓). */
  const refocus = (selector: string) =>
    requestAnimationFrame(() => listRef.current?.querySelector<HTMLElement>(selector)?.focus());

  // Arrow keys move between friends (rows are buttons; Tab still visits the actions). Alt+↑/↓ on
  // a friend moves them to the category above or below.
  const onRowKey = (event: KeyboardEvent<HTMLButtonElement>, friendId?: string) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    if (event.altKey) {
      if (!friendId || !doc || !grouped) return;
      event.preventDefault();
      const next = stepFriend(doc, friendId, event.key === "ArrowUp" ? -1 : 1);
      if (!next) return;
      save(next);
      announce(
        t("friendGroups.moved", {
          name: friendName(friendId),
          group: nameOf(groupOf(next, friendId)),
        }),
      );
      refocus(`[data-row-friend="${friendId}"] [data-friend-row]`);
      return;
    }
    const rows = [
      ...(listRef.current?.querySelectorAll<HTMLElement>(
        "[data-friend-row], [data-category-header], [data-team-header]",
      ) ?? []),
    ];
    const at = rows.indexOf(event.currentTarget);
    if (at < 0) return;
    event.preventDefault();
    // ↑ from the first row goes back up to the search field, when there is one.
    if (at === 0 && event.key === "ArrowUp" && searchRef.current) {
      searchRef.current.focus();
      return;
    }
    rows[(at + (event.key === "ArrowDown" ? 1 : -1) + rows.length) % rows.length]?.focus();
  };

  const moveGroup = (id: string, step: -1 | 1) => {
    if (!doc) return;
    const next = stepGroup(doc, id, step);
    if (!next) return;
    save(next);
    announce(
      t("friendGroups.groupMoved", {
        name: nameOf(id),
        position: next.groups.findIndex((g) => g.id === id) + 1,
      }),
    );
  };

  const onHeaderKey = (event: KeyboardEvent<HTMLButtonElement>, id: string) => {
    if (event.altKey && (event.key === "ArrowUp" || event.key === "ArrowDown") && id !== REST) {
      event.preventDefault();
      moveGroup(id, event.key === "ArrowUp" ? -1 : 1);
      refocus(`[data-category-header="${id}"]`);
      return;
    }
    onRowKey(event);
  };

  const removeCategory = (id: string) => {
    if (!doc) return;
    const before = doc;
    const name = nameOf(id);
    save(removeGroup(doc, id));
    toast(t("friendGroups.removed", { name }), {
      action: { label: t("friendGroups.undo"), onClick: () => save(before) },
    });
  };

  const create = (name: string) => {
    if (!doc) return;
    const id = newGroupId(doc.groups);
    const friend = creating?.friend;
    setCreating(null);
    save(addGroup(doc, id, name, friend));
    if (friend) announce(t("friendGroups.moved", { name: friendName(friend), group: name }));
  };

  const empty =
    view.loaded &&
    !view.friends.length &&
    !view.incoming.length &&
    !view.outgoing.length &&
    !view.invites.length &&
    !worldInvites.length &&
    !view.teamInvites.length &&
    !crews.length;

  const draggedFriend =
    drag?.kind === "friend" ? view.friends.find((f) => f.id === drag.id) : undefined;
  const draggedGroup = drag?.kind === "group" ? drag : undefined;

  const render = (item: Item): ReactNode => {
    switch (item.kind) {
      case "header":
        return <Header label={item.label} count={item.count} />;
      case "friend": {
        const friend = item.friend;
        if (drag?.kind === "friend" && drag.id === friend.id) {
          return <DragSlot height={drag.height} />;
        }
        return (
          <div data-row-friend={friend.id}>
            <FriendRow
              friend={friend}
              favorite={favorites.includes(friend.id)}
              onKeyDown={(event) => onRowKey(event, friend.id)}
              team={teamRowFor(friend.id)}
              categories={
                doc && categoriesOn
                  ? {
                      groups: doc.groups,
                      current: groupOf(doc, friend.id),
                      onMove: (group) => moveTo(friend.id, group),
                      // After the menu: the name field takes the focus.
                      onNew: () => window.setTimeout(() => setCreating({ friend: friend.id }), 0),
                    }
                  : null
              }
              onPress={
                grouped && item.group
                  ? (event) =>
                      press(event, { kind: "friend", id: friend.id, from: item.group ?? REST })
                  : undefined
              }
            />
          </div>
        );
      }
      case "request":
        return <RequestRow request={item.request} />;
      case "sent":
        return <SentRow request={item.request} />;
      case "invite":
        return <InviteRow invite={item.invite} />;
      case "worldInvite":
        return <WorldInviteRow session={item.session} />;
      case "teamInvite":
        return <TeamInviteRow invite={item.invite} />;
      case "teamHeader":
        return (
          <TeamHeader
            team={item.team}
            folded={item.folded}
            onToggle={() => toggleCollapsed(foldOf(item.team, multi))}
            onOpen={() => openTeam(undefined, item.team.id)}
            onKeyDown={onRowKey}
          />
        );
      case "call":
        return <CallRow call={item.call} team={item.team} />;
      case "joins":
        return <JoinsRow team={item.team} count={item.count} />;
      case "teammate":
        return <TeamMemberRow member={item.member} team={item.team} onKeyDown={onRowKey} />;
      case "group": {
        const id = item.id;
        const target =
          drag?.kind === "friend" && drag.over === id && drag.over !== drag.from && !drag.settling;
        return (
          <div data-group-header={id}>
            <CategoryHeader
              id={id}
              name={item.name ?? t("friendGroups.rest")}
              online={item.online}
              count={item.count}
              folded={item.folded}
              target={target}
              canAdd={(doc?.groups.length ?? 0) < MAX_GROUPS}
              onNewCategory={() => setCreating({})}
              onToggle={() => toggleCollapsed(id)}
              onKeyDown={(event) => onHeaderKey(event, id)}
              actions={
                item.name === null || !doc
                  ? null
                  : {
                      canMoveUp: item.index > 0,
                      canMoveDown: item.index < item.total - 1,
                      onMove: (step) => moveGroup(id, step),
                      onRename: (name) => save(renameGroup(doc, id, name)),
                      onRemove: () => removeCategory(id),
                      onGrip: (event) =>
                        press(
                          event,
                          { kind: "group", id, order: doc.groups.map((g) => g.id) },
                          (event.currentTarget.closest("[data-group-header]") as HTMLElement) ??
                            undefined,
                        ),
                    }
              }
            />
          </div>
        );
      }
      case "groupEmpty":
        return <EmptyCategory dragging={drag?.kind === "friend" && !drag.settling} />;
      case "slot":
        return <DragSlot height={item.height} folded={item.folded} origin={item.origin} />;
      case "newGroup":
        return (
          <NewCategoryRow
            editing={creating !== null}
            disabled={(doc?.groups.length ?? 0) >= MAX_GROUPS}
            onStart={() => setCreating({})}
            onCreate={create}
            onCancel={() => setCreating(null)}
          />
        );
    }
  };

  return (
    <div>
      <ConnectionBanner />
      {!view.loaded ? (
        <ListSkeleton />
      ) : empty ? (
        <EmptyFriends />
      ) : (
        <>
          {searchable ? (
            <div className="px-1 pt-1">
              <SearchInput
                ref={searchRef}
                value={query}
                onChange={setQuery}
                placeholder={t("social.list.search")}
                label={t("social.list.search")}
                clearLabel={t("social.list.clearSearch")}
              />
            </div>
          ) : null}
          {q && !items.some((item) => item.kind !== "header" && item.kind !== "newGroup") ? (
            <p className="px-2 pt-8 text-center text-[13px] text-ash-400">
              {t("social.list.noMatch")}
            </p>
          ) : null}
          <LayoutGroup>
            <ul
              ref={listRef}
              aria-label={t("social.list.label")}
              className="relative flex flex-col"
            >
              <AnimatePresence initial={false} mode="popLayout">
                {items.map((item) => (
                  <motion.li
                    key={item.key}
                    layout={layout && !quiet ? "position" : false}
                    transition={{ layout: glide }}
                    // Layout glides compose with `y` (not a transform string).
                    initial={quiet ? false : { opacity: 0, y: reduce ? 0 : 6 }}
                    animate={{ opacity: 1, y: 0, transition: enter(0.26) }}
                    exit={{ opacity: 0, transition: quiet ? { duration: 0 } : leave(0.22) }}
                  >
                    {render(item)}
                  </motion.li>
                ))}
              </AnimatePresence>
            </ul>
          </LayoutGroup>
        </>
      )}
      {drag && draggedFriend ? (
        <DragGhost left={drag.left} width={drag.width} {...ghost}>
          <FriendGhost friend={draggedFriend} />
        </DragGhost>
      ) : null}
      {draggedGroup && doc ? (
        <DragGhost left={draggedGroup.left} width={draggedGroup.width} {...ghost}>
          <GroupGhost
            name={nameOf(draggedGroup.id)}
            friends={view.friends.filter((f) => groupOf(doc, f.id) === draggedGroup.id)}
          />
        </DragGhost>
      ) : null}
    </div>
  );
}
