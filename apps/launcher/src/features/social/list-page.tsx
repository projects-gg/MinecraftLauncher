// The friend list: waiting invites, requests, then friends in game, in the launcher and offline,
// and the requests the player sent. One list, so a friend who comes online glides from
// "Çevrimdışı" up to "Çevrimiçi" instead of vanishing and reappearing; section headers glide
// with it and fade when their section empties.
import {
  ArrowClockwise,
  Check,
  Copy,
  DotsThree,
  Prohibit,
  UserPlus,
  WifiSlash,
  X,
} from "@phosphor-icons/react";
import { Button, easeOutExpo, IconButton, Progress, SearchInput, Skeleton } from "@pl/ui";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";
import { DropdownMenu } from "radix-ui";
import { type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { Friend, FriendRequest, SocialInvite } from "@/lib/api/social-types";
import { formatRelative } from "@/lib/format";
import { matchesQuery } from "@/lib/search";
import { attempt, copyText } from "./actions";
import { FriendRow, JoinButton } from "./friend-row";
import { groupFriends, isJoinable } from "./model";
import { enter, glide, leave, rise, swap, useSocialMotion } from "./motion";
import { SocialAvatar } from "./social-face";
import { useSocialPrefs, useSocialUi } from "./store";

const ago = (unixSeconds: number) => formatRelative(new Date(unixSeconds * 1000).toISOString());

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
  const calm = useCalm();
  const [busy, setBusy] = useState(false);
  const [open, setOpen] = useState(false);
  const act = async (action: "accept" | "decline" | "block") => {
    setBusy(true);
    const done = await attempt(() =>
      action === "accept"
        ? api.social.accept(request.id)
        : action === "decline"
          ? api.social.decline(request.id)
          : api.social.block(request.id),
    );
    setBusy(false);
    if (done !== undefined && action === "accept") {
      announce(t("social.actions.accepted", { name: request.name }));
    }
  };
  return (
    <div className="flex items-center gap-3 rounded-[var(--radius-panel)] py-2 pr-1.5 pl-2">
      <SocialAvatar source={request} size={36} />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14px] font-semibold text-bone-50">
          {request.name}
        </span>
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
      <DropdownMenu.Root open={open} onOpenChange={setOpen} modal={false}>
        <DropdownMenu.Trigger asChild>
          <IconButton
            label={t("social.actions.more")}
            icon={<DotsThree size={18} weight="bold" />}
            size="sm"
            disabled={busy}
          />
        </DropdownMenu.Trigger>
        <AnimatePresence>
          {open ? (
            <DropdownMenu.Portal forceMount>
              <DropdownMenu.Content
                forceMount
                asChild
                align="end"
                sideOffset={4}
                collisionPadding={12}
                loop
              >
                <motion.div
                  className="z-50 w-[200px] rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-1.5 shadow-[0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
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
                    onSelect={() => void act("block")}
                    className="flex h-10 cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-[13.5px] text-flare-400 outline-none select-none data-[highlighted]:bg-white/8"
                  >
                    <Prohibit size={16} aria-hidden />
                    {t("social.actions.block")}
                  </DropdownMenu.Item>
                </motion.div>
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          ) : null}
        </AnimatePresence>
      </DropdownMenu.Root>
    </div>
  );
}

function SentRow({ request }: { request: FriendRequest }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  return (
    <div className="flex items-center gap-3 rounded-[var(--radius-panel)] py-2 pr-1.5 pl-2">
      <SocialAvatar source={request} size={36} />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14px] font-semibold text-bone-100">
          {request.name}
        </span>
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
      >
        {t("social.actions.cancelRequest")}
      </Button>
    </div>
  );
}

function InviteRow({ invite }: { invite: SocialInvite }) {
  const { t } = useTranslation();
  const name = invite.name ?? t("social.notices.someone");
  return (
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
      {isJoinable(invite.server) ? <JoinButton /> : null}
      <IconButton
        label={t("social.actions.dismiss")}
        icon={<X size={16} />}
        size="sm"
        onClick={() => void api.social.dismissInvite(invite.id)}
      />
    </div>
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
      className="group mt-4 inline-flex cursor-pointer items-center gap-2.5 rounded-md bg-obsidian-950/50 py-2 pr-2.5 pl-3 ring-1 ring-white/8 outline-none transition-[box-shadow,transform] duration-150 hover:ring-white/16 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.98]"
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
      <span className="flex size-16 items-center justify-center rounded-[var(--radius-head)] bg-gold-700/25 text-gold-300 ring-1 ring-gold-400/25">
        <UserPlus size={32} weight="duotone" aria-hidden />
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

type Item =
  | { key: string; kind: "header"; label: string; count: number }
  | { key: string; kind: "friend"; friend: Friend }
  | { key: string; kind: "request"; request: FriendRequest }
  | { key: string; kind: "sent"; request: FriendRequest }
  | { key: string; kind: "invite"; invite: SocialInvite };

/** From this many friends the list gets a search field (a short list reads at a glance). */
const SEARCH_FROM = 8;

export function ListPage() {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const view = useSocialUi((s) => s.view);
  const favorites = useSocialPrefs((s) => s.favorites);
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

  const items = useMemo<Item[]>(() => {
    const list: Item[] = [];
    const hit = (name: string | undefined, username?: string) =>
      matchesQuery(name ?? "", q) || (username ? matchesQuery(username, q) : false);
    const section = (key: string, label: string, entries: Item[]) => {
      if (!entries.length) return;
      list.push({ key: `h:${key}`, kind: "header", label, count: entries.length }, ...entries);
    };
    section(
      "invites",
      t("social.list.invites"),
      view.invites
        .filter((invite) => hit(invite.name))
        .map((invite) => ({ key: `i:${invite.id}`, kind: "invite", invite })),
    );
    section(
      "requests",
      t("social.list.requests"),
      view.incoming
        .filter((request) => hit(request.name))
        .map((request) => ({ key: `r:${request.id}`, kind: "request", request })),
    );
    const groups = groupFriends(
      view.friends.filter((friend) => hit(friend.name, friend.username)),
      favorites,
    );
    const rows = (friends: Friend[]): Item[] =>
      friends.map((friend) => ({ key: `f:${friend.id}`, kind: "friend", friend }));
    section("playing", t("social.list.playing"), rows(groups.playing));
    section("online", t("social.list.online"), rows(groups.online));
    section("offline", t("social.list.offline"), rows(groups.offline));
    section(
      "sent",
      t("social.list.sent"),
      view.outgoing
        .filter((request) => hit(request.name))
        .map((request) => ({ key: `s:${request.id}`, kind: "sent", request })),
    );
    return list;
  }, [view.invites, view.incoming, view.friends, view.outgoing, favorites, t, q]);

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
      if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== "f") return;
      event.preventDefault();
      field?.focus();
      field?.select();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [searchable]);

  // Arrow keys move between friends (rows are buttons; Tab still visits the actions).
  const onRowKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const rows = [...(listRef.current?.querySelectorAll<HTMLElement>("[data-friend-row]") ?? [])];
    const at = rows.indexOf(event.currentTarget);
    if (at < 0) return;
    event.preventDefault();
    // ↑ from the first friend goes back up to the search field, when there is one.
    if (at === 0 && event.key === "ArrowUp" && searchRef.current) {
      searchRef.current.focus();
      return;
    }
    rows[(at + (event.key === "ArrowDown" ? 1 : -1) + rows.length) % rows.length]?.focus();
  };

  const empty =
    view.loaded &&
    !view.friends.length &&
    !view.incoming.length &&
    !view.outgoing.length &&
    !view.invites.length;

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
          {q && !items.length ? (
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
                    {item.kind === "header" ? (
                      <Header label={item.label} count={item.count} />
                    ) : item.kind === "friend" ? (
                      <FriendRow
                        friend={item.friend}
                        favorite={favorites.includes(item.friend.id)}
                        onKeyDown={onRowKey}
                      />
                    ) : item.kind === "request" ? (
                      <RequestRow request={item.request} />
                    ) : item.kind === "sent" ? (
                      <SentRow request={item.request} />
                    ) : (
                      <InviteRow invite={item.invite} />
                    )}
                  </motion.li>
                ))}
              </AnimatePresence>
            </ul>
          </LayoutGroup>
        </>
      )}
    </div>
  );
}
