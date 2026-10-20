// The crew in the friend list: its header (the crew's mark, name, tinted tag, crewmates online;
// folds like a category; the arrow opens the crew page; its menu opens, folds, calls the crew,
// shares the code, invites, edits and leaves, each where the crew lets the player) and invites to
// other crews, answered in place.
import {
  CaretDown,
  CaretRight,
  Check,
  Copy,
  DotsThree,
  MegaphoneSimple,
  PencilSimple,
  SignOut,
  Trash,
  UserPlus,
  UsersThree,
  X,
} from "@phosphor-icons/react";
import { Button, ContextMenu, cn, IconButton, Menu, type MenuItem, toast } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type KeyboardEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { SocialTeam, TeamInvite } from "@/lib/api/social-types";
import { formatRelative } from "@/lib/format";
import { attempt, copyText } from "./actions";
import { callCrew } from "./call-actions";
import { callWaitUntil, liveCalls } from "./calls";
import { openMenuFromKey } from "./menu-key";
import { rowsOf, teamMenu } from "./menus";
import { swap, useSocialMotion } from "./motion";
import { useSocialUi } from "./store";
import { CrewMark, TeamTag } from "./team-emblem";
import { crewRights, teammatesOnline } from "./teams";
import { useCallClock } from "./use-call-clock";

const ago = (unixSeconds: number) => formatRelative(new Date(unixSeconds * 1000).toISOString());

/** The team header's menu: a right click, Shift+F10 or the menu key, or its "…". */
function useTeamItems(team: SocialTeam, folded: boolean, onToggle: () => void) {
  const { t } = useTranslation();
  const friends = useSocialUi((s) => s.view.friends);
  const crew = useSocialUi((s) => s.view.crew);
  const calls = useSocialUi((s) => s.view.calls);
  const refusedUntil = useSocialUi((s) => s.callRefusedUntil);
  const lastCallAt = useSocialUi((s) => s.lastCallAt);
  const now = useCallClock((s) => s.now);
  const openTeam = useSocialUi((s) => s.openTeam);
  const members = new Set(team.members.map((m) => m.id));
  const friendsOutside = friends.filter((f) => !members.has(f.id)).length;
  const rights = crewRights(team, crew);
  // A call that is on (anyone's) takes the button's place on the crew page: none here either.
  const call =
    !rights.call || liveCalls(calls, now).length
      ? "hidden"
      : callWaitUntil(calls, refusedUntil, lastCallAt) > now
        ? "waiting"
        : "ready";
  const plan = teamMenu({
    team,
    friendsOutside,
    call,
    canInvite: rights.invite,
    canEdit: rights.look,
  });
  return rowsOf(plan, (row): MenuItem | null => {
    switch (row.action) {
      case "call":
        return {
          label: t("teams.call.start"),
          icon: <MegaphoneSimple size={16} />,
          disabled: row.disabled,
          onSelect: () => void callCrew(),
        };
      case "open":
        return {
          label: t("teams.open"),
          icon: <UsersThree size={16} />,
          onSelect: () => openTeam(),
        };
      case "toggle":
        return {
          label: folded ? t("friendMenus.expand") : t("friendMenus.collapse"),
          icon: folded ? <CaretDown size={16} /> : <CaretRight size={16} />,
          onSelect: onToggle,
        };
      case "copyCode":
        return {
          label: t("friendMenus.copyTeamCode"),
          icon: <Copy size={16} />,
          onSelect: () =>
            void copyText(team.code).then((ok) => {
              if (!ok) return;
              toast.success(t("friendMenus.teamCodeCopied"));
              announce(t("friendMenus.teamCodeCopied"));
            }),
        };
      case "invite":
        return {
          label: t("teams.invite.title"),
          icon: <UserPlus size={16} />,
          disabled: row.disabled,
          onSelect: () => openTeam("invite"),
        };
      case "edit":
        return {
          label: t("friendMenus.editTeam"),
          icon: <PencilSimple size={16} />,
          onSelect: () => openTeam("edit"),
        };
      case "leave":
      case "close":
      case "disband":
        return {
          label:
            row.action === "leave"
              ? t("teams.leave.action")
              : row.action === "close"
                ? t("teams.leave.close")
                : t("teams.leave.disband"),
          icon: row.action === "leave" ? <SignOut size={16} /> : <Trash size={16} />,
          danger: true,
          // Asked on the team page, in place, with one sentence.
          onSelect: () => openTeam("leave"),
        };
    }
  });
}

export function TeamHeader({
  team,
  folded,
  onToggle,
  onOpen,
  onKeyDown,
}: {
  team: SocialTeam;
  folded: boolean;
  onToggle: () => void;
  onOpen: () => void;
  onKeyDown?: (event: KeyboardEvent<HTMLButtonElement>) => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const online = teammatesOnline(team);
  const items = useTeamItems(team, folded, onToggle);
  return (
    <ContextMenu items={items}>
      <div className="group/team flex items-center gap-1 pt-3 pr-1 pb-1">
        <button
          type="button"
          data-team-header
          aria-expanded={!folded}
          aria-label={t("teams.summary", { name: team.name, online })}
          onClick={onToggle}
          onKeyDown={(event) => {
            if (!openMenuFromKey(event)) onKeyDown?.(event);
          }}
          className="flex h-8 min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-md px-1.5 text-left outline-none transition-colors duration-150 hover:bg-white/[0.04] focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          <CaretDown
            size={12}
            weight="bold"
            aria-hidden
            className={cn(
              "shrink-0 text-ash-400 transition-[rotate] duration-200 ease-[cubic-bezier(0.16,1,0.3,1)]",
              folded && "-rotate-90",
            )}
          />
          <CrewMark look={team} size={22} />
          <span className="min-w-0 truncate text-[13px] font-semibold text-bone-50">
            {team.name}
          </span>
          <TeamTag tag={team.tag} color={team.color} />
          <span className="relative ml-0.5 inline-flex font-display text-[12px] tabular-nums">
            <AnimatePresence mode="popLayout" initial={false}>
              <motion.span key={online} className="flex" {...swap(reduce, 0.18)}>
                <span className={online > 0 ? "text-jade-400" : "text-ash-400"}>{online}</span>
                <span className="text-ash-400">/{team.members.length - 1}</span>
              </motion.span>
            </AnimatePresence>
          </span>
        </button>
        <Menu
          items={items}
          trigger={
            <IconButton
              label={t("friendMenus.options", { name: team.name })}
              icon={<DotsThree size={18} weight="bold" />}
              size="sm"
              className="opacity-0 transition-opacity duration-150 group-hover/team:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
            />
          }
        />
        <IconButton
          label={t("teams.open")}
          icon={<CaretRight size={16} weight="bold" />}
          size="sm"
          onClick={onOpen}
        />
      </div>
    </ContextMenu>
  );
}

/** An invite to a crew: its mark and name, who sent it, "Kabul et" and ×. */
export function TeamInviteRow({ invite }: { invite: TeamInvite }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const act = async (accept: boolean) => {
    setBusy(true);
    const done = await attempt(() =>
      accept
        ? api.social.acceptTeamInvite(invite.teamId)
        : api.social.declineTeamInvite(invite.teamId),
    );
    setBusy(false);
    if (done !== undefined && accept) announce(t("teams.joined", { name: invite.name }));
  };
  return (
    <div className="mb-1.5 flex items-center gap-3 rounded-[var(--radius-panel)] bg-gold-500/[0.07] py-2 pr-1.5 pl-2 ring-1 ring-gold-400/15">
      <CrewMark look={invite} size={36} />
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-1.5">
          <span className="truncate text-[14px] font-semibold text-bone-50">{invite.name}</span>
          <TeamTag tag={invite.tag} color={invite.color} />
        </span>
        <span className="block truncate text-[12.5px] text-ash-300">
          {t("teams.invitedBy", { name: invite.from.name })}, {ago(invite.sentAt)}
        </span>
      </span>
      <Button
        size="sm"
        variant="secondary"
        icon={<Check size={14} weight="bold" />}
        disabled={busy}
        onClick={() => void act(true)}
      >
        {t("teams.accept")}
      </Button>
      <IconButton
        label={t("teams.decline")}
        icon={<X size={16} />}
        size="sm"
        disabled={busy}
        onClick={() => void act(false)}
      />
    </div>
  );
}
