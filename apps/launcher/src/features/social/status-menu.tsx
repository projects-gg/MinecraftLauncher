// The player's own card in the panel header: picture with presence, name ("Sınırlı hesap" beside it
// while the Discord account is new), and the status picker (Çevrimiçi / Rahatsız etmeyin /
// Görünmez). The chosen status shows at once and settles when the service confirms; a failure puts
// the old one back. The panel's menu changes it too.
import { CaretDown, Check } from "@phosphor-icons/react";
import { easeOutExpo, returnFocusAfterKeys } from "@pl/ui";
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
import { LimitedChip } from "./limited-chip";
import { swap, useSocialMotion } from "./motion";
import { PresenceDot, SocialAvatar } from "./social-face";
import { useSocialUi } from "./store";

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

export function MeCard({ me }: { me: SocialMe }) {
  const { t } = useTranslation();
  const calm = useCalm();
  const { reduce } = useSocialMotion();
  const [open, setOpen] = useState(false);
  const pending = useSocialUi((s) => s.statusPending);
  const status = pending ?? me.status;
  const choose = changeStatus;

  return (
    <div className="flex min-w-0 flex-1 items-center gap-3">
      <SocialAvatar
        source={me}
        size={40}
        state={status === "invisible" ? undefined : "launcher"}
        dnd={status === "dnd"}
      />
      <div className="min-w-0">
        <p className="truncate text-[15px] leading-tight font-semibold text-bone-50">{me.name}</p>
        {/* A limited account's chip sits by the status, where the name keeps its whole width. */}
        <div className="mt-0.5 flex min-w-0 items-center gap-1.5">
          <DropdownMenu.Root open={open} onOpenChange={setOpen}>
            <DropdownMenu.Trigger asChild>
              <button
                type="button"
                aria-label={t("social.status.change")}
                className="group -ml-1.5 flex h-6 shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-1.5 text-[12.5px] text-ash-300 outline-none transition-colors hover:bg-white/6 hover:text-bone-100 focus-visible:ring-2 focus-visible:ring-gold-300 data-[state=open]:text-bone-100"
              >
                <PresenceDot kind={dotOf(status)} size={8} />
                <span className="relative inline-flex">
                  <AnimatePresence mode="popLayout" initial={false}>
                    <motion.span key={status} {...swap(reduce)}>
                      {t(`social.status.${status}`)}
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
                        {STATUSES.map((option) => (
                          <DropdownMenu.RadioItem key={option} value={option} className={itemClass}>
                            <span className="flex w-5 justify-center">
                              <PresenceDot kind={dotOf(option)} size={10} />
                            </span>
                            <span className="flex-1">{t(`social.status.${option}`)}</span>
                            <DropdownMenu.ItemIndicator>
                              <Check size={15} weight="bold" className="text-gold-300" />
                            </DropdownMenu.ItemIndicator>
                          </DropdownMenu.RadioItem>
                        ))}
                      </DropdownMenu.RadioGroup>
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
