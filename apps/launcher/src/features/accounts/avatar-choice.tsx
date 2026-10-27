// The face of an offline account while it is being edited: a button that opens the picture
// picker. A pick applies at once (the face cross-fades in the card behind the popover); the
// popover lingers a moment so the brackets can land on the new picture, then closes.
import { PencilSimple } from "@phosphor-icons/react";
import { EntranceGate, easeOutExpo, GatedMotion, PlayerHead, swallowDismissClick } from "@pl/ui";
import { AnimatePresence, useReducedMotion } from "motion/react";
import { Popover } from "radix-ui";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AvatarPicker } from "@/features/avatars/avatar-picker";
import type { AccountSummary } from "@/lib/api";
import { AccountAvatar } from "./account-avatar";
import { useCalm } from "./calm";

/** How long the picker stays after a pick, so the brackets visibly land. */
const LINGER_MS = 260;

export function AvatarChoiceButton({
  account,
  bracketsId,
  onChange,
}: {
  account: AccountSummary;
  bracketsId: string;
  onChange: (avatar: string | null) => void;
}) {
  const { t } = useTranslation();
  const reduced = useReducedMotion();
  const calm = useCalm();
  const [open, setOpen] = useState(false);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (closeTimer.current) clearTimeout(closeTimer.current);
    },
    [],
  );

  const choose = (avatar: string | null) => {
    if (closeTimer.current) clearTimeout(closeTimer.current);
    const unchanged = (account.avatar ?? null) === avatar;
    if (!unchanged) onChange(avatar);
    closeTimer.current = setTimeout(() => setOpen(false), unchanged || reduced ? 0 : LINGER_MS);
  };

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <button
          type="button"
          aria-label={t("accounts.avatar.change")}
          className="group/face relative mt-[-2px] flex shrink-0 cursor-pointer rounded-[var(--radius-head)] transition-transform duration-150 ease-out active:scale-[0.96]"
        >
          <AccountAvatar account={account} selected={account.active} bracketsId={bracketsId} />
          <span
            aria-hidden
            className="pointer-events-none absolute inset-0 rounded-[var(--radius-head)] bg-obsidian-950/0 transition-colors duration-150 group-hover/face:bg-obsidian-950/25"
          />
          <span
            aria-hidden
            className="chamfer absolute -right-2 -bottom-2 flex size-[22px] items-center justify-center bg-obsidian-950 [--cut:5px]"
          >
            <span className="chamfer flex size-[18px] items-center justify-center bg-obsidian-700 text-gold-300 transition-colors duration-150 [--cut:4px] group-hover/face:bg-gold-500 group-hover/face:text-gold-ink">
              <PencilSimple size={11} weight="bold" />
            </span>
          </span>
        </button>
      </Popover.Trigger>
      <AnimatePresence>
        {open ? (
          // The grid's first frame (26 pictures) is drawn before the popover grows in.
          <EntranceGate key="picker">
            <Popover.Portal forceMount>
              <Popover.Content
                onPointerDownOutside={swallowDismissClick}
                forceMount
                asChild
                side="bottom"
                align="start"
                sideOffset={8}
                // Flips above the face when the space below is short; a window too short for
                // either side scrolls the grid inside the popover instead of cutting it off.
                avoidCollisions
                collisionPadding={10}
                aria-label={t("accounts.avatar.label")}
              >
                <GatedMotion.div
                  // Over a busy list: nearly solid, so the faces read on their own.
                  className="z-50 overflow-y-auto overscroll-contain rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-2.5 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
                  style={{
                    transformOrigin: "var(--radix-popover-content-transform-origin)",
                    maxHeight: "var(--radix-popover-content-available-height)",
                  }}
                  initial={calm({ opacity: 0, scale: 0.96, y: -4 })}
                  animate={calm({
                    opacity: 1,
                    scale: 1,
                    y: 0,
                    transition: { duration: 0.22, ease: easeOutExpo },
                  })}
                  exit={calm({ opacity: 0, scale: 0.98, transition: { duration: 0.14 } })}
                >
                  {/* 7 × 40 px: every picture in four rows, small enough to fit above or below
                    the third card of a 640 px tall window. */}
                  <AvatarPicker
                    value={account.avatar ?? null}
                    onSelect={choose}
                    label={t("accounts.avatar.label")}
                    columns={7}
                    size={40}
                    leading={[
                      {
                        id: null,
                        label: t("accounts.avatar.head"),
                        face: (size) => (
                          <PlayerHead name={account.username} src={account.headUrl} size={size} />
                        ),
                      },
                    ]}
                  />
                </GatedMotion.div>
              </Popover.Content>
            </Popover.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </Popover.Root>
  );
}
