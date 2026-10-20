// One teammate: picture with presence, name (a crown for the founder), where they are, and "Katıl"
// when they are on projects.gg. The row opens their card beside the panel: join, "Discord'dan mesaj
// at", and in a quieter corner what the player may do about them (the founder hands on the role or
// removes them; anyone may block), each confirmed in place with one sentence.
import { Crown, DotsThree, Prohibit, UserMinus } from "@phosphor-icons/react";
import { Button, cn, easeOutExpo, IconButton, returnFocusAfterKeys } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { DropdownMenu, Popover } from "radix-ui";
import { type KeyboardEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { TeamMember } from "@/lib/api/social-types";
import { attempt } from "./actions";
import { DiscordButton } from "./discord-button";
import { openInDiscord } from "./discord-link";
import { JoinButton } from "./friend-row";
import { isJoinable, presenceLine } from "./model";
import { rise, swap, useSocialMotion } from "./motion";
import { SocialAvatar } from "./social-face";

type Question = "founder" | "remove" | "block" | "unblock";

function MemberCard({
  member,
  founder,
  onDone,
}: {
  member: TeamMember;
  /** The player founded the team (may hand on the role or remove). */
  founder: boolean;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const calm = useCalm();
  const [question, setQuestion] = useState<Question | null>(null);
  const [menu, setMenu] = useState(false);
  const [busy, setBusy] = useState(false);
  const joinable = member.state === "playing" && isJoinable(member.game);

  const run = async (action: Question) => {
    setBusy(true);
    const done = await attempt(() => {
      switch (action) {
        case "founder":
          return api.social.transferTeam(member.id);
        case "remove":
          return api.social.removeTeamMember(member.id);
        case "block":
          return api.social.block(member.id);
        case "unblock":
          return api.social.unblock(member.id);
      }
    });
    setBusy(false);
    if (done === undefined) return;
    const said = {
      founder: t("teams.member.madeFounder", { name: member.name }),
      remove: t("teams.member.removed", { name: member.name }),
      block: t("social.actions.blocked", { name: member.name }),
      unblock: t("social.actions.unblocked", { name: member.name }),
    }[action];
    announce(said);
    onDone();
  };

  const asks: Record<Question, string> = {
    founder: t("teams.member.makeFounderAsk", { name: member.name }),
    remove: t("teams.member.removeAsk", { name: member.name }),
    block: t("social.actions.blockAsk"),
    unblock: t("social.actions.unblock"),
  };
  const labels: Record<Question, string> = {
    founder: t("teams.member.makeFounder"),
    remove: t("teams.member.remove"),
    block: t("social.actions.block"),
    unblock: t("social.actions.unblock"),
  };

  return (
    <div className="w-[300px]">
      <div className="flex items-center gap-3.5">
        <SocialAvatar
          source={member}
          size={56}
          state={member.state}
          dnd={member.dnd}
          surface="var(--color-obsidian-900)"
        />
        <div className="min-w-0">
          <p className="flex items-center gap-1.5 truncate font-display text-[18px] leading-tight font-semibold text-bone-50">
            <span className="truncate">{member.name}</span>
            {member.role === "founder" ? (
              <Crown
                size={15}
                weight="fill"
                className="shrink-0 text-gold-300"
                aria-label={t("teams.founder")}
              />
            ) : null}
          </p>
          <p className="truncate text-[12.5px] text-ash-400" data-selectable>
            @{member.username}
          </p>
          <p
            className={cn(
              "mt-1 truncate text-[13px]",
              member.blocked
                ? "text-ash-400"
                : joinable
                  ? "text-jade-400"
                  : member.state === "offline"
                    ? "text-ash-400"
                    : "text-ash-300",
            )}
          >
            {member.blocked ? t("teams.member.blocked") : presenceLine(member, t)}
          </p>
        </div>
      </div>

      <div className="mt-4 flex flex-col gap-2">
        {joinable && !member.blocked ? (
          <JoinButton size="md" variant="primary" className="w-full" />
        ) : null}
        <DiscordButton className="w-full" onClick={() => void openInDiscord(member)}>
          {t("social.actions.message")}
        </DiscordButton>
      </div>

      <div className="mt-3 border-t border-white/8 pt-2">
        <AnimatePresence mode="popLayout" initial={false}>
          {question ? (
            <motion.div key={question} className="pt-1" {...rise(reduce, 4, 0.2)}>
              <p className="text-[13px] leading-snug text-bone-100">{asks[question]}</p>
              <div className="mt-2.5 flex justify-end gap-2">
                <Button size="sm" variant="ghost" onClick={() => setQuestion(null)}>
                  {t("social.actions.cancel")}
                </Button>
                <Button
                  size="sm"
                  variant={
                    question === "founder" || question === "unblock" ? "secondary" : "danger"
                  }
                  disabled={busy}
                  onClick={() => void run(question)}
                >
                  {labels[question]}
                </Button>
              </div>
            </motion.div>
          ) : (
            <motion.div
              key="tools"
              className="flex items-center justify-between"
              {...rise(reduce, 4, 0.2)}
            >
              <span className="px-1 text-[12px] text-ash-400">
                {member.role === "founder" ? t("teams.founder") : null}
              </span>
              <DropdownMenu.Root open={menu} onOpenChange={setMenu} modal={false}>
                <DropdownMenu.Trigger asChild>
                  <IconButton
                    label={t("social.actions.more")}
                    icon={<DotsThree size={20} weight="bold" />}
                    size="sm"
                  />
                </DropdownMenu.Trigger>
                <AnimatePresence>
                  {menu ? (
                    <DropdownMenu.Portal forceMount>
                      <DropdownMenu.Content
                        onCloseAutoFocus={returnFocusAfterKeys}
                        forceMount
                        asChild
                        align="end"
                        sideOffset={4}
                        collisionPadding={12}
                        loop
                      >
                        <motion.div
                          className="z-[60] w-[220px] rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-1.5 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
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
                          {founder ? (
                            <>
                              <DropdownMenu.Item
                                onSelect={() => setQuestion("founder")}
                                className="flex h-10 cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-[13.5px] text-bone-100 outline-none select-none data-[highlighted]:bg-white/8"
                              >
                                <Crown size={16} className="text-ash-300" aria-hidden />
                                {t("teams.member.makeFounder")}
                              </DropdownMenu.Item>
                              <DropdownMenu.Item
                                onSelect={() => setQuestion("remove")}
                                className="flex h-10 cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-[13.5px] text-bone-100 outline-none select-none data-[highlighted]:bg-white/8"
                              >
                                <UserMinus size={16} className="text-ash-300" aria-hidden />
                                {t("teams.member.remove")}
                              </DropdownMenu.Item>
                              <DropdownMenu.Separator className="my-1.5 h-px bg-white/8" />
                            </>
                          ) : null}
                          <DropdownMenu.Item
                            onSelect={() =>
                              member.blocked ? void run("unblock") : setQuestion("block")
                            }
                            className={cn(
                              "flex h-10 cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-[13.5px] outline-none select-none data-[highlighted]:bg-white/8",
                              member.blocked ? "text-bone-100" : "text-flare-400",
                            )}
                          >
                            <Prohibit size={16} aria-hidden />
                            {member.blocked
                              ? t("social.actions.unblock")
                              : t("social.actions.block")}
                          </DropdownMenu.Item>
                        </motion.div>
                      </DropdownMenu.Content>
                    </DropdownMenu.Portal>
                  ) : null}
                </AnimatePresence>
              </DropdownMenu.Root>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}

export function TeamMemberRow({
  member,
  founder,
  onKeyDown,
}: {
  member: TeamMember;
  founder: boolean;
  onKeyDown?: (event: KeyboardEvent<HTMLButtonElement>) => void;
}) {
  const { t } = useTranslation();
  const calm = useCalm();
  const { reduce } = useSocialMotion();
  const [open, setOpen] = useState(false);
  const joinable = member.state === "playing" && isJoinable(member.game) && !member.blocked;
  const line = member.blocked ? t("teams.member.blocked") : presenceLine(member, t);

  return (
    <div className="group/friend flex items-center gap-2 rounded-[var(--radius-panel)] pr-2 transition-colors duration-150 hover:bg-white/[0.045] has-[[data-state=open]]:bg-white/[0.06]">
      <Popover.Root open={open} onOpenChange={setOpen}>
        <Popover.Trigger asChild>
          <button
            type="button"
            data-friend-row
            onKeyDown={onKeyDown}
            aria-label={`${member.name}, ${line}`}
            className="flex min-w-0 flex-1 cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] py-2 pl-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
          >
            <SocialAvatar
              source={member}
              size={36}
              state={member.blocked ? "offline" : member.state}
              dnd={member.dnd}
            />
            <span className="min-w-0 flex-1">
              <span className="flex items-center gap-1.5">
                <span
                  className={cn(
                    "truncate text-[14px] font-semibold",
                    member.state === "offline" ? "text-bone-100/70" : "text-bone-50",
                  )}
                >
                  {member.name}
                </span>
                {member.role === "founder" ? (
                  <Crown
                    size={12}
                    weight="fill"
                    className="shrink-0 text-gold-300"
                    aria-label={t("teams.founder")}
                  />
                ) : null}
              </span>
              <span className="relative block h-[18px] overflow-hidden">
                <AnimatePresence mode="popLayout" initial={false}>
                  <motion.span
                    key={line}
                    className={cn(
                      "absolute inset-x-0 truncate text-[12.5px] leading-[18px]",
                      joinable ? "text-jade-400" : "text-ash-400",
                    )}
                    {...swap(reduce)}
                  >
                    {line}
                  </motion.span>
                </AnimatePresence>
              </span>
            </span>
          </button>
        </Popover.Trigger>
        <AnimatePresence>
          {open ? (
            <Popover.Portal forceMount>
              <Popover.Content
                forceMount
                asChild
                side="left"
                align="start"
                sideOffset={14}
                collisionPadding={16}
                aria-label={member.name}
              >
                <motion.div
                  className="z-50 rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-4 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_30px_80px_rgb(34_6_6/0.6)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
                  style={{ transformOrigin: "var(--radix-popover-content-transform-origin)" }}
                  initial={calm({ opacity: 0, scale: 0.96, x: 8 })}
                  animate={calm({
                    opacity: 1,
                    scale: 1,
                    x: 0,
                    transition: { duration: 0.22, ease: easeOutExpo },
                  })}
                  exit={calm({ opacity: 0, scale: 0.98, transition: { duration: 0.14 } })}
                >
                  <MemberCard member={member} founder={founder} onDone={() => setOpen(false)} />
                </motion.div>
              </Popover.Content>
            </Popover.Portal>
          ) : null}
        </AnimatePresence>
      </Popover.Root>
      <AnimatePresence initial={false}>
        {joinable ? (
          <motion.span key="join" className="flex" data-no-drag {...rise(reduce, 0, 0.2)}>
            <JoinButton />
          </motion.span>
        ) : null}
      </AnimatePresence>
    </div>
  );
}
