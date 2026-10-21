// Join requests on screen (TEAM5). Whoever answers them in a crew (`admit`: its leader, and its
// moderators unless the leader keeps it) sees the waiting ones above the members: who asked and
// when, "Kabul et", × (declines, silently, as friend requests are) and "…" with "Engelle" (asked
// once, in place) and "Rapor et"; the same menu opens on right click. The player's own waiting
// requests sit on "Ekip kur ya da katıl" with "Geri çek". A row that goes fades while the rows
// under it glide up; nothing moves by itself.
import { Check, DotsThree, Flag, Prohibit, X } from "@phosphor-icons/react";
import { Button, ContextMenu, IconButton, Menu, type MenuItem } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { JoinRequest, SocialTeam, TeamRequest } from "@/lib/api/social-types";
import { formatRelative } from "@/lib/format";
import { attempt } from "./actions";
import { answersRequests, stillWaits, waitingRequests } from "./crew-joining";
import { enter, glide, leave, rise, useSocialMotion } from "./motion";
import { SocialAvatar } from "./social-face";
import { useSocialUi } from "./store";
import { CrewMark, TeamTag } from "./team-emblem";

const ago = (sentAt: number) => formatRelative(new Date(sentAt * 1000).toISOString());

function RequestRow({ team, request }: { team: SocialTeam; request: JoinRequest }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const [busy, setBusy] = useState(false);
  const [asking, setAsking] = useState(false);

  const answer = async (accept: boolean) => {
    setBusy(true);
    const done = await attempt(() => api.social.answerTeamRequest(team.id, request.id, accept));
    setBusy(false);
    if (done === undefined) return;
    announce(
      accept
        ? t("crewWave2.requests.accepted", { name: request.name })
        : t("crewWave2.requests.declined"),
    );
  };
  const block = async () => {
    setBusy(true);
    const done = await attempt(() => api.social.block(request.id));
    setBusy(false);
    setAsking(false);
    if (done !== undefined) announce(t("social.actions.blocked", { name: request.name }));
  };
  const report = () =>
    useSocialUi.getState().openReport({
      target: { kind: "player", id: request.id },
      name: request.name,
      person: request,
    });
  const items: MenuItem[] = [
    {
      label: t("social.actions.accept"),
      icon: <Check size={16} />,
      onSelect: () => void answer(true),
    },
    {
      label: t("social.actions.decline"),
      icon: <X size={16} />,
      onSelect: () => void answer(false),
    },
    {
      label: t("social.actions.block"),
      icon: <Prohibit size={16} />,
      danger: true,
      onSelect: () => setAsking(true),
    },
    { label: t("reports.action"), icon: <Flag size={16} />, onSelect: report },
  ];

  return (
    <ContextMenu items={items}>
      <div className="rounded-[var(--radius-panel)] py-1.5 pr-1 pl-2">
        <div className="flex items-center gap-3">
          <SocialAvatar source={request} size={32} />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[13.5px] font-semibold text-bone-50">
              {request.name}
            </span>
            <span className="block truncate text-[12px] text-ash-400">
              {t("crewWave2.requests.asked", { when: ago(request.sentAt) })}
            </span>
          </span>
          <Button
            size="sm"
            variant="secondary"
            icon={<Check size={14} weight="bold" />}
            disabled={busy}
            onClick={() => void answer(true)}
          >
            {t("social.actions.accept")}
          </Button>
          <IconButton
            size="sm"
            label={t("social.actions.decline")}
            icon={<X size={14} weight="bold" />}
            disabled={busy}
            onClick={() => void answer(false)}
          />
          <Menu
            items={items.slice(2)}
            trigger={
              <IconButton
                size="sm"
                label={t("social.actions.more")}
                icon={<DotsThree size={18} weight="bold" />}
              />
            }
          />
        </div>
        <AnimatePresence initial={false}>
          {asking ? (
            <motion.div key="ask" className="mt-2 pl-11" {...rise(reduce, 4, 0.2)}>
              <p className="text-[13px] leading-snug text-bone-100">
                {t("social.actions.blockAsk")}
              </p>
              <div className="mt-2 flex justify-end gap-2">
                <Button size="sm" variant="ghost" onClick={() => setAsking(false)}>
                  {t("teams.cancel")}
                </Button>
                <Button size="sm" variant="danger" disabled={busy} onClick={() => void block()}>
                  {t("social.actions.block")}
                </Button>
              </div>
            </motion.div>
          ) : null}
        </AnimatePresence>
      </div>
    </ContextMenu>
  );
}

/**
 * The crew's waiting requests, oldest first, for whoever answers them; nothing otherwise. Gold
 * tinted like a call: something waits for the player.
 */
export function CrewRequests({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const features = useSocialUi((s) => s.view.crew);
  const requests = answersRequests(team, features) ? waitingRequests(team, Date.now()) : [];
  return (
    <AnimatePresence initial={false}>
      {requests.length ? (
        <motion.section
          key="requests"
          aria-label={t("crewWave2.requests.title")}
          className="mt-3 rounded-[var(--radius-panel)] bg-gold-500/[0.06] px-2 pt-2.5 pb-1.5 ring-1 ring-gold-400/15"
          {...rise(reduce, 6, 0.26)}
        >
          <h3 className="mb-1 flex items-center gap-2 px-2 text-[12.5px] font-medium text-gold-200">
            {t("crewWave2.requests.title")}
            <span className="font-display tabular-nums">{requests.length}</span>
          </h3>
          <ul className="flex flex-col">
            <AnimatePresence initial={false} mode="popLayout">
              {requests.map((request) => (
                <motion.li
                  key={request.id}
                  layout={layout ? "position" : false}
                  transition={{ layout: glide }}
                  initial={{ opacity: 0, transform: reduce ? "none" : "translateY(6px)" }}
                  animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.26) }}
                  exit={{ opacity: 0, transition: leave(0.2) }}
                >
                  <RequestRow team={team} request={request} />
                </motion.li>
              ))}
            </AnimatePresence>
          </ul>
        </motion.section>
      ) : null}
    </AnimatePresence>
  );
}

/** Under a crew's header in the friend list: how many join requests wait, and "Göster". */
export function JoinsRow({ team, count }: { team: SocialTeam; count: number }) {
  const { t } = useTranslation();
  const openTeam = useSocialUi((s) => s.openTeam);
  const first = waitingRequests(team, Date.now())[0];
  return (
    <div className="mb-1.5 flex items-center gap-3 rounded-[var(--radius-panel)] bg-gold-500/[0.07] py-2 pr-1.5 pl-2 ring-1 ring-gold-400/15">
      {first ? <SocialAvatar source={first} size={32} /> : null}
      <span className="min-w-0 flex-1 truncate text-[13.5px] font-semibold text-bone-50">
        {t("crewWave2.requests.waiting", { count })}
      </span>
      <Button size="sm" variant="secondary" onClick={() => openTeam(undefined, team.id)}>
        {t("social.notices.show")}
      </Button>
    </div>
  );
}

function MyRequestRow({ request }: { request: TeamRequest }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const withdraw = async () => {
    setBusy(true);
    const done = await attempt(() => api.social.withdrawTeamRequest(request.teamId));
    setBusy(false);
    if (done !== undefined) announce(t("crewWave2.join.withdrawn"));
  };
  return (
    <div className="flex items-center gap-3 rounded-[var(--radius-panel)] bg-obsidian-950/40 py-2 pr-2 pl-2.5 ring-1 ring-white/8">
      <CrewMark look={request} size={36} />
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-1.5">
          <span className="truncate text-[13.5px] font-semibold text-bone-50">{request.name}</span>
          <TeamTag tag={request.tag} color={request.color} />
        </span>
        <span className="block truncate text-[12px] text-ash-400">
          {t("crewWave2.mine.sent", { when: ago(request.sentAt) })}
        </span>
      </span>
      <Button size="sm" variant="ghost" disabled={busy} onClick={() => void withdraw()}>
        {t("crewWave2.join.withdraw")}
      </Button>
    </div>
  );
}

/** "Katılma isteklerin": the player's requests that still wait, newest first. */
export function MyRequests() {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const all = useSocialUi((s) => s.view.teamRequests);
  const requests = all.filter((request) => stillWaits(request, Date.now()));
  if (!requests.length) return null;
  return (
    <section className="mb-4">
      <h3 className="mb-2.5 text-[12.5px] font-medium text-ash-400">{t("crewWave2.mine.title")}</h3>
      <ul className="flex flex-col gap-1.5">
        <AnimatePresence initial={false} mode="popLayout">
          {requests.map((request) => (
            <motion.li
              key={request.teamId}
              layout={layout ? "position" : false}
              transition={{ layout: glide }}
              {...rise(reduce, 6, 0.24)}
            >
              <MyRequestRow request={request} />
            </motion.li>
          ))}
        </AnimatePresence>
      </ul>
    </section>
  );
}
