// One crew of "Ekip bul" (CREW-DISCOVER): its mark, name and tag, size, the leader's text, its
// tags, and one action in a fixed slot: "Katıl" (an open crew), "Katılma isteği gönder" (one that
// approves newcomers), or what is true instead ("Ekibindesin" with "Ekibe git", "İsteğin bekliyor",
// "Ekip dolu", "Ekip sınırındasın"). The action is secondary: a page of cards has no gold. The
// "⋯" (and a right click, Shift+F10) holds "Ekibi rapor et". Nothing moves but what the player
// starts: the action's words cross-fade in their slot.
import { CheckCircle, Clock, DotsThree, Flag } from "@phosphor-icons/react";
import { Button, Chip, ContextMenu, cn, IconButton, Menu } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { discoverApi } from "@/lib/api/discover-api";
import type { DiscoverCrew } from "@/lib/api/discover-types";
import { attempt } from "../actions";
import { openCrewPage, sayJoined } from "../badge-actions";
import { swap, useSocialMotion } from "../motion";
import { useSocialUi } from "../store";
import { CrewMark, TeamTag } from "../team-emblem";
import { dropFromListing } from "./listing";

/** A crew holds 30 (the service's limit). */
const FULL = 30;

type Slot = "join" | "ask" | "member" | "waiting" | "full" | "limit";

export function DiscoverCard({ crew, canAdd }: { crew: DiscoverCrew; canAdd: boolean }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const member = useSocialUi((s) => s.view.crews.some((mine) => mine.id === crew.id));
  const waiting = useSocialUi((s) => s.view.teamRequests.some((r) => r.teamId === crew.id));
  const openReport = useSocialUi((s) => s.openReport);
  const [busy, setBusy] = useState(false);
  const slot: Slot = member
    ? "member"
    : waiting
      ? "waiting"
      : crew.members >= FULL
        ? "full"
        : !canAdd
          ? "limit"
          : crew.join === "open"
            ? "join"
            : "ask";

  const join = async () => {
    if (busy) return;
    setBusy(true);
    const result = await attempt(() => discoverApi.join(crew.id));
    setBusy(false);
    if (result === "joined") {
      sayJoined(t("teams.joined", { name: crew.name }), () => crew.id, {
        label: t("discover.card.goTo"),
        onClick: () => openCrewPage(crew.id),
      });
    } else if (result === "requested") {
      announce(t("crewWave2.join.sent"));
    } else if (result === "gone") {
      announce(t("discover.gone"));
      dropFromListing(crew.id);
    }
  };

  const report = () =>
    openReport({
      target: { kind: "crew", id: crew.id, discover: true },
      name: crew.name,
      crew: { emblem: crew.emblem, color: crew.color, frame: crew.frame },
      text: crew.text,
    });
  const menuItems = [
    { label: t("discover.card.report"), icon: <Flag size={15} />, onSelect: report },
  ];

  return (
    <ContextMenu items={menuItems} title={crew.name}>
      <article
        aria-label={crew.name}
        className="group/card rounded-[var(--radius-panel)] bg-obsidian-950/40 px-4 pt-3.5 pb-3 ring-1 ring-white/8 transition-[box-shadow] duration-150 hover:ring-white/14"
      >
        <header className="flex items-start gap-3">
          <CrewMark look={crew} size={40} className="mt-0.5" />
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-2">
              <h3 className="truncate text-[14.5px] leading-[1.25] font-semibold text-bone-50">
                {crew.name}
              </h3>
              <TeamTag tag={crew.tag} color={crew.color} />
            </div>
            <p className="mt-0.5 text-[12.5px] text-ash-400">
              {crew.active
                ? t("discover.card.membersActive", { count: crew.members })
                : t("discover.card.members", { count: crew.members })}
            </p>
          </div>
          <Menu
            items={menuItems}
            title={crew.name}
            trigger={
              <IconButton
                size="sm"
                label={t("discover.card.menu", { name: crew.name })}
                icon={<DotsThree size={18} weight="bold" />}
                className="-mr-2 opacity-0 transition-opacity duration-150 group-hover/card:opacity-100 group-focus-within/card:opacity-100 data-[state=open]:opacity-100"
              />
            }
          />
        </header>
        <p className="mt-2.5 text-[13.5px] leading-normal text-bone-100 break-words">{crew.text}</p>
        <footer className="mt-3 flex flex-wrap items-end gap-x-3 gap-y-2">
          {crew.tags.length > 0 ? (
            <ul
              className="flex min-w-0 flex-1 flex-wrap gap-1.5"
              aria-label={t("discover.tagsLabel")}
            >
              {crew.tags.map((tag) => (
                <li key={tag}>
                  <Chip className="font-medium text-ash-300">{t(`discover.tags.${tag}`)}</Chip>
                </li>
              ))}
            </ul>
          ) : (
            <span className="flex-1" />
          )}
          <div className="ml-auto flex h-8 items-center">
            <AnimatePresence mode="popLayout" initial={false}>
              <motion.div key={slot} className="flex items-center gap-1" {...swap(reduce, 0.18)}>
                {slot === "join" || slot === "ask" ? (
                  <Button size="sm" variant="secondary" disabled={busy} onClick={() => void join()}>
                    {slot === "join" ? t("discover.card.join") : t("discover.card.ask")}
                  </Button>
                ) : slot === "member" ? (
                  <>
                    <span className="flex items-center gap-1.5 text-[12.5px] text-ash-300">
                      <CheckCircle size={15} weight="fill" className="text-bone-100" aria-hidden />
                      {t("discover.card.member")}
                    </span>
                    <Button size="sm" variant="ghost" onClick={() => openCrewPage(crew.id)}>
                      {t("discover.card.goTo")}
                    </Button>
                  </>
                ) : (
                  <span
                    className={cn(
                      "flex items-center gap-1.5 pr-1 text-[12.5px] text-ash-300",
                      slot === "waiting" && "text-bone-100",
                    )}
                  >
                    {slot === "waiting" ? <Clock size={15} aria-hidden /> : null}
                    {slot === "waiting"
                      ? t("discover.card.waiting")
                      : slot === "full"
                        ? t("discover.card.full")
                        : t("discover.card.limit")}
                  </span>
                )}
              </motion.div>
            </AnimatePresence>
          </div>
        </footer>
      </article>
    </ContextMenu>
  );
}
