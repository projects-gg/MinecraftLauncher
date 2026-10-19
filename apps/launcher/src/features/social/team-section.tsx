// The team in the friend list: its header (emblem, name, tag, teammates online; folds like a
// category; the arrow opens the team page) and invites to other teams, answered in place.
import { CaretDown, CaretRight, Check, X } from "@phosphor-icons/react";
import { Button, cn, IconButton } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type KeyboardEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { SocialTeam, TeamInvite } from "@/lib/api/social-types";
import { formatRelative } from "@/lib/format";
import { attempt } from "./actions";
import { swap, useSocialMotion } from "./motion";
import { TeamEmblem, TeamTag } from "./team-emblem";
import { teammatesOnline } from "./teams";

const ago = (unixSeconds: number) => formatRelative(new Date(unixSeconds * 1000).toISOString());

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
  return (
    <div className="flex items-center gap-1 pt-3 pr-1 pb-1">
      <button
        type="button"
        aria-expanded={!folded}
        aria-label={t("teams.summary", { name: team.name, online })}
        onClick={onToggle}
        onKeyDown={onKeyDown}
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
        <TeamEmblem emblem={team.emblem} size={22} />
        <span className="min-w-0 truncate text-[13px] font-semibold text-bone-50">{team.name}</span>
        <TeamTag tag={team.tag} />
        <span className="relative ml-0.5 inline-flex font-display text-[12px] tabular-nums">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span key={online} className="flex" {...swap(reduce, 0.18)}>
              <span className={online > 0 ? "text-jade-400" : "text-ash-400"}>{online}</span>
              <span className="text-ash-400">/{team.members.length - 1}</span>
            </motion.span>
          </AnimatePresence>
        </span>
      </button>
      <IconButton
        label={t("teams.open")}
        icon={<CaretRight size={16} weight="bold" />}
        size="sm"
        onClick={onOpen}
      />
    </div>
  );
}

/** An invite to a team: its emblem and name, who sent it, "Kabul et" and ×. */
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
      <TeamEmblem emblem={invite.emblem} size={36} />
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-1.5">
          <span className="truncate text-[14px] font-semibold text-bone-50">{invite.name}</span>
          <TeamTag tag={invite.tag} />
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
