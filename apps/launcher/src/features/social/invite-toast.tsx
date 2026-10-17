// "Birlikte oyna" arriving live: a toast with the friend's face, where they play, and "Katıl"
// when that is projects.gg. It stays a little longer than other toasts (it asks for an
// answer), and the invite also waits in the panel for an hour.
import { X } from "@phosphor-icons/react";
import { Button, IconButton, toast } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import type { SocialInvite } from "@/lib/api/social-types";
import { useLaunch } from "@/stores/launch";
import { gameBusy, joinProjects } from "./join";
import { isJoinable } from "./model";
import { SocialAvatar } from "./social-face";

function InviteToast({ toastId, invite }: { toastId: string | number; invite: SocialInvite }) {
  const { t } = useTranslation();
  const busy = useLaunch((s) => gameBusy(s.phase.kind));
  const name = invite.name ?? t("social.notices.someone");
  const joinable = isJoinable(invite.server);
  const where = joinable
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
          {t("social.notices.invite", { name })}
        </p>
        {where ? <p className="truncate text-[12.5px] text-jade-400">{where}</p> : null}
      </div>
      {joinable ? (
        <Button
          size="sm"
          variant="primary"
          disabled={busy}
          onClick={() => {
            toast.dismiss(toastId);
            void api.social.dismissInvite(invite.id);
            void joinProjects();
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
