// "Ekip bildirimleri": what the player hears of their crew. Where the service keeps the crew's
// settings (`teamFeatures: mute`), "Sessize al" silences every crew notification for an hour, eight
// hours, until the next morning or until "Sesi aç", and "Ekip çağrıları" is the same on all the
// player's devices; with an older service this computer's switch decides calls, as before. "Ekip
// arkadaşların Projects'e girince" stays this computer's (and the settings backup's). A muted
// crew's calls still show on the crew page and the Home line, ready to answer; do-not-disturb
// silences all of it too. The crew page's bell (struck through while muted), the crew's menu and
// Settings → Arkadaşlar → Bildirimler show them.
import { Bell, BellSlash, CaretDown } from "@phosphor-icons/react";
import { Button, cn, easeOutExpo, IconButton, Menu, type MenuItem, Switch } from "@pl/ui";
import type { TFunction } from "i18next";
import { AnimatePresence, motion } from "motion/react";
import { Popover } from "radix-ui";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { CrewNotifications } from "@/lib/api/social-types";
import { dateTimeFormat } from "@/lib/format";
import i18n from "@/lib/i18n";
import { attempt } from "./actions";
import { pop, rise, swap, useSocialMotion } from "./motion";
import {
  MUTE_PRESETS,
  type MutePreset,
  mutedNow,
  muteEndDay,
  muteUntilFor,
  timeDative,
} from "./mute";
import { useSocialPrefs, useSocialUi } from "./store";
import { useCallClock } from "./use-call-clock";

/** The crew's settings where the service keeps them, and whether the crew is muted now. */
export function useCrewMute() {
  const settings = useSocialUi((s) => (s.view.crew.mute ? s.view.crewNotifications : undefined));
  // Moves at a timed mute's end (use-call-clock.ts), so "Sessizde" goes by itself.
  const now = useCallClock((s) => s.now);
  return { settings, muted: mutedNow(settings, now), now };
}

/** "14:30'a kadar", "yarın 08:00'e kadar", "23 Ekim 14:30'a kadar". */
function untilText(t: TFunction, untilMs: number, nowMs: number): string {
  const end = new Date(untilMs);
  const time = dateTimeFormat({ hour: "2-digit", minute: "2-digit" }).format(end);
  const ending = timeDative(end.getHours(), end.getMinutes());
  switch (muteEndDay(untilMs, nowMs)) {
    case "today":
      return t("teams.mute.untilToday", { time, ending });
    case "tomorrow":
      return t("teams.mute.untilTomorrow", { time, ending });
    case "later":
      return t("teams.mute.untilLater", {
        time,
        ending,
        date: dateTimeFormat({ day: "numeric", month: "long" }).format(end),
      });
  }
}

/** "Sessizde · 14:30'a kadar" ("Sessizde" until unmuted); null while not muted. */
export function muteState(
  t: TFunction,
  settings: CrewNotifications | undefined,
  nowMs: number,
): string | null {
  if (!settings || !mutedNow(settings, nowMs)) return null;
  const muted = t("teams.mute.muted");
  return settings.muteUntil === null
    ? muted
    : `${muted} · ${untilText(t, settings.muteUntil * 1000, nowMs)}`;
}

/** "Sessize al" for a while (said to screen readers once it is so). */
export async function muteCrew(preset: MutePreset): Promise<void> {
  const until = muteUntilFor(preset, Date.now());
  const done = await attempt(() =>
    api.social.setCrewNotifications(
      until === null ? { muted: true } : { muted: true, muteUntil: until },
    ),
  );
  if (done === undefined) return;
  const state = muteState(i18n.t, useSocialUi.getState().view.crewNotifications, Date.now());
  announce(state ?? i18n.t("teams.mute.muted"));
}

/** "Sesi aç". */
export async function unmuteCrew(): Promise<void> {
  const done = await attempt(() => api.social.setCrewNotifications({ muted: false }));
  if (done !== undefined) announce(i18n.t("teams.mute.unmuted"));
}

/** "1 saat", "8 saat", "Yarın sabaha kadar", "Ben açana kadar" (a menu, or a submenu). */
export function muteItems(t: TFunction): MenuItem[] {
  return MUTE_PRESETS.map((preset) => ({
    label: t(`teams.mute.presets.${preset}`),
    onSelect: () => void muteCrew(preset),
  }));
}

/** A bell, struck through while the crew is muted; one turns into the other. */
export function MuteBell({ muted, size }: { muted: boolean; size: number }) {
  const { reduce } = useSocialMotion();
  return (
    <span className="relative flex">
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span key={muted ? "muted" : "on"} className="flex" {...pop(reduce, 0.18)}>
          {muted ? <BellSlash size={size} /> : <Bell size={size} />}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

/** "Sessize al" (how long, in a menu) or, while muted, "Sesi aç". */
function MuteControl({ muted }: { muted: boolean }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const [busy, setBusy] = useState(false);
  const unmute = async () => {
    setBusy(true);
    await unmuteCrew();
    setBusy(false);
  };
  return (
    <span className="relative flex shrink-0">
      <AnimatePresence mode="popLayout" initial={false}>
        {muted ? (
          <motion.span key="unmute" className="flex" {...swap(reduce, 0.18)}>
            <Button
              size="sm"
              variant="secondary"
              icon={<Bell size={15} />}
              disabled={busy}
              onClick={() => void unmute()}
            >
              {t("teams.mute.undo")}
            </Button>
          </motion.span>
        ) : (
          <motion.span key="mute" className="flex" {...swap(reduce, 0.18)}>
            <Menu
              items={muteItems(t)}
              trigger={
                <Button
                  size="sm"
                  variant="secondary"
                  icon={<BellSlash size={15} />}
                  iconRight={<CaretDown size={12} weight="bold" className="text-ash-300" />}
                >
                  {t("teams.mute.action")}
                </Button>
              }
            />
          </motion.span>
        )}
      </AnimatePresence>
    </span>
  );
}

/** The crew's own switches: crew calls (the service's where it keeps them) and crewmates joining
 * Projects (this computer's). */
export function CrewNotifySwitches() {
  const { t } = useTranslation();
  const localCalls = useSocialPrefs((s) => s.notifyCrewCalls);
  const projects = useSocialPrefs((s) => s.notifyCrewProjects);
  const offersCalls = useSocialUi((s) => s.view.crew.calls);
  const set = useSocialPrefs((s) => s.set);
  const { settings } = useCrewMute();
  // The switch moves at once; the service's answer (or its failure) settles it.
  const [pending, setPending] = useState<boolean | null>(null);
  const kept = settings?.calls;
  useEffect(() => {
    if (pending !== null && kept === pending) setPending(null);
  }, [pending, kept]);
  const calls = kept === undefined ? localCalls : (pending ?? kept);
  const setCalls = async (on: boolean) => {
    if (kept === undefined) {
      set("notifyCrewCalls", on);
      return;
    }
    setPending(on);
    const done = await attempt(() => api.social.setCrewNotifications({ calls: on }));
    if (done === undefined) setPending(null);
  };
  return (
    <div className="space-y-3.5">
      {offersCalls ? (
        <Switch
          checked={calls}
          onCheckedChange={(on) => void setCalls(on)}
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

/**
 * "Ekip bildirimleri" with "Sessize al" beside it (where the service keeps the crew's settings),
 * what is muted until when, and the switches. The bell's panel and Settings show the same.
 */
export function CrewNotifyGroup({ titleClassName }: { titleClassName: string }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const { settings, muted, now } = useCrewMute();
  const state = muteState(t, settings, now);
  return (
    <div>
      <div className="flex min-h-8 items-center gap-3">
        <p className={cn("min-w-0 flex-1", titleClassName)}>{t("teams.notify.title")}</p>
        {settings ? <MuteControl muted={muted} /> : null}
      </div>
      <AnimatePresence initial={false}>
        {state ? (
          <motion.p
            key="muted"
            className="mt-1 flex items-center gap-1.5 text-[12.5px] text-ash-300"
            {...rise(reduce, 4, 0.22)}
          >
            <BellSlash size={13} aria-hidden className="shrink-0" />
            <span className="min-w-0 truncate">{state}</span>
          </motion.p>
        ) : null}
      </AnimatePresence>
      <div className="mt-3.5">
        <CrewNotifySwitches />
      </div>
    </div>
  );
}

/** The bell in the crew page's header: struck through while muted, its name says until when. */
export function CrewNotifyButton() {
  const { t } = useTranslation();
  const calm = useCalm();
  const [open, setOpen] = useState(false);
  const { settings, muted, now } = useCrewMute();
  const state = muteState(t, settings, now);
  const title = t("teams.notify.title");
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <IconButton
          label={state ? `${title}: ${state}` : title}
          icon={<MuteBell muted={muted} size={19} />}
        />
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
              aria-label={title}
            >
              <motion.div
                className="z-50 w-[320px] rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-4 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
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
                <CrewNotifyGroup titleClassName="font-display text-[15px] font-semibold text-bone-50" />
              </motion.div>
            </Popover.Content>
          </Popover.Portal>
        ) : null}
      </AnimatePresence>
    </Popover.Root>
  );
}
