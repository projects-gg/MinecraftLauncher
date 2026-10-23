// The open world's sheet (HOST1, host1.md §6.1: "Yönet" → the right sheet): the world, "Dünyan açık"
// and how many are in it ("3/8"), the friends invited with where each one is ("Dünyada",
// "Bağlanıyor", "Çağrıldı"), "Davet et", the session's mode and commands, the one line about voice
// chat, and "Paylaşımı kapat" at the bottom, which asks first while friends are in the world (they
// go back to the main menu). Each guest has the person menu ("Dünyadan çıkar", "Rapor et"; right
// click, Shift+F10, the menu key or "…"). While the world opens the sheet says where it is; when
// it could not open, its two ways out. Rows that come and go fade in place, the others glide.
import { PaperPlaneTilt, X } from "@phosphor-icons/react";
import {
  Button,
  Chip,
  cn,
  EntranceGate,
  focusPanel,
  GatedMotion,
  IconButton,
  Progress,
  scrimMotion,
  sheetMotion,
  useInteractionMotion,
} from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { Dialog } from "radix-ui";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { enter, glide, leave, rise, useSocialMotion } from "@/features/social/motion";
import { PersonContextMenu, PersonMoreButton } from "@/features/social/person-menu";
import { SocialFace } from "@/features/social/social-face";
import type { HostSession, WorldGuest } from "@/lib/api/host-types";
import { cancelOpening, dismissFailure, meetOnProjects, retryOpen, stopSharing } from "./actions";
import { GuestStateText, PeopleCount, useWorldIcon, WorldMark } from "./parts";
import { guestOrder, sessionPeople } from "./rules";
import { useHostUi } from "./store";

function GuestRow({ guest }: { guest: WorldGuest }) {
  const { t } = useTranslation();
  const person = {
    id: guest.friend,
    name: guest.name,
    avatar: guest.avatar,
    avatarUrl: guest.avatarUrl,
  };
  return (
    <PersonContextMenu person={person} place={{ from: "list" }}>
      <div className="group/guest flex items-center gap-3 rounded-[var(--radius-panel)] py-1.5 pr-1 pl-2 transition-colors duration-150 hover:bg-white/[0.045] data-[state=open]:bg-white/[0.06]">
        <span
          className={cn(
            "flex transition-opacity duration-300",
            guest.state === "in" ? "opacity-100" : "opacity-70",
          )}
        >
          <SocialFace source={guest} size={36} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[14px] font-semibold text-bone-50">
            {guest.name}
          </span>
          {guest.gameName ? (
            <span className="block truncate text-[12px] text-ash-400" data-selectable>
              {t("host.guest.as", { name: guest.gameName })}
            </span>
          ) : null}
        </span>
        <GuestStateText state={guest.state} />
        <PersonMoreButton
          person={person}
          place={{ from: "list" }}
          className="opacity-0 transition-opacity duration-150 group-hover/guest:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100"
        />
      </div>
    </PersonContextMenu>
  );
}

function StatusChip({ session }: { session: HostSession }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const tone = session.phase === "open" ? "jade" : session.phase === "failed" ? "flare" : "gold";
  return (
    <span className="relative inline-grid">
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span key={session.phase} className="[grid-area:1/1]" {...rise(reduce, 4, 0.2)}>
          <Chip tone={tone}>{t(`host.sheet.${session.phase}`)}</Chip>
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

function OpeningBody({ session }: { session: Extract<HostSession, { phase: "opening" }> }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const label = t(`host.sheet.step.${session.step}`);
  return (
    <div className="rounded-[var(--radius-panel)] bg-obsidian-950/40 px-3.5 py-3 ring-1 ring-white/7">
      <div className="relative mb-2 h-[18px] overflow-hidden text-[13px] text-bone-100">
        <AnimatePresence initial={false} mode="popLayout">
          <motion.span key={session.step} className="absolute inset-x-0" {...rise(reduce, 5, 0.22)}>
            {label}
          </motion.span>
        </AnimatePresence>
      </div>
      <Progress
        value={session.step === "backup" ? (session.percent ?? 0) : undefined}
        label={label}
      />
      {/* A large world's backup can take a while: until the game starts it may be left. */}
      {session.step === "backup" ? (
        <div className="mt-2.5 flex justify-end">
          <Button size="sm" variant="ghost" onClick={cancelOpening}>
            {t("fix2.connect.cancel")}
          </Button>
        </div>
      ) : null}
    </div>
  );
}

function FailedBody({ session }: { session: Extract<HostSession, { phase: "failed" }> }) {
  const { t } = useTranslation();
  return (
    <div className="rounded-[var(--radius-panel)] bg-flare-400/[0.07] px-3.5 py-3 ring-1 ring-flare-400/20">
      <p className="text-[13.5px] text-bone-100">{t(`host.failed.${session.reason}`)}</p>
      <div className="mt-3 flex flex-wrap justify-end gap-2">
        <Button size="sm" variant="ghost" onClick={dismissFailure}>
          {t("host.sheet.dismiss")}
        </Button>
        {session.guests.length ? (
          <Button
            size="sm"
            variant="secondary"
            onClick={() => void meetOnProjects(session.guests.map((g) => g.friend))}
          >
            {t("host.sheet.meet")}
          </Button>
        ) : null}
        {session.reason === "unavailable" ? (
          <Button size="sm" variant="primary" onClick={() => retryOpen(session)}>
            {t("host.sheet.retry")}
          </Button>
        ) : null}
      </div>
    </div>
  );
}

function Guests({ session }: { session: HostSession }) {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const openPicker = useHostUi((s) => s.openPicker);
  const guests = guestOrder(session.guests);
  const full = session.guests.length >= session.max;
  return (
    <section className="mt-4">
      <div className="mb-1 flex h-8 items-center gap-2 px-1">
        <h3 className="flex-1 text-[12.5px] font-medium text-ash-400">{t("host.sheet.guests")}</h3>
        {session.phase === "open" ? (
          <Button
            size="sm"
            variant="secondary"
            icon={<PaperPlaneTilt size={15} className="text-gold-300" />}
            disabled={full}
            onClick={() => openPicker({ kind: "more" })}
          >
            {t("host.sheet.invite")}
          </Button>
        ) : null}
      </div>
      {guests.length === 0 ? (
        <p className="px-2 py-4 text-[13px] text-ash-400">{t("host.sheet.none")}</p>
      ) : (
        <ul className="relative flex flex-col">
          <AnimatePresence initial={false} mode="popLayout">
            {guests.map((guest) => (
              <motion.li
                key={guest.friend}
                layout={layout ? "position" : false}
                transition={{ layout: glide }}
                initial={{ opacity: 0, y: reduce ? 0 : 6 }}
                animate={{ opacity: 1, y: 0, transition: enter(0.26) }}
                exit={{ opacity: 0, transition: leave(0.22) }}
              >
                <GuestRow guest={guest} />
              </motion.li>
            ))}
          </AnimatePresence>
        </ul>
      )}
    </section>
  );
}

function StopFooter({ session }: { session: Extract<HostSession, { phase: "open" }> }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const inWorld = session.guests.some((g) => g.state === "in");
  const stop = async () => {
    setBusy(true);
    await stopSharing();
    setBusy(false);
  };
  return (
    <footer className="relative border-t border-white/8 px-5 py-3.5">
      <AnimatePresence initial={false} mode="popLayout">
        {asking ? (
          <motion.div key="ask" {...rise(reduce, 4, 0.2)}>
            <p className="text-[13px] leading-snug text-bone-100">{t("host.sheet.stopAsk")}</p>
            <div className="mt-2.5 flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setAsking(false)}>
                {t("fix2.connect.cancel")}
              </Button>
              <Button size="sm" variant="danger" disabled={busy} onClick={() => void stop()}>
                {t("host.sheet.stop")}
              </Button>
            </div>
          </motion.div>
        ) : (
          <motion.div key="stop" {...rise(reduce, 4, 0.2)}>
            <Button
              variant="danger"
              className="w-full"
              disabled={busy}
              onClick={() => (inWorld ? setAsking(true) : void stop())}
            >
              {t("host.sheet.stop")}
            </Button>
          </motion.div>
        )}
      </AnimatePresence>
    </footer>
  );
}

function SheetBody({ session }: { session: HostSession }) {
  const { t } = useTranslation();
  const icon = useWorldIcon(session);
  const people = sessionPeople(session);
  return (
    <>
      <header className="flex items-center gap-3.5 border-b border-white/8 px-5 py-4">
        <WorldMark size={48} iconUrl={icon} />
        <div className="min-w-0 flex-1">
          <Dialog.Title className="truncate font-display text-[18px] leading-tight font-semibold text-bone-50">
            {session.worldName}
          </Dialog.Title>
          <div className="mt-1.5 flex items-center gap-2.5">
            <StatusChip session={session} />
            {session.phase === "open" ? (
              <PeopleCount
                count={people.count}
                max={people.max}
                label={t("host.sheet.countLabel", people)}
                className="text-[13px] text-bone-100"
              />
            ) : null}
          </div>
        </div>
        <Dialog.Close asChild>
          <IconButton label={t("common.close")} icon={<X size={18} />} size="sm" />
        </Dialog.Close>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pt-4 pb-6">
        {session.phase === "opening" ? <OpeningBody session={session} /> : null}
        {session.phase === "failed" ? <FailedBody session={session} /> : null}
        <Guests session={session} />
        {session.phase === "open" ? (
          <div className="mt-5 space-y-2.5 px-1">
            <div className="flex flex-wrap gap-1.5">
              <Chip>{t(`host.modes.${session.mode}`)}</Chip>
              <Chip>
                {session.commands ? t("host.sheet.commandsOn") : t("host.sheet.commandsOff")}
              </Chip>
            </div>
            <p className="text-[12.5px] text-ash-400">{t("host.sheet.voice")}</p>
          </div>
        ) : null}
      </div>
      {session.phase === "open" ? <StopFooter session={session} /> : null}
    </>
  );
}

export function WorldSheet() {
  const sheet = useHostUi((s) => s.sheet);
  const close = useHostUi((s) => s.closeSheet);
  const session = useHostUi((s) => s.view.session);
  const animate = useInteractionMotion();
  // The sheet keeps the last session while it slides away.
  const last = useRef<HostSession | null>(null);
  if (session) last.current = session;
  const shown = session ?? last.current;
  const open = sheet && session !== null;
  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && close()}>
      <AnimatePresence>
        {open && shown ? (
          <EntranceGate key="sheet">
            <Dialog.Portal forceMount>
              <Dialog.Overlay asChild forceMount>
                <GatedMotion.div
                  className="fixed inset-0 z-30 bg-obsidian-950/55"
                  {...scrimMotion(!animate)}
                />
              </Dialog.Overlay>
              <Dialog.Content
                asChild
                forceMount
                aria-describedby={undefined}
                onOpenAutoFocus={(event) => {
                  event.preventDefault();
                  focusPanel(event.currentTarget as HTMLElement);
                }}
                // Toasts live outside the sheet: answering one must not close it.
                onPointerDownOutside={(event) => {
                  if ((event.target as Element | null)?.closest?.("[data-sonner-toaster]")) {
                    event.preventDefault();
                  }
                }}
                onFocusOutside={(event) => {
                  if ((event.target as Element | null)?.closest?.("[data-sonner-toaster]")) {
                    event.preventDefault();
                  }
                }}
              >
                <GatedMotion.aside
                  tabIndex={-1}
                  className="glass-strong fixed top-0 right-0 bottom-0 z-40 flex w-[min(440px,94vw)] flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none"
                  {...sheetMotion(!animate)}
                >
                  <SheetBody session={shown} />
                </GatedMotion.aside>
              </Dialog.Content>
            </Dialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}
