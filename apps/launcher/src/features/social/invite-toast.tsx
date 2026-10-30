// "Birlikte oyna" arriving live: a toast with the friend's face, where they play, and "Katıl"
// when the invite leads to a server (projects.gg, or another one whose address the friend shares).
// "Katıl" asks before it starts the game (join-dialog.tsx). The toast stays a little longer than
// other toasts (it asks for an answer), and the invite also waits in the panel for an hour.
// A friend's world invite (HOST1, host1.md §1.4) arrives the same way: "Mert seni dünyasına
// çağırıyor", their Minecraft version small under it, "Katıl" (the join then says where it is on
// the invite's rows) and "Kapat"; the invite waits in the panel while the world is open.
import { X } from "@phosphor-icons/react";
import { Button, IconButton, toast } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { pressJoin } from "@/features/host/actions";
import type { WorldInvite } from "@/lib/api/host-types";
import type { SocialInvite, TeamInvite } from "@/lib/api/social-types";
import { canJoinInvite, joinInvite } from "./join-dialog";
import { isJoinable } from "./model";
import { MarkedSentence } from "./person-mark";
import { SocialAvatar } from "./social-face";
import { useSocialUi } from "./store";
import { CrewMark } from "./team-emblem";

function InviteToast({ toastId, invite }: { toastId: string | number; invite: SocialInvite }) {
  const { t } = useTranslation();
  const name = invite.name ?? t("social.notices.someone");
  const where = isJoinable(invite.server)
    ? t("social.presence.projects")
    : invite.server?.host
      ? t("social.presence.otherHost", { host: invite.server.host })
      : null;
  return (
    <div className="flex w-full items-center gap-3 font-normal">
      <SocialAvatar
        source={{ name, avatar: invite.avatar, avatarUrl: invite.avatarUrl }}
        size={40}
        state="playing"
      />
      <div className="min-w-0 flex-1">
        <p className="text-[13.5px] leading-snug font-semibold text-bone-50">
          <MarkedSentence
            text={(who) => t("social.notices.invite", { name: who })}
            name={name}
            mark={invite.mark}
          />
        </p>
        {where ? <p className="truncate text-[12.5px] text-jade-400">{where}</p> : null}
      </div>
      {canJoinInvite(invite) ? (
        <Button
          size="sm"
          variant="primary"
          onClick={() => {
            toast.dismiss(toastId);
            joinInvite(invite);
          }}
        >
          {t("social.actions.join")}
        </Button>
      ) : null}
      <IconButton
        label={t("social.actions.dismiss")}
        icon={<X size={15} />}
        size="sm"
        className="-mr-1"
        onClick={() => toast.dismiss(toastId)}
      />
    </div>
  );
}

export function showInviteToast(invite: SocialInvite) {
  toast.custom((id) => <InviteToast toastId={id} invite={invite} />, {
    id: `invite:${invite.id}`,
    duration: 12_000,
    classNames: { content: "min-w-0 flex-1", title: "w-full" },
  });
}

function WorldInviteToast({ toastId, invite }: { toastId: string | number; invite: WorldInvite }) {
  const { t } = useTranslation();
  // The friend's mark (SOC-VERIFY) from the friend list: the invite itself does not carry it.
  const mark = useSocialUi((s) => s.view.friends.find((friend) => friend.id === invite.from)?.mark);
  return (
    <div className="flex w-full items-center gap-3 font-normal">
      <SocialAvatar
        source={{ name: invite.name, avatar: invite.avatar, avatarUrl: invite.avatarUrl }}
        size={40}
        state="playing"
      />
      <div className="min-w-0 flex-1">
        <p className="text-[13.5px] leading-snug font-semibold text-bone-50">
          <MarkedSentence
            text={(who) => t("host.invite.title", { name: who })}
            name={invite.name}
            mark={mark}
          />
        </p>
        <p className="truncate text-[12px] text-ash-400 tabular-nums">
          <span className="sr-only">{t("host.invite.version", { version: invite.mc })}</span>
          <span aria-hidden>{invite.mc}</span>
        </p>
      </div>
      <Button
        size="sm"
        variant="primary"
        onClick={() => {
          toast.dismiss(toastId);
          pressJoin(invite);
        }}
      >
        {t("social.actions.join")}
      </Button>
      <IconButton
        label={t("social.actions.dismiss")}
        icon={<X size={15} />}
        size="sm"
        className="-mr-1"
        onClick={() => toast.dismiss(toastId)}
      />
    </div>
  );
}

/** A friend's world invite, live (HOST1). */
export function showWorldInviteToast(invite: WorldInvite) {
  toast.custom((id) => <WorldInviteToast toastId={id} invite={invite} />, {
    id: `world:${invite.session}`,
    duration: 12_000,
    classNames: { content: "min-w-0 flex-1", title: "w-full" },
  });
}

/**
 * A crew invite, live: who invites, with the crew's mark on the face's corner as on a call's toast,
 * and "Daveti gör", which opens the list where the invite waits (its answers are there). Quiet, not
 * gold: the toast sits right above OYNA.
 */
function TeamInviteToast({ toastId, invite }: { toastId: string | number; invite: TeamInvite }) {
  const { t } = useTranslation();
  const from = invite.from;
  // The button under the sentence: beside it, a crew's name squeezed the sentence to three lines.
  return (
    <div className="flex w-full items-start gap-3 font-normal">
      <span className="relative flex shrink-0">
        <SocialAvatar source={from} size={40} />
        <span className="absolute -right-1.5 -bottom-1.5">
          <CrewMark look={invite} size={22} />
        </span>
      </span>
      <div className="min-w-0 flex-1">
        <p className="text-[13.5px] leading-snug font-semibold text-bone-50">
          <MarkedSentence
            text={(who) => t("teams.notices.invite", { name: who, team: invite.name })}
            name={from.name}
            mark={from.mark}
          />
        </p>
        <Button
          size="sm"
          variant="secondary"
          className="mt-2"
          onClick={() => {
            toast.dismiss(toastId);
            useSocialUi.getState().openPanel({ page: "list" });
          }}
        >
          {t("teams.notices.seeInvite")}
        </Button>
      </div>
      <IconButton
        label={t("social.actions.dismiss")}
        icon={<X size={15} />}
        size="sm"
        className="-mr-1"
        onClick={() => toast.dismiss(toastId)}
      />
    </div>
  );
}

/** A crew invite, live (the list shows it in place when it is open). */
export function showTeamInviteToast(invite: TeamInvite) {
  toast.custom((id) => <TeamInviteToast toastId={id} invite={invite} />, {
    id: `team-invite:${invite.teamId}`,
    duration: 12_000,
    classNames: { content: "min-w-0 flex-1", title: "w-full" },
  });
}
