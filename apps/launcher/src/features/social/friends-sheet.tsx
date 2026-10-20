// The friends panel: a sheet from the right, like the account center. The header carries the
// player's own card and status; pages (list, add, settings) slide in from the side they belong
// to. Signed out it is the opt-in screen; while a saved session is checked, skeletons.
import { CaretLeft, GearSix, UserPlus, UsersThree, X } from "@phosphor-icons/react";
import {
  EntranceGate,
  focusPanel,
  GatedMotion,
  IconButton,
  Skeleton,
  scrimMotion,
  sheetMotion,
  useInteractionMotion,
} from "@pl/ui";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { Dialog } from "radix-ui";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { AddPage } from "./add-page";
import { ConsentPage } from "./consent-page";
import { ListPage } from "./list-page";
import { crossfade, pageVariants, useSocialMotion } from "./motion";
import { openPanelMenuFromKey, PanelContextMenu, PanelMenuButton } from "./panel-menu";
import { SocialSettings } from "./settings-page";
import { MeCard } from "./status-menu";
import { type SocialPage, useSocialUi } from "./store";
import { TeamPage } from "./team-page";

function HeaderSkeleton() {
  return (
    <div aria-hidden className="flex flex-1 items-center gap-3">
      <Skeleton className="size-10 rounded-[var(--radius-head)]" />
      <div className="space-y-1.5">
        <Skeleton className="h-4 w-28" />
        <Skeleton className="h-3 w-20" />
      </div>
    </div>
  );
}

function Header({ page }: { page: SocialPage | "consent" | "restoring" }) {
  const { t } = useTranslation();
  const me = useSocialUi((s) => s.view.me);
  const teams = useSocialUi((s) => s.view.teams);
  const teamName = useSocialUi((s) => s.view.team?.name);
  const goTo = useSocialUi((s) => s.goTo);
  const sub = page === "add" || page === "settings" || page === "team";
  const subTitle =
    page === "add"
      ? t("social.add.title")
      : page === "team"
        ? (teamName ?? t("teams.title"))
        : t("social.settings.title");
  return (
    <header className="flex h-[68px] shrink-0 items-center gap-2 border-b border-white/8 pr-3 pl-5">
      <div className="relative flex min-w-0 flex-1 items-center">
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.div
            key={sub ? page : page === "list" ? "me" : page}
            className="flex min-w-0 flex-1 items-center gap-2"
            {...crossfade()}
          >
            {sub ? (
              <>
                <IconButton
                  label={t("social.actions.back")}
                  icon={<CaretLeft size={18} weight="bold" />}
                  className="-ml-2"
                  onClick={() => goTo("list")}
                />
                <Dialog.Title className="truncate font-display text-[20px] leading-tight font-semibold text-bone-50">
                  {subTitle}
                </Dialog.Title>
              </>
            ) : page === "list" && me ? (
              <>
                <Dialog.Title className="sr-only">{t("social.title")}</Dialog.Title>
                <MeCard me={me} />
              </>
            ) : page === "restoring" ? (
              <>
                <Dialog.Title className="sr-only">{t("social.title")}</Dialog.Title>
                <HeaderSkeleton />
              </>
            ) : (
              <Dialog.Title className="font-display text-[22px] leading-tight font-semibold text-bone-50">
                {t("social.title")}
              </Dialog.Title>
            )}
          </motion.div>
        </AnimatePresence>
      </div>
      {page === "list" && me ? (
        <>
          {teams ? (
            <IconButton
              label={t("teams.title")}
              icon={<UsersThree size={19} />}
              onClick={() => goTo("team")}
            />
          ) : null}
          <IconButton
            label={t("social.actions.add")}
            icon={<UserPlus size={19} />}
            onClick={() => goTo("add")}
          />
          <IconButton
            label={t("social.actions.settings")}
            icon={<GearSix size={19} />}
            onClick={() => goTo("settings")}
          />
          <PanelMenuButton />
        </>
      ) : null}
      <Dialog.Close asChild>
        <IconButton label={t("common.close")} icon={<X size={18} />} />
      </Dialog.Close>
    </header>
  );
}

export function FriendsSheet() {
  const open = useSocialUi((s) => s.open);
  const close = useSocialUi((s) => s.closePanel);
  const page = useSocialUi((s) => s.page);
  const direction = useSocialUi((s) => s.direction);
  const returnFocus = useSocialUi((s) => s.returnFocus);
  const view = useSocialUi((s) => s.view);
  const animate = useInteractionMotion();
  const { reduce } = useSocialMotion();
  const scroller = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const variants = pageVariants(reduce);

  const shown: SocialPage | "consent" | "restoring" =
    view.auth === "signedIn"
      ? view.me && (page !== "team" || view.teams)
        ? page
        : "list"
      : view.auth === "restoring"
        ? "restoring"
        : "consent";

  return (
    <Dialog.Root open={open && view.enabled} onOpenChange={(next) => !next && close()}>
      <AnimatePresence>
        {open && view.enabled ? (
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
                onCloseAutoFocus={(event) => {
                  if (returnFocus?.isConnected) {
                    event.preventDefault();
                    returnFocus.focus();
                  }
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
                  onKeyDown={(event) => openPanelMenuFromKey(event, shown === "list", body.current)}
                  {...sheetMotion(!animate)}
                >
                  <MotionConfig reducedMotion="user">
                    <PanelContextMenu enabled={shown === "list"} bodyRef={body}>
                      <Header page={shown} />
                      <div
                        ref={scroller}
                        className="relative min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain"
                      >
                        <AnimatePresence mode="popLayout" initial={false} custom={direction}>
                          <motion.div
                            key={shown}
                            custom={direction}
                            variants={variants}
                            initial="enter"
                            animate="center"
                            exit="exit"
                            onAnimationStart={() => scroller.current?.scrollTo({ top: 0 })}
                            className="px-4 pt-3 pb-8"
                          >
                            {shown === "consent" ? (
                              <ConsentPage />
                            ) : shown === "restoring" ? (
                              <RestoringBody />
                            ) : shown === "add" ? (
                              <AddPage />
                            ) : shown === "team" ? (
                              <TeamPage />
                            ) : shown === "settings" ? (
                              <div className="px-1">
                                <SocialSettings />
                              </div>
                            ) : (
                              <ListPage />
                            )}
                          </motion.div>
                        </AnimatePresence>
                      </div>
                    </PanelContextMenu>
                  </MotionConfig>
                </GatedMotion.aside>
              </Dialog.Content>
            </Dialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}

function RestoringBody() {
  return (
    <div aria-hidden className="space-y-1 pt-3">
      <Skeleton className="mx-2 mb-3 h-3.5 w-20" />
      {[0, 1, 2, 3].map((i) => (
        <div key={i} className="flex items-center gap-3 px-2 py-2" style={{ opacity: 1 - i * 0.2 }}>
          <Skeleton className="size-9 rounded-[var(--radius-head)]" />
          <div className="flex-1 space-y-1.5">
            <Skeleton className="h-3.5 w-28" />
            <Skeleton className="h-3 w-20" />
          </div>
        </div>
      ))}
    </div>
  );
}
