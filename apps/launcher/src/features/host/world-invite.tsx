// A friend's world invite where the friends panel and Home show it (HOST1, host1.md §1.4, §6.1):
// the panel's invite row ("Katıl" / × "Daveti kapat", "2/8") and Home's line in the "Birlikte
// oyna" slot (× "Daveti gizle": it leaves Home only). "Katıl" says where the join is in place
// (parts.tsx); while it is under way the × keeps its place but waits, so nothing beside it moves.
// A right click (Shift+F10, the menu key) offers the row's own actions under the ×'s own name,
// then the friend's rows without a second "Katıl" (FIX2, SMALL10).
import { Play, X } from "@phosphor-icons/react";
import { ContextMenu, IconButton, type MenuItem } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { openMenuFromKey } from "@/features/social/menu-key";
import { usePeopleItems } from "@/features/social/person-menu";
import { SocialAvatar } from "@/features/social/social-face";
import { useSocialUi } from "@/features/social/store";
import type { WorldInvite } from "@/lib/api/host-types";
import { dismissWorldInvite, pressJoin } from "./actions";
import { Faces, PeopleCount, WorldJoinButton } from "./parts";
import { inviteFailureKey, invitePeople, joinActive, retryWithCopy } from "./rules";
import { useHostUi } from "./store";

/** The friend's own "Katıl" is the row's: their rows follow without it. */
const SKIP_JOIN = ["joinWorld"] as const;

function useInviteItems(invite: WorldInvite, where: "panel" | "home"): Array<MenuItem | null> {
  const { t } = useTranslation();
  const active = joinActive(invite.join);
  const closeHome = useHostUi((s) => s.closeHome);
  // After the game turned away for missing mods the row's button is "Kur ve katıl": the menu
  // names the same action the same way (it still ends in joining, hence the same mark).
  const copy = retryWithCopy(invite.join);
  const people = usePeopleItems(
    [{ id: invite.from, name: invite.name, avatar: invite.avatar, avatarUrl: invite.avatarUrl }],
    { from: "list", skip: SKIP_JOIN },
  );
  return [
    {
      label: copy ? t("host.join.copyAction") : t("social.actions.join"),
      icon: <Play size={16} />,
      disabled: active,
      onSelect: () => pressJoin(invite),
    },
    // What the row's × does, by the same name: the panel turns the invite down, Home only hides
    // its line (the invite stays in the panel).
    where === "panel"
      ? {
          label: t("small10.menu.dismissInvite"),
          icon: <X size={16} />,
          disabled: active,
          onSelect: () => dismissWorldInvite(invite),
        }
      : {
          label: t("small10.menu.hideInvite"),
          icon: <X size={16} />,
          onSelect: () => closeHome(invite.session),
        },
    ...(people.length ? [null, ...people] : []),
  ];
}

/** "2/8" under the invite: who is in the friend's world, the friend counted. */
function InviteCount({ invite, className }: { invite: WorldInvite; className?: string }) {
  const { t } = useTranslation();
  const people = invitePeople(invite);
  return (
    <PeopleCount
      count={people.count}
      max={people.max}
      label={t("host.invite.count", people)}
      className={className}
    />
  );
}

/** The friends panel's invite row (the "Davetler" section); it follows its own invite. */
export function WorldInviteRow({ session }: { session: string }) {
  const invite = useHostUi((s) => s.view.invites.find((one) => one.session === session));
  return invite ? <InviteRowBody invite={invite} /> : null;
}

function InviteRowBody({ invite }: { invite: WorldInvite }) {
  const { t } = useTranslation();
  const items = useInviteItems(invite, "panel");
  const active = joinActive(invite.join);
  // A failed join says why beside its button ("Tekrar dene", or "Kur ve katıl" after
  // missing mods): the toast that said it may be gone when the player looks. Missing mods are one
  // press from fixed, so their line stays calm; a refused join reads as an error.
  const failed = inviteFailureKey(invite.join);
  const calm = retryWithCopy(invite.join);
  return (
    <ContextMenu items={items}>
      <div className="mb-1.5 flex items-center gap-3 rounded-[var(--radius-panel)] bg-gold-500/[0.07] py-2 pr-1.5 pl-2 ring-1 ring-gold-400/15">
        <SocialAvatar
          source={{ name: invite.name, avatar: invite.avatar, avatarUrl: invite.avatarUrl }}
          size={36}
          state="playing"
        />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[14px] font-semibold text-bone-50">
            {invite.name}
          </span>
          <span className="flex items-center gap-2 text-[12.5px] text-ash-300">
            {failed ? (
              <span className={calm ? "truncate" : "truncate text-flare-300"}>
                {t(failed, { name: invite.name })}
              </span>
            ) : (
              <>
                <span className="truncate">{t("host.invite.line")}</span>
                <InviteCount invite={invite} className="shrink-0 text-ash-400" />
              </>
            )}
          </span>
        </span>
        <WorldJoinButton invite={invite} />
        {/* Waits while the join is under way, in its place: nothing beside it moves. */}
        <IconButton
          label={t("small10.menu.dismissInvite")}
          icon={<X size={16} />}
          size="sm"
          disabled={active}
          className="transition-opacity"
          onClick={() => dismissWorldInvite(invite)}
          onKeyDown={(event) => {
            openMenuFromKey(event);
          }}
        />
      </div>
    </ContextMenu>
  );
}

/**
 * Home's line (the friends strip's slot, before every other line): the friend's face, "Mert seni
 * dünyasına çağırıyor", "Katıl" and ×, which closes the line here (the invite stays in the panel).
 */
export function HomeWorldLine({ invite }: { invite: WorldInvite }) {
  const { t } = useTranslation();
  const items = useInviteItems(invite, "home");
  const openPanel = useSocialUi((s) => s.openPanel);
  const closeHome = useHostUi((s) => s.closeHome);
  // After missing mods the line says why its button copies the install (Home has no row below
  // the name for it, and the toast that said it may be long gone when the player comes back).
  const text = retryWithCopy(invite.join)
    ? t("small9.join.homeMissingMods", { name: invite.name })
    : t("host.invite.title", { name: invite.name });
  return (
    <>
      <ContextMenu items={items}>
        <button
          type="button"
          aria-label={`${text}. ${t("social.home.open")}`}
          onClick={() => openPanel({ page: "list" })}
          onKeyDown={(event) => {
            openMenuFromKey(event);
          }}
          className="flex min-w-0 cursor-pointer items-center gap-3 rounded-full py-0.5 pr-2.5 pl-0.5 outline-none transition-colors duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          {/* Round inside the round pill, 7 px from its edge all round (SMALL10b), as on the rest
              of Home's lines. */}
          <Faces
            sources={[
              {
                id: invite.from,
                name: invite.name,
                avatar: invite.avatar,
                avatarUrl: invite.avatarUrl,
              },
            ]}
            size={26}
            round
          />
          <span className="truncate text-[13.5px] font-medium text-bone-50">{text}</span>
        </button>
      </ContextMenu>
      <WorldJoinButton invite={invite} glyph />
      <IconButton
        size="sm"
        label={t("small10.menu.hideInvite")}
        icon={<X size={13} weight="bold" />}
        onClick={() => closeHome(invite.session)}
        className="shrink-0 rounded-full"
      />
    </>
  );
}
