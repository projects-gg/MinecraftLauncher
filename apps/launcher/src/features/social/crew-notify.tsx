// "Ekip bildirimleri": the crew page's bell opens a small panel with two switches, kept on this
// computer (and in the settings backup): crew calls (on) and crewmates joining Projects (off).
// Do-not-disturb silences both, as it silences every friends notice. Settings → Arkadaşlar →
// Bildirimler shows the same switches (CrewNotifySwitches).
import { Bell } from "@phosphor-icons/react";
import { easeOutExpo, IconButton, Switch } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { Popover } from "radix-ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import { useSocialPrefs, useSocialUi } from "./store";

export function CrewNotifySwitches() {
  const { t } = useTranslation();
  const calls = useSocialPrefs((s) => s.notifyCrewCalls);
  const projects = useSocialPrefs((s) => s.notifyCrewProjects);
  const offersCalls = useSocialUi((s) => s.view.crew.calls);
  const set = useSocialPrefs((s) => s.set);
  return (
    <div className="space-y-3.5">
      {offersCalls ? (
        <Switch
          checked={calls}
          onCheckedChange={(on) => set("notifyCrewCalls", on)}
          label={t("teams.notify.calls")}
        />
      ) : null}
      <Switch
        checked={projects}
        onCheckedChange={(on) => set("notifyCrewProjects", on)}
        label={t("teams.notify.projects")}
      />
    </div>
  );
}

/** The bell in the crew page's header. */
export function CrewNotifyButton() {
  const { t } = useTranslation();
  const calm = useCalm();
  const [open, setOpen] = useState(false);
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <IconButton label={t("teams.notify.title")} icon={<Bell size={19} />} />
      </Popover.Trigger>
      <AnimatePresence>
        {open ? (
          <Popover.Portal forceMount>
            <Popover.Content
              forceMount
              asChild
              side="bottom"
              align="end"
              sideOffset={6}
              collisionPadding={12}
              aria-label={t("teams.notify.title")}
            >
              <motion.div
                className="z-50 w-[300px] rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-4 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
                style={{ transformOrigin: "var(--radix-popover-content-transform-origin)" }}
                initial={calm({ opacity: 0, scale: 0.96, y: -4 })}
                animate={calm({
                  opacity: 1,
                  scale: 1,
                  y: 0,
                  transition: { duration: 0.18, ease: easeOutExpo },
                })}
                exit={calm({ opacity: 0, scale: 0.98, transition: { duration: 0.12 } })}
              >
                <p className="mb-3 font-display text-[15px] font-semibold text-bone-50">
                  {t("teams.notify.title")}
                </p>
                <CrewNotifySwitches />
              </motion.div>
            </Popover.Content>
          </Popover.Portal>
        ) : null}
      </AnimatePresence>
    </Popover.Root>
  );
}
