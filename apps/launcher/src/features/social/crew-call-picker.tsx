// "Ekibi çağır" where it cannot go yet, and "Ekiplerini çağır" (CALL1, TEAM4).
// - CallReason: the one line under the call button after a press, in place (never a toast): not on
//   a multiplayer server, or on another server whose address is hidden, with "Aç ve çağır" (turns
//   "Başka sunuculardaysam adresini göster" on, then calls). It goes by itself once the player is
//   on a server a call may start from.
// - CrewCallDialog: one call to one or more of the player's crews (one call however many): the
//   crews the player may call, each a row with its mark, name and who is online, ticked or not; a
//   crew whose call is on waits. Opened from the crew page ("Birden çok ekibi çağır"), the friend
//   list's menu and Home. Mounted once, by SocialHost.
import { Info, MegaphoneSimple } from "@phosphor-icons/react";
import { Button, cn, Dialog } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check } from "@/features/settings/cleanup/check";
import { callCrew, shareAndCall } from "./call-actions";
import { type CallRefusal, callableCrews, crewsWithCall, refusalNow } from "./crews";
import { isProjectsHost } from "./model";
import { rise, useSocialMotion } from "./motion";
import { useSocialUi } from "./store";
import { CrewMark, TeamTag } from "./team-emblem";
import { teammatesOnline } from "./teams";
import { useCallClock } from "./use-call-clock";

/** Why "Ekibi çağır" did not go, said where the player looks; "Aç ve çağır" on a hidden server. */
export function CallReason({
  reason,
  crews,
  onCalled,
  className,
}: {
  reason: CallRefusal | null;
  /** The crews the call is for (empty: the primary one). */
  crews: string[];
  /** The call went out after "Aç ve çağır". */
  onCalled?: () => void;
  className?: string;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const [busy, setBusy] = useState(false);
  const share = async () => {
    setBusy(true);
    const outcome = await shareAndCall(crews);
    setBusy(false);
    if (outcome?.status === "called") onCalled?.();
  };
  return (
    <AnimatePresence initial={false} mode="popLayout">
      {reason ? (
        <motion.div
          key={reason}
          role="status"
          className={cn(
            "flex items-center gap-2 text-[12.5px] leading-snug text-ash-300",
            className,
          )}
          {...rise(reduce, 4, 0.2)}
        >
          <Info size={15} className="shrink-0 text-ash-400" aria-hidden />
          <span className="min-w-0 flex-1">{t(`crews.call.${reason}`)}</span>
          {reason === "serverHidden" ? (
            <Button
              size="sm"
              variant="secondary"
              className="shrink-0"
              disabled={busy}
              onClick={() => void share()}
            >
              {t("crews.call.showAndCall")}
            </Button>
          ) : null}
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

/**
 * The press and what came of it: `call(crews)` calls; a refusal for the place (CALL1) stays as
 * the reason line until the player is somewhere a call may start from.
 */
export function useCallPress() {
  const place = useSocialUi((s) => s.view.callPlace);
  const [asked, setAsked] = useState<{ reason: CallRefusal; place: typeof place } | null>(null);
  const reason = refusalNow(asked, place);
  const call = async (crews: string[]) => {
    const outcome = await callCrew(crews);
    if (outcome?.status === "needsServer" || outcome?.status === "serverHidden") {
      setAsked({ reason: outcome.status, place });
      return false;
    }
    if (outcome) setAsked(null);
    return outcome?.status === "called";
  };
  return { reason, call, clear: () => setAsked(null) };
}

/** Where the crews are called to, from what this launcher tells friends. */
function useCallTarget(): string | null {
  const { t } = useTranslation();
  const activity = useSocialUi((s) => s.view.activity);
  if (activity.state !== "playing" || !activity.host) return null;
  return isProjectsHost(activity.host)
    ? t("crews.call.toProjects")
    : t("crews.call.toServer", { host: activity.host });
}

export function CrewCallDialog() {
  const { t } = useTranslation();
  const ticked = useSocialUi((s) => s.callPicker);
  const close = useSocialUi((s) => s.closeCallPicker);
  const crews = useSocialUi((s) => s.view.crews);
  const features = useSocialUi((s) => s.view.crew);
  const calls = useSocialUi((s) => s.view.calls);
  const place = useSocialUi((s) => s.view.callPlace);
  const now = useCallClock((s) => s.now);
  const target = useCallTarget();
  const listId = useId();
  const callable = callableCrews(crews, features);
  const waiting = crewsWithCall(calls, crews, now);
  const open = ticked !== null;
  const [chosen, setChosen] = useState<string[]>([]);
  const { reason, call, clear } = useCallPress();
  const [busy, setBusy] = useState(false);
  // Each opening starts from what it was opened with (a crew page ticks its crew; nothing named:
  // the primary crew).
  // biome-ignore lint/correctness/useExhaustiveDependencies: only a new opening resets it
  useEffect(() => {
    if (ticked) {
      setChosen(ticked.length ? ticked : callable.slice(0, 1).map((crew) => crew.id));
      clear();
    }
  }, [ticked]);
  // Ticked crews that cannot be called (gone, or their call is on) do not count.
  const sendable = chosen.filter(
    (id) => callable.some((crew) => crew.id === id) && !waiting.has(id),
  );
  // The place says no before the press, here: the dialog is the button.
  const shown = reason ?? (place === "ok" ? null : place);
  // With one crew to call there is nothing to pick: the dialog is its reason and its button.
  const several = callable.length > 1;

  const submit = async () => {
    if (busy || !sendable.length) return;
    setBusy(true);
    const called = await call(sendable);
    setBusy(false);
    if (called) close();
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) close();
      }}
      title={several ? t("crews.call.pickTitle") : t("teams.call.start")}
      description={target ?? undefined}
      closeLabel={t("common.close")}
      width={440}
      footer={
        <>
          <Button variant="ghost" onClick={close}>
            {t("teams.cancel")}
          </Button>
          <Button
            variant="primary"
            icon={<MegaphoneSimple size={16} />}
            // Where a call cannot go, the reason's line says why (and "Aç ve çağır" where it can).
            disabled={!sendable.length || busy || shown !== null}
            onClick={() => void submit()}
          >
            {t("crews.call.submit", { count: Math.max(1, sendable.length) })}
          </Button>
        </>
      }
    >
      {several ? (
        <ul aria-label={t("crews.rail")} className="-mx-1 flex flex-col gap-1">
          {callable.map((crew) => {
            const busyCrew = waiting.has(crew.id);
            const on = chosen.includes(crew.id) && !busyCrew;
            const online = teammatesOnline(crew);
            const id = `${listId}-${crew.id}`;
            return (
              <li key={crew.id}>
                <label
                  htmlFor={id}
                  className={cn(
                    "flex cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] px-2 py-2 transition-colors duration-150",
                    busyCrew ? "cursor-default opacity-55" : "hover:bg-white/[0.045]",
                  )}
                >
                  <Check
                    id={id}
                    checked={on}
                    disabled={busyCrew}
                    onCheckedChange={(next) =>
                      setChosen((current) =>
                        next ? [...current, crew.id] : current.filter((c) => c !== crew.id),
                      )
                    }
                  />
                  <CrewMark look={crew} size={28} />
                  <span className="flex min-w-0 flex-1 items-center gap-1.5">
                    <span className="truncate text-[14px] font-semibold text-bone-50">
                      {crew.name}
                    </span>
                    <TeamTag tag={crew.tag} color={crew.color} />
                  </span>
                  <span className="shrink-0 text-[12.5px] text-ash-300">
                    {busyCrew ? (
                      t("crews.call.busy")
                    ) : (
                      <>
                        <span
                          className={cn(
                            "font-display font-semibold tabular-nums",
                            online ? "text-jade-400" : "text-ash-400",
                          )}
                        >
                          {online}
                        </span>{" "}
                        {t("teams.card.online")}
                      </>
                    )}
                  </span>
                </label>
              </li>
            );
          })}
        </ul>
      ) : null}
      <CallReason
        reason={shown}
        crews={sendable}
        onCalled={close}
        className={several ? "mt-3 px-1" : undefined}
      />
    </Dialog>
  );
}
