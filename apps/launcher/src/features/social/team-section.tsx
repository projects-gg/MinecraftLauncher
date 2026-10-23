// The crew in the friend list: its header (the crew's mark, name, tinted tag, crewmates online, a
// struck-through bell while muted; folds like a category; the arrow opens the crew page; its menu
// opens, folds, calls the crew, shares the code, invites, edits, mutes and leaves, each where the
// crew and the service let the player) and invites to other crews, answered in place. With several
// crews (TEAM4) each has its own header, in the player's order: its menu also makes it the primary
// crew or moves it up or down (Alt+↑/↓ on the header), and its bell is its own.
import {
  Bell,
  BellSlash,
  CaretDown,
  CaretRight,
  ChatsCircle,
  Check,
  Copy,
  DotsThree,
  Flag,
  MegaphoneSimple,
  PencilSimple,
  SignOut,
  Star,
  TagSimple,
  Trash,
  UserPlus,
  UsersThree,
  X,
} from "@phosphor-icons/react";
import { Button, ContextMenu, cn, IconButton, Menu, type MenuItem, Tooltip, toast } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type KeyboardEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { SocialTeam, TeamInvite } from "@/lib/api/social-types";
import { formatRelative } from "@/lib/format";
import { attempt, copyText } from "./actions";
import { sayJoined, wearTag } from "./badge-actions";
import { wearState } from "./badges";
import { callOrAsk } from "./call-actions";
import { callNames, callWaitUntil, liveCalls } from "./calls";
import { muteItems, unmuteCrew, useCrewMute } from "./crew-notify";
import { makeMainCrew, stepCrew } from "./crew-order";
import { canAddCrew } from "./crews";
import { openMenuFromKey } from "./menu-key";
import { rowsOf, type TeamAction, teamMenu, without } from "./menus";
import { pop, swap, useSocialMotion } from "./motion";
import { useSocialUi } from "./store";
import { CrewMark, TeamTag } from "./team-emblem";
import { crewRights, teammatesOnline } from "./teams";
import { useCallClock } from "./use-call-clock";

const ago = (unixSeconds: number) => formatRelative(new Date(unixSeconds * 1000).toISOString());

/** Where a crew's menu opens: the friend list's header, the crew page's rail or its card. */
export interface CrewMenuPlace {
  /** The friend list's header folds the crew ("Daralt" / "Genişlet"). */
  folded?: boolean;
  onToggle?: () => void;
  /** What "Ekibi aç" does here; false where the crew is on screen already (its own card). */
  onOpen?: (() => void) | false;
  /** The crew page's rail lays the crews out sideways: "Sola taşı" / "Sağa taşı". */
  sideways?: boolean;
}

/**
 * A crew's menu, the same wherever the crew shows (FIX2): a right click, Shift+F10 or the menu
 * key, or the header's "…". Rows a place has no use for (folding off the list, opening the crew
 * already on screen) are left out there.
 */
export function useCrewItems(team: SocialTeam, place: CrewMenuPlace = {}) {
  const { folded = false, onToggle, onOpen, sideways = false } = place;
  const { t } = useTranslation();
  const friends = useSocialUi((s) => s.view.friends);
  const crew = useSocialUi((s) => s.view.crew);
  const crews = useSocialUi((s) => s.view.crews);
  const calls = useSocialUi((s) => s.view.calls);
  const refusedUntil = useSocialUi((s) => s.callRefusedUntil);
  const lastCallAt = useSocialUi((s) => s.lastCallAt);
  const now = useCallClock((s) => s.now);
  const openTeam = useSocialUi((s) => s.openTeam);
  const members = new Set(team.members.map((m) => m.id));
  const friendsOutside = friends.filter((f) => !members.has(f.id)).length;
  const rights = crewRights(team, crew);
  const mute = useCrewMute(team.id);
  // A call on in this crew (anyone's) takes the button's place on the crew page: none here either.
  const call =
    !rights.call || liveCalls(calls, now).some((open) => callNames(open, team, crews))
      ? "hidden"
      : callWaitUntil(calls, refusedUntil, lastCallAt) > now
        ? "waiting"
        : "ready";
  const at = crews.findIndex((c) => c.id === team.id);
  const chat = useSocialUi((s) => s.view.chat !== undefined);
  // TEAM6: the crew's tag ("Etiketi tak" / "Etiketi çıkar"), a change under way included.
  const worn = useSocialUi((s) =>
    s.badgePending !== undefined ? s.badgePending : (s.view.me?.badge?.crew ?? null),
  );
  const badge = crew.badges ? wearState(team, worn, Date.now()) : undefined;
  const full = teamMenu({
    team,
    friendsOutside,
    call,
    canInvite: rights.invite,
    canEdit: rights.look,
    mute: mute.target || mute.settings ? (mute.muted ? "muted" : "unmuted") : "hidden",
    order:
      crew.multi && crews.length > 1 && at >= 0
        ? { first: at === 0, last: at === crews.length - 1 }
        : undefined,
    chat,
    // The leader runs the crew, so reporting it is someone else's step; members and moderators keep it.
    report: team.role !== "founder",
    badge,
  });
  const drop: TeamAction[] = [];
  if (!onToggle) drop.push("toggle");
  if (onOpen === false) drop.push("open");
  const plan = without(full, drop);
  return rowsOf(plan, (row): MenuItem | null => {
    switch (row.action) {
      case "chat":
        return {
          label: t("fix2.menu.crewChat"),
          icon: <ChatsCircle size={16} />,
          onSelect: () => useSocialUi.getState().openChat(`c:${team.id}`),
        };
      case "report":
        return {
          label: t("reports.crewAction"),
          icon: <Flag size={16} />,
          onSelect: () =>
            useSocialUi.getState().openReport({
              target: { kind: "crew", id: team.id },
              name: team.name,
              crew: team,
            }),
        };
      case "mute":
        return {
          label: t("teams.mute.action"),
          icon: <BellSlash size={16} />,
          items: muteItems(t, mute.target),
          onSelect: () => {},
        };
      case "unmute":
        return {
          label: t("teams.mute.undo"),
          icon: <Bell size={16} />,
          onSelect: () => void unmuteCrew(mute.target),
        };
      case "call":
        return {
          label: t("teams.call.start"),
          icon: <MegaphoneSimple size={16} />,
          disabled: row.disabled,
          onSelect: () => void callOrAsk([team.id]),
        };
      case "open":
        return {
          label: t("teams.open"),
          icon: <UsersThree size={16} />,
          onSelect: onOpen || (() => openTeam(undefined, team.id)),
        };
      case "makeMain":
        return {
          label: t("crews.makeMain"),
          icon: <Star size={16} />,
          onSelect: () => makeMainCrew(team.id),
        };
      case "wearTag":
        // Soft while it cannot be worn, its reason beside it (a menu row has no tooltip).
        return {
          label:
            badge === "hidden"
              ? `${t("team6.wear")} (${t("team6.hiddenShort")})`
              : badge === "locked"
                ? `${t("team6.wear")} (${t("team6.lockedShort")})`
                : t("team6.wear"),
          icon: <TagSimple size={16} />,
          disabled: row.disabled,
          onSelect: () => void wearTag(team.id),
        };
      case "takeOffTag":
        return {
          label: t("team6.takeOff"),
          icon: <TagSimple size={16} />,
          onSelect: () => void wearTag(null),
        };
      case "moveUp":
      case "moveDown": {
        const up = row.action === "moveUp";
        return {
          label: sideways
            ? t(up ? "crews.moveLeft" : "crews.moveRight")
            : t(up ? "crews.moveUp" : "crews.moveDown"),
          shortcut: sideways ? (up ? "Alt ←" : "Alt →") : up ? "Alt ↑" : "Alt ↓",
          disabled: row.disabled,
          onSelect: () => stepCrew(team.id, up ? -1 : 1),
        };
      }
      case "toggle":
        return {
          label: folded ? t("friendMenus.expand") : t("friendMenus.collapse"),
          icon: folded ? <CaretDown size={16} /> : <CaretRight size={16} />,
          onSelect: () => onToggle?.(),
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
          onSelect: () => openTeam("invite", team.id),
        };
      case "edit":
        return {
          label: t("friendMenus.editTeam"),
          icon: <PencilSimple size={16} />,
          onSelect: () => openTeam("edit", team.id),
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
          onSelect: () => openTeam("leave", team.id),
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
  const items = useCrewItems(team, { folded, onToggle });
  const { muted: own, target, allMuted } = useCrewMute(team.id);
  const muted = own || (Boolean(target) && allMuted);
  const multi = useSocialUi((s) => s.view.crew.multi && s.view.crews.length > 1);
  return (
    <ContextMenu items={items}>
      <div className="group/team flex items-center gap-1 pt-3 pr-1 pb-1">
        <button
          type="button"
          data-team-header
          aria-expanded={!folded}
          aria-label={t(muted ? "teams.summaryMuted" : "teams.summary", {
            name: team.name,
            online,
          })}
          onClick={onToggle}
          onKeyDown={(event) => {
            if (openMenuFromKey(event)) return;
            // Not with Ctrl: Windows reports AltGr as Ctrl+Alt (Turkish Q types with it).
            if (
              multi &&
              event.altKey &&
              !event.ctrlKey &&
              (event.key === "ArrowUp" || event.key === "ArrowDown")
            ) {
              event.preventDefault();
              stepCrew(team.id, event.key === "ArrowUp" ? -1 : 1);
              return;
            }
            onKeyDown?.(event);
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
          <AnimatePresence initial={false}>
            {muted ? (
              <motion.span key="muted" className="flex text-ash-400" {...pop(reduce, 0.18)}>
                <BellSlash size={13} aria-hidden />
              </motion.span>
            ) : null}
          </AnimatePresence>
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

/** An invite to a crew: its mark and name, who sent it, "Kabul et" and ×. At the crew limit (several
 * crews) "Kabul et" waits and says why; × still declines. */
export function TeamInviteRow({ invite }: { invite: TeamInvite }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const full = useSocialUi(
    (s) => s.view.crew.multi && !canAddCrew(s.view.crews, s.view.crewLimit, true),
  );
  const limit = useSocialUi((s) => s.view.crewLimit);
  const act = async (accept: boolean) => {
    setBusy(true);
    const done = await attempt(() =>
      accept
        ? api.social.acceptTeamInvite(invite.teamId)
        : api.social.declineTeamInvite(invite.teamId),
    );
    setBusy(false);
    // With crew tags: "Etiketi tak" on the toast while the player wears none (TEAM6).
    if (done !== undefined && accept) {
      sayJoined(t("teams.joined", { name: invite.name }), () => invite.teamId);
    }
  };
  // "Rapor et": the crew, or who sent the invite (the dialog asks which).
  const report: MenuItem[] = [
    {
      label: t("reports.action"),
      icon: <Flag size={16} />,
      onSelect: () =>
        useSocialUi.getState().openReport({
          target: { kind: "crew", id: invite.teamId },
          name: invite.name,
          crew: invite,
          inviter: {
            target: { kind: "player", id: invite.from.id },
            name: invite.from.name,
            person: invite.from,
          },
        }),
    },
  ];
  return (
    <ContextMenu items={report}>
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
        <Tooltip content={t("crews.limit", { count: limit })} disabled={!full}>
          <span className="inline-flex">
            <Button
              size="sm"
              variant="secondary"
              icon={<Check size={14} weight="bold" />}
              disabled={busy || full}
              onClick={() => void act(true)}
            >
              {t("teams.accept")}
            </Button>
          </span>
        </Tooltip>
        <IconButton
          label={t("teams.decline")}
          icon={<X size={16} />}
          size="sm"
          disabled={busy}
          onClick={() => void act(false)}
        />
        <Menu
          items={report}
          trigger={
            <IconButton
              label={t("social.actions.more")}
              icon={<DotsThree size={18} weight="bold" />}
              size="sm"
              disabled={busy}
            />
          }
        />
      </div>
    </ContextMenu>
  );
}
