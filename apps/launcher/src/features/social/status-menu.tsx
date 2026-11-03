// The player's own card in the panel header: picture with presence, name ("Sınırlı hesap" beside it
// while the Discord account is new), and the status picker (Çevrimiçi / Rahatsız etmeyin /
// Görünmez). The chosen status shows at once and settles when the service confirms; a failure puts
// the old one back. The panel's menu changes it too. With crew tags (TEAM6) the tag the player
// wears sits before the name (its card as on anyone's), and the picker's "Ekip etiketi ▸" puts one
// on, switches or takes it off ("Etiket yok"). The player's profile badges (BADGES-UI) show as
// others see them: the first beside the name, its tooltip naming them all (the picker is for the
// status; friends settings' "Rozetler" lists them with their switches).
//
// SOC-NOTE §3.5: the menu's "Not ekle" / "Notu düzenle" opens the note's dialog, and the player's
// own note takes the status's words in the picker (Discord's way: the dot keeps the status, the
// tooltip and the screen reader say it), in the same line: nothing moves when it comes or goes.
//
// SOC-NEXT §3.3: "Rahatsız etmeyin" and "Görünmez" open a short list ("1 saat", "8 saat", "Yarın
// sabaha kadar", "Ben açana kadar"; the crew mute's words); a timed one goes back to "Çevrimiçi"
// by itself (next/host.tsx) and the status's tooltip says until when ("17:30'a kadar").
import { CaretDown, CaretRight, Check, NotePencil, TagSimple } from "@phosphor-icons/react";
import { cn, easeOutExpo, returnFocusAfterKeys, Tooltip } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { DropdownMenu } from "radix-ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { SocialMe, StatusMode } from "@/lib/api/social-types";
import i18n from "@/lib/i18n";
import { attempt } from "./actions";
import { useMyBadge, wearTag } from "./badge-actions";
import { tagChoices } from "./badges";
import { BadgedName } from "./crew-badge";
import { untilText } from "./crew-notify";
import { LimitedChip } from "./limited-chip";
import { dayMonth } from "./limits";
import { swap, useSocialMotion } from "./motion";
import { useSocNext } from "./next/flag";
import { useSocNextPrefs } from "./next/prefs";
import { STATUS_PRESETS, statusUntil } from "./next/status-timer";
import { askNote } from "./note/note-dialog";
import { useNotesOn, useOwnNote } from "./note/notes";
import { PresenceDot, SocialAvatar } from "./social-face";
import { useSocialUi } from "./store";
import { CrewMark, TeamTag } from "./team-emblem";

/** "Etiket yok" in the tag picker (Radix menus take no empty value). */
const NO_TAG = "none";

export const STATUSES: StatusMode[] = ["online", "dnd", "invisible"];

export const dotOf = (status: StatusMode) =>
  status === "online" ? "launcher" : status === "dnd" ? "dnd" : "invisible";

/** Sets the player's status: shown at once, the old one back if the service refuses. */
export function changeStatus(next: StatusMode) {
  const ui = useSocialUi.getState();
  if (next === (ui.statusPending ?? ui.view.me?.status)) return;
  ui.setStatusPending(next);
  void attempt(() => api.social.setStatus(next)).then((done) => {
    useSocialUi.getState().setStatusPending(null);
    if (done !== undefined) {
      announce(i18n.t("social.status.changed", { status: i18n.t(`social.status.${next}`) }));
    }
  });
}

const itemClass =
  "flex h-11 cursor-pointer items-center gap-3 rounded-md px-2.5 text-[13.5px] text-bone-100 outline-none select-none data-[highlighted]:bg-white/8";

/** SOC-NEXT: a status for a while ("Ben açana kadar": no end), shown at once as any status. */
export function changeStatusFor(
  next: "dnd" | "invisible",
  preset: (typeof STATUS_PRESETS)[number],
) {
  changeStatus(next);
  const until = statusUntil(preset, Date.now());
  useSocNextPrefs.getState().setStatusTimer(until === null ? null : { status: next, until });
}

/** The menus' floating panel (the status picker's and its lists). */
const panelClass =
  "z-50 rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-1.5 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none";

export function MeCard({ me }: { me: SocialMe }) {
  const { t } = useTranslation();
  const calm = useCalm();
  const { reduce } = useSocialMotion();
  const [open, setOpen] = useState(false);
  const pending = useSocialUi((s) => s.statusPending);
  const status = pending ?? me.status;
  const timed = useSocNext();
  const timer = useSocNextPrefs((s) => (timed ? s.statusTimer : null));
  // "17:30'a kadar" while a timed status holds.
  const until = timer && timer.status === status ? untilText(t, timer.until, Date.now()) : null;
  // SOC-NOTE: the player's own note in the picker's words.
  const ownNote = useOwnNote();
  const statusWords = until
    ? t("socNext.status.until", { status: t(`social.status.${status}`), until })
    : t(`social.status.${status}`);
  const choose = (next: StatusMode) => {
    if (timed) useSocNextPrefs.getState().setStatusTimer(null);
    changeStatus(next);
  };
  const badge = useMyBadge();
  // "Ekip etiketi ▸" while the service has tags and the player is in a crew.
  const tags = useSocialUi((s) => s.view.crew.badges);
  const crews = useSocialUi((s) => s.view.crews);
  const choices = tags ? tagChoices(crews, Date.now()) : [];
  // SOC-NOTE: "Not ekle" while notes are on (SOC-NEXT's switch and the service's).
  const notes = useNotesOn();

  return (
    <div className="flex min-w-0 flex-1 items-center gap-3">
      <SocialAvatar
        source={me}
        size={40}
        state={status === "invisible" ? undefined : "launcher"}
        dnd={status === "dnd"}
      />
      <div className="min-w-0">
        <BadgedName
          name={me.name}
          badge={badge}
          wearer={me.id}
          marks={me.marks}
          chip="button"
          nameClassName="text-[15px] leading-tight font-semibold text-bone-50"
        />
        {/* A limited account's chip sits by the status, where the name keeps its whole width. */}
        <div className="mt-0.5 flex min-w-0 items-center gap-1.5">
          <DropdownMenu.Root open={open} onOpenChange={setOpen}>
            <Tooltip
              content={ownNote ? statusWords : (until ?? "")}
              disabled={(!until && !ownNote) || open}
            >
              <DropdownMenu.Trigger asChild>
                <button
                  type="button"
                  aria-label={
                    ownNote
                      ? `${t("social.status.change")}, ${statusWords}, ${t("socNote.note.mine", { text: ownNote.text })}`
                      : until
                        ? `${t("social.status.change")}, ${statusWords}`
                        : t("social.status.change")
                  }
                  className={cn(
                    "group -ml-1.5 flex h-6 cursor-pointer items-center gap-1.5 rounded-md px-1.5 text-[12.5px] text-ash-300 outline-none transition-colors hover:bg-white/6 hover:text-bone-100 focus-visible:ring-2 focus-visible:ring-gold-300 data-[state=open]:text-bone-100",
                    ownNote ? "min-w-0" : "shrink-0",
                  )}
                >
                  <PresenceDot kind={dotOf(status)} size={8} />
                  <span className={cn("relative inline-flex", ownNote && "min-w-0")}>
                    <AnimatePresence mode="popLayout" initial={false}>
                      <motion.span
                        key={ownNote ? `note:${ownNote.text}` : status}
                        className={ownNote ? "truncate text-bone-100" : undefined}
                        {...swap(reduce)}
                      >
                        {ownNote ? ownNote.text : t(`social.status.${status}`)}
                      </motion.span>
                    </AnimatePresence>
                  </span>
                  <CaretDown
                    size={10}
                    weight="bold"
                    aria-hidden
                    className="text-ash-400 transition-transform duration-200 group-data-[state=open]:rotate-180"
                  />
                </button>
              </DropdownMenu.Trigger>
            </Tooltip>
            <AnimatePresence>
              {open ? (
                <DropdownMenu.Portal forceMount>
                  <DropdownMenu.Content
                    onCloseAutoFocus={returnFocusAfterKeys}
                    forceMount
                    asChild
                    align="start"
                    sideOffset={6}
                    collisionPadding={12}
                    loop
                  >
                    <motion.div
                      className="z-50 w-[232px] rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-1.5 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
                      style={{
                        transformOrigin: "var(--radix-dropdown-menu-content-transform-origin)",
                      }}
                      initial={calm({ opacity: 0, scale: 0.96, y: -4 })}
                      animate={calm({
                        opacity: 1,
                        scale: 1,
                        y: 0,
                        transition: { duration: 0.18, ease: easeOutExpo },
                      })}
                      exit={calm({ opacity: 0, scale: 0.98, transition: { duration: 0.12 } })}
                    >
                      <DropdownMenu.RadioGroup
                        value={status}
                        onValueChange={(value) => choose(value as StatusMode)}
                      >
                        {STATUSES.map((option) =>
                          timed && option !== "online" ? (
                            // SOC-NEXT: the status for a while, its four choices to the side.
                            <DropdownMenu.Sub key={option}>
                              <DropdownMenu.SubTrigger
                                className={cn(itemClass, "data-[state=open]:bg-white/8")}
                              >
                                <span className="flex w-5 justify-center">
                                  <PresenceDot kind={dotOf(option)} size={10} />
                                </span>
                                <span className="flex-1">{t(`social.status.${option}`)}</span>
                                {status === option ? (
                                  <Check size={15} weight="bold" className="text-bone-100" />
                                ) : null}
                                <CaretRight
                                  size={13}
                                  weight="bold"
                                  aria-hidden
                                  className="shrink-0 text-ash-400"
                                />
                              </DropdownMenu.SubTrigger>
                              <DropdownMenu.Portal>
                                <DropdownMenu.SubContent
                                  sideOffset={8}
                                  alignOffset={-6}
                                  collisionPadding={12}
                                  className={cn("pop-motion w-[208px]", panelClass)}
                                >
                                  {STATUS_PRESETS.map((preset) => (
                                    <DropdownMenu.Item
                                      key={preset}
                                      className={cn(itemClass, "h-10")}
                                      onSelect={() =>
                                        changeStatusFor(option as "dnd" | "invisible", preset)
                                      }
                                    >
                                      {preset === "forever"
                                        ? t("socNext.status.forever")
                                        : t(`teams.mute.presets.${preset}`)}
                                    </DropdownMenu.Item>
                                  ))}
                                </DropdownMenu.SubContent>
                              </DropdownMenu.Portal>
                            </DropdownMenu.Sub>
                          ) : (
                            <DropdownMenu.RadioItem
                              key={option}
                              value={option}
                              className={itemClass}
                            >
                              <span className="flex w-5 justify-center">
                                <PresenceDot kind={dotOf(option)} size={10} />
                              </span>
                              <span className="flex-1">{t(`social.status.${option}`)}</span>
                              <DropdownMenu.ItemIndicator>
                                <Check size={15} weight="bold" className="text-bone-100" />
                              </DropdownMenu.ItemIndicator>
                            </DropdownMenu.RadioItem>
                          ),
                        )}
                      </DropdownMenu.RadioGroup>
                      {notes ? (
                        <>
                          <DropdownMenu.Separator className="my-1.5 h-px bg-white/8" />
                          <DropdownMenu.Item className={itemClass} onSelect={() => askNote()}>
                            <span className="flex w-5 justify-center">
                              <NotePencil size={15} className="text-ash-300" aria-hidden />
                            </span>
                            <span className="flex-1">
                              {ownNote ? t("socNote.note.edit") : t("socNote.note.add")}
                            </span>
                          </DropdownMenu.Item>
                        </>
                      ) : null}
                      {choices.length ? (
                        <>
                          <DropdownMenu.Separator className="my-1.5 h-px bg-white/8" />
                          <DropdownMenu.Sub>
                            <DropdownMenu.SubTrigger
                              className={cn(itemClass, "data-[state=open]:bg-white/8")}
                            >
                              <span className="flex w-5 justify-center">
                                <TagSimple size={15} className="text-ash-300" aria-hidden />
                              </span>
                              <span className="flex-1">{t("team6.choose")}</span>
                              {badge ? (
                                <TeamTag tag={badge.tag} color={badge.color} size="sm" />
                              ) : null}
                              <CaretRight
                                size={13}
                                weight="bold"
                                aria-hidden
                                className="shrink-0 text-ash-400"
                              />
                            </DropdownMenu.SubTrigger>
                            <DropdownMenu.Portal>
                              <DropdownMenu.SubContent
                                sideOffset={8}
                                alignOffset={-6}
                                collisionPadding={12}
                                className="pop-motion z-50 w-[256px] rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-1.5 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
                              >
                                <DropdownMenu.RadioGroup
                                  value={badge?.crew ?? NO_TAG}
                                  onValueChange={(value) =>
                                    void wearTag(value === NO_TAG ? null : value)
                                  }
                                >
                                  <DropdownMenu.RadioItem value={NO_TAG} className={itemClass}>
                                    <span className="flex w-5" aria-hidden />
                                    <span className="flex-1">{t("team6.noTag")}</span>
                                    <DropdownMenu.ItemIndicator>
                                      <Check size={15} weight="bold" className="text-bone-100" />
                                    </DropdownMenu.ItemIndicator>
                                  </DropdownMenu.RadioItem>
                                  {choices.map(({ crew, main, blocked }) => (
                                    <DropdownMenu.RadioItem
                                      key={crew.id}
                                      value={crew.id}
                                      disabled={blocked !== null}
                                      className={cn(
                                        itemClass,
                                        "data-[disabled]:cursor-default data-[disabled]:opacity-45",
                                      )}
                                    >
                                      <span className="flex w-5 justify-center">
                                        <CrewMark look={crew} size={18} />
                                      </span>
                                      <span className="min-w-0 flex-1">
                                        <span className="block truncate">{crew.name}</span>
                                        {blocked || main ? (
                                          <span className="block truncate text-[12px] leading-tight text-ash-400">
                                            {blocked === "hidden"
                                              ? t("team6.hiddenChip")
                                              : blocked === "locked"
                                                ? t("team6.lockedUntil", {
                                                    date: dayMonth(crew.textLockedUntil ?? 0),
                                                  })
                                                : t("crews.main")}
                                          </span>
                                        ) : null}
                                      </span>
                                      <TeamTag tag={crew.tag} color={crew.color} size="sm" />
                                      <span className="flex w-4 justify-end">
                                        <DropdownMenu.ItemIndicator>
                                          <Check
                                            size={15}
                                            weight="bold"
                                            className="text-bone-100"
                                          />
                                        </DropdownMenu.ItemIndicator>
                                      </span>
                                    </DropdownMenu.RadioItem>
                                  ))}
                                </DropdownMenu.RadioGroup>
                              </DropdownMenu.SubContent>
                            </DropdownMenu.Portal>
                          </DropdownMenu.Sub>
                        </>
                      ) : null}
                    </motion.div>
                  </DropdownMenu.Content>
                </DropdownMenu.Portal>
              ) : null}
            </AnimatePresence>
          </DropdownMenu.Root>
          <LimitedChip me={me} />
        </div>
      </div>
    </div>
  );
}
