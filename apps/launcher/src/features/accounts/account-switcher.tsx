// Title bar account switcher: the active account's head and name; opens a menu of every account
// (arrow keys, Enter, Esc, typeahead) with "Hesap ekle" and "Hesap merkezini aç".
import { CaretDown, Check, PushPin, UserPlus, UsersThree } from "@phosphor-icons/react";
import { cn, easeOutExpo, returnFocusAfterKeys, springSnappy } from "@pl/ui";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { DropdownMenu } from "radix-ui";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AccountFace, faceKey } from "./account-face";
import { onlineAfkSession } from "./afk-match";
import { useCalm } from "./calm";
import { useAccountsUi } from "./store";
import { useAccountHeads, useAccounts, useAfkSessions, useSwitchAccount } from "./use-accounts";
import { usePlayingAccountIds } from "./use-own-launch";

const itemClass =
  "flex cursor-pointer items-center gap-3 rounded-md px-2 outline-none select-none data-[highlighted]:bg-white/8";

export function AccountSwitcher() {
  const { t } = useTranslation();
  const calm = useCalm();
  const { data: accounts = [], isPending } = useAccounts();
  const [open, setOpen] = useState(false);
  const sessions = useAfkSessions(open);
  // The account whose game is on screen says so (PLAY-ACCT): OYNA is that account's alone.
  // Every account with a game on screen (PLAY-AGAIN: several may play at once).
  const playing = usePlayingAccountIds();
  const switchTo = useSwitchAccount();
  const openCenter = useAccountsUi((s) => s.openCenter);
  const trigger = useRef<HTMLButtonElement>(null);
  // Opening the center from the menu: the sheet takes focus, the menu must not take it back.
  const handoff = useRef(false);
  // The title bar is always there: it keeps the heads of every account filled in.
  useAccountHeads(accounts);
  const active = accounts.find((a) => a.active);

  if (isPending) return <span className="w-[120px]" aria-hidden />;

  const openFromMenu = (focus: "list" | "add") => {
    handoff.current = true;
    openCenter({ focus, from: trigger.current });
  };

  const triggerClass =
    "group relative flex h-titlebar cursor-pointer items-center px-0.5 outline-none after:absolute after:inset-x-0 after:top-0 after:-bottom-1";

  // No account yet: the pill goes straight to adding one.
  if (!accounts.length) {
    return (
      <button
        ref={trigger}
        type="button"
        className={triggerClass}
        onClick={() => openCenter({ focus: "add", from: trigger.current })}
      >
        <span className="flex h-8 items-center gap-2 rounded-full bg-gold-500/15 px-3 ring-1 ring-gold-400/40 transition-[background-color,transform,scale] duration-150 group-hover:bg-gold-500/22 group-focus-visible:ring-2 group-focus-visible:ring-gold-300 group-active:scale-[0.97]">
          <UserPlus size={16} className="text-gold-300" aria-hidden />
          <span className="text-[13px] font-semibold text-gold-200">
            {t("accounts.switcher.empty")}
          </span>
        </span>
      </button>
    );
  }

  return (
    <MotionConfig reducedMotion="user">
      <DropdownMenu.Root open={open} onOpenChange={setOpen}>
        <DropdownMenu.Trigger asChild>
          <button
            ref={trigger}
            type="button"
            aria-label={
              active ? t("accounts.switcher.label", { name: active.username }) : undefined
            }
            className={triggerClass}
          >
            {/* The face is round and sits 2 px inside the pill all round, so it follows the pill's
                round end (SMALL10b: a square face did not fit it). */}
            <span
              className={cn(
                "flex h-8 items-center gap-2 rounded-full bg-obsidian-950/45 pr-3 pl-[2px] ring-1 ring-white/8 backdrop-blur-md transition-[background-color,box-shadow,transform,scale] duration-150",
                "group-hover:bg-obsidian-950/60 group-hover:ring-white/16 group-active:scale-[0.97]",
                "group-data-[state=open]:ring-gold-400/45 group-focus-visible:ring-2 group-focus-visible:ring-gold-300",
                !active && "pl-2.5",
              )}
            >
              <AnimatePresence mode="popLayout" initial={false}>
                <motion.span
                  key={active ? `${active.id}:${faceKey(active)}` : "none"}
                  className="flex"
                  initial={calm({ opacity: 0, scale: 0.7, filter: "blur(3px)" })}
                  animate={calm({ opacity: 1, scale: 1, filter: "blur(0px)" })}
                  exit={calm({ opacity: 0, scale: 0.7, filter: "blur(3px)" })}
                  transition={springSnappy}
                >
                  {active ? (
                    <AccountFace account={active} size={24} className="rounded-full" />
                  ) : (
                    <UserPlus size={16} className="text-gold-300" aria-hidden />
                  )}
                </motion.span>
              </AnimatePresence>
              <span className="max-w-[150px] truncate text-[13px] font-semibold text-bone-50">
                {active?.username ?? t("accounts.switcher.empty")}
              </span>
              <CaretDown
                size={12}
                weight="bold"
                aria-hidden
                className="text-ash-400 transition-transform duration-200 group-data-[state=open]:rotate-180"
              />
            </span>
          </button>
        </DropdownMenu.Trigger>
        <AnimatePresence>
          {open ? (
            <DropdownMenu.Portal forceMount>
              <DropdownMenu.Content
                forceMount
                asChild
                align="end"
                sideOffset={4}
                collisionPadding={12}
                loop
                onCloseAutoFocus={(event) => {
                  if (handoff.current) {
                    handoff.current = false;
                    event.preventDefault();
                  }
                  // After a click the chip would light up with its ring (POLISH6).
                  returnFocusAfterKeys(event);
                }}
              >
                <motion.div
                  className="glass-strong z-50 w-[320px] rounded-[var(--radius-panel)] p-1.5 shadow-[0_24px_60px_rgb(34_6_6/0.55)] outline-none"
                  style={{ transformOrigin: "var(--radix-dropdown-menu-content-transform-origin)" }}
                  initial={calm({ opacity: 0, scale: 0.96, y: -4 })}
                  animate={calm({
                    opacity: 1,
                    scale: 1,
                    y: 0,
                    transition: { duration: 0.2, ease: easeOutExpo },
                  })}
                  exit={calm({ opacity: 0, scale: 0.98, transition: { duration: 0.12 } })}
                >
                  {accounts.length ? (
                    <>
                      <DropdownMenu.Label className="px-2.5 pt-1.5 pb-1.5 text-[12px] font-medium text-ash-400">
                        {t("accounts.switcher.title")}
                      </DropdownMenu.Label>
                      <DropdownMenu.RadioGroup
                        value={active?.id ?? ""}
                        onValueChange={(id) => {
                          const account = accounts.find((a) => a.id === id);
                          if (account) void switchTo(account);
                        }}
                      >
                        {accounts.map((account) => {
                          const afk = onlineAfkSession(account.username, sessions);
                          const expired = account.kind === "microsoft" && account.needsRelogin;
                          return (
                            <DropdownMenu.RadioItem
                              key={account.id}
                              value={account.id}
                              textValue={account.username}
                              className={cn(itemClass, "h-14")}
                            >
                              <AccountFace account={account} size={32} />
                              <span className="min-w-0 flex-1">
                                <span className="flex items-center gap-2">
                                  <span className="truncate text-[14px] font-semibold text-bone-50">
                                    {account.username}
                                  </span>
                                  {account.pinned ? (
                                    <PushPin
                                      size={12}
                                      weight="fill"
                                      className="-ml-0.5 shrink-0 text-ash-400"
                                      aria-hidden
                                    />
                                  ) : null}
                                </span>
                                {/* Every state in one place, as coloured words under the name
                                    (PLAY-AGAIN second look: tags beside the name and a line
                                    under it read as two systems): an expired session first, then
                                    in game, AFK, else what kind the account is (a username alone
                                    needs no line). */}
                                {expired ? (
                                  <span className="block truncate text-[12px] text-flare-400">
                                    {t("accounts.switcher.expired")}
                                  </span>
                                ) : playing.includes(account.id) || afk ? (
                                  <span className="block truncate text-[12px] text-jade-400">
                                    {playing.includes(account.id)
                                      ? t("playAcct.inGame")
                                      : t("accounts.center.afk")}
                                  </span>
                                ) : account.kind === "microsoft" ? (
                                  <span className="block truncate text-[12px] text-ash-400">
                                    {t("accounts.kind.microsoft")}
                                  </span>
                                ) : null}
                              </span>
                              <DropdownMenu.ItemIndicator className="flex shrink-0 pr-1">
                                <Check size={17} weight="bold" className="text-gold-300" />
                              </DropdownMenu.ItemIndicator>
                            </DropdownMenu.RadioItem>
                          );
                        })}
                      </DropdownMenu.RadioGroup>
                      <DropdownMenu.Separator className="my-1.5 h-px bg-white/8" />
                    </>
                  ) : null}
                  <DropdownMenu.Item
                    onSelect={() => openFromMenu("add")}
                    className={cn(itemClass, "h-11 text-[13.5px] text-bone-100")}
                  >
                    <span className="flex w-8 justify-center text-gold-300">
                      <UserPlus size={18} aria-hidden />
                    </span>
                    {t("accounts.add")}
                  </DropdownMenu.Item>
                  <DropdownMenu.Item
                    onSelect={() => openFromMenu("list")}
                    className={cn(itemClass, "h-11 text-[13.5px] text-bone-100")}
                  >
                    <span className="flex w-8 justify-center text-ash-300">
                      <UsersThree size={18} aria-hidden />
                    </span>
                    {t("accounts.center.open")}
                  </DropdownMenu.Item>
                </motion.div>
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          ) : null}
        </AnimatePresence>
      </DropdownMenu.Root>
    </MotionConfig>
  );
}
