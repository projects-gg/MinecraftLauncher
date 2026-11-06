// "Keşif" (CREW-DISCOVER), the leader's block on the crew page, under the code: one switch
// ("Keşifte görün") with one line of what becomes public, then one status line ("Bugün 18:00'de
// bakılacak.", "Keşifte görünüyor.", "Bu sefer onaylanmadı. 7 gün sonra yeniden deneyebilirsin.",
// "Bu ekip keşifte görünemez."). Turning it on opens the text and the tags in place; nothing goes
// until "Keşfe gönder". The requirements the service checks before a wave are shown before asking,
// so nobody waits a day for nothing; why a wave refused is never told (only when to try again).
// Turning a listed crew off asks once, in place: coming back waits for another approval.
import {
  CheckCircle,
  Circle,
  Clock,
  Eye,
  EyeSlash,
  LockSimple,
  PencilSimple,
} from "@phosphor-icons/react";
import { Button, cn, Skeleton, Switch } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { ToggleChip } from "@/features/library/parts";
import { discoverApi } from "@/lib/api/discover-api";
import {
  DISCOVER_MAX_TAGS,
  DISCOVER_TAGS,
  DISCOVER_TEXT_MAX,
  type DiscoverStatus,
  type DiscoverTag,
} from "@/lib/api/discover-types";
import type { SocialTeam } from "@/lib/api/social-types";
import { dateTimeFormat } from "@/lib/format";
import { attempt } from "../actions";
import { rise, swap, useSocialMotion } from "../motion";
import { discoverOff } from "./listing";
import {
  checksHold,
  cleanText,
  daysUntil,
  maySend,
  textLength,
  waveEnding,
  waveWhen,
} from "./model";

const fieldClass =
  "w-full min-w-0 resize-none rounded-md border border-white/10 bg-obsidian-800/80 px-3 py-2 text-[13.5px] leading-normal text-bone-50 transition-[border-color,box-shadow] duration-150 outline-none placeholder:text-ash-500 hover:border-white/16 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-300/25 disabled:opacity-55";

/** The status line's words and mark. */
function useStatusLine(
  status: DiscoverStatus,
): { text: string; icon: "wait" | "on" | "lock" | "off" } | null {
  const { t } = useTranslation();
  const now = Date.now();
  const wave = waveWhen(status.waveAt, now);
  const time = dateTimeFormat({ hour: "2-digit", minute: "2-digit" }).format(
    new Date(status.waveAt * 1000),
  );
  const at = { time, ending: waveEnding(wave.hour, wave.minute) };
  const tomorrow = wave.day !== "today";
  switch (status.state) {
    case "waiting":
      return {
        text: t(
          tomorrow ? "discover.block.status.waitingTomorrow" : "discover.block.status.waitingToday",
          at,
        ),
        icon: "wait",
      };
    case "listed":
      return status.changeWaits
        ? {
            text: t(
              tomorrow
                ? "discover.block.status.changeTomorrow"
                : "discover.block.status.changeToday",
              at,
            ),
            icon: "wait",
          }
        : { text: t("discover.block.status.listed"), icon: "on" };
    case "refused":
      return {
        text: t("discover.block.status.refused", {
          count: status.retryAt ? daysUntil(status.retryAt, now) : 7,
        }),
        icon: "off",
      };
    case "closed":
      return { text: t("discover.block.status.closed"), icon: "lock" };
    case "held":
      return { text: t("discover.block.status.held"), icon: "off" };
    default:
      return null;
  }
}

const ICONS = {
  wait: Clock,
  on: Eye,
  lock: LockSimple,
  off: EyeSlash,
} as const;

/** The requirements, each with whether it holds (shown only while one does not). */
function Needs({ status }: { status: DiscoverStatus }) {
  const { t } = useTranslation();
  const { checks } = status;
  const rows = [
    {
      key: "members",
      ok: checks.members.have >= checks.members.need,
      text: t("discover.block.needs.members", {
        count: checks.members.have,
        need: checks.members.need,
      }),
    },
    {
      key: "crewDays",
      ok: checks.crewDays.have >= checks.crewDays.need,
      text: t("discover.block.needs.crewDays", { count: checks.crewDays.need }),
    },
    {
      key: "leaderDays",
      ok: checks.leaderDays.have >= checks.leaderDays.need,
      text: t("discover.block.needs.leaderDays", { count: checks.leaderDays.need }),
    },
    ...(checks.visible
      ? []
      : [{ key: "hidden", ok: false, text: t("discover.block.needs.hidden") }]),
  ]
    .filter((row) => row.key !== "crewDays" || checks.crewDays.need > 0)
    .filter((row) => row.key !== "leaderDays" || checks.leaderDays.need > 0);
  return (
    <ul className="mt-3 space-y-1.5">
      {rows.map((row) => (
        <li
          key={row.key}
          className={cn(
            "flex items-start gap-2 text-[13px] leading-snug",
            row.ok ? "text-ash-400" : "text-bone-100",
          )}
        >
          {row.ok ? (
            <CheckCircle
              size={15}
              weight="fill"
              className="mt-px shrink-0 text-ash-400"
              aria-hidden
            />
          ) : (
            <Circle size={15} className="mt-px shrink-0 text-ash-400" aria-hidden />
          )}
          {row.text}
        </li>
      ))}
    </ul>
  );
}

export function CrewDiscover({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const switchId = useId();
  const helpId = useId();
  const textId = useId();
  const [status, setStatus] = useState<DiscoverStatus | null>(null);
  const [editing, setEditing] = useState(false);
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [text, setText] = useState("");
  const [tags, setTags] = useState<DiscoverTag[]>([]);

  // Asked again when the crew's size changes while the block is open (a newcomer can be the
  // member the requirements were waiting for); nothing polls.
  const memberCount = team.members.length;
  useEffect(() => {
    let live = true;
    void memberCount;
    void attempt(() => discoverApi.status(team.id), discoverOff).then((found) => {
      if (live && found) setStatus(found);
    });
    return () => {
      live = false;
    };
  }, [team.id, memberCount]);

  const line = useStatusLine(status ?? EMPTY);
  if (!status) {
    return (
      <section className="mt-5">
        <h3 className="mb-2.5 px-4 text-[13px] font-semibold text-ash-300">
          {t("discover.block.title")}
        </h3>
        <div className="rounded-[var(--radius-panel)] bg-obsidian-950/40 px-4 py-3.5 ring-1 ring-white/8">
          <Skeleton className="h-4 w-36" />
          <Skeleton className="mt-2 h-3 w-full" />
        </div>
      </section>
    );
  }

  const ready = checksHold(status.checks);
  const on =
    editing || status.state === "waiting" || status.state === "listed" || status.state === "held";
  // Closed for good, or refused lately: the switch stays off (the line says why or until when).
  const locked = status.state === "closed" || status.state === "refused";
  const startEditing = () => {
    setText(status.draft?.text ?? "");
    setTags(status.draft?.tags ?? []);
    setEditing(true);
  };
  const turnOff = async () => {
    setBusy(true);
    const next = await attempt(() => discoverApi.off(team.id), discoverOff);
    setBusy(false);
    setAsking(false);
    if (next === undefined) return;
    if (next) setStatus(next);
    else setStatus({ ...status, state: "off", listed: null, changeWaits: false });
    announce(t("discover.block.offDone"));
  };
  const flip = (want: boolean) => {
    if (want) {
      startEditing();
      return;
    }
    if (editing && status.state === "off") {
      setEditing(false);
      return;
    }
    // Listed (or hidden by reports): coming back waits for another approval, so it asks once.
    if (status.state === "listed" || status.state === "held") setAsking(true);
    else void turnOff();
  };
  const send = async () => {
    if (busy || !maySend(status, text, tags)) return;
    setBusy(true);
    const next = await attempt(() => discoverApi.set(team.id, cleanText(text), tags), discoverOff);
    setBusy(false);
    if (!next) return;
    setStatus(next);
    setEditing(false);
  };
  const toggleTag = (tag: DiscoverTag, pressed: boolean) =>
    setTags((now) =>
      pressed
        ? now.length < DISCOVER_MAX_TAGS
          ? [...now, tag]
          : now
        : now.filter((x) => x !== tag),
    );
  const Icon = line ? ICONS[line.icon] : null;
  const length = textLength(text);
  const editsOut = status.editsLeft <= 0;

  return (
    <section className="mt-5">
      <h3 className="mb-2.5 px-4 text-[13px] font-semibold text-ash-300">
        {t("discover.block.title")}
      </h3>
      <div className="rounded-[var(--radius-panel)] bg-obsidian-950/40 px-4 py-3.5 ring-1 ring-white/8">
        <div className="switch-row flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <label htmlFor={switchId} className="text-[13.5px] font-medium text-bone-100">
              {t("discover.block.switch")}
            </label>
            {/* What becomes public, and who can join from the list (closed for good: nothing). */}
            {status.state === "closed" ? null : (
              <p id={helpId} className="mt-0.5 text-[12.5px] leading-snug text-ash-400">
                {t("discover.block.help")}{" "}
                {team.join === "approval"
                  ? t("discover.block.helpApproval")
                  : t("discover.block.helpOpen")}
              </p>
            )}
          </div>
          <Switch
            id={switchId}
            hideLabel
            label={t("discover.block.switch")}
            describedBy={status.state === "closed" ? undefined : helpId}
            checked={on && !locked}
            disabled={busy || locked || (!on && !ready)}
            onCheckedChange={flip}
          />
        </div>

        {/* One line of where it stands. */}
        <AnimatePresence mode="popLayout" initial={false}>
          {line && Icon && !editing ? (
            <motion.p
              key={line.text}
              className="mt-3 flex items-start gap-2 text-[13px] leading-snug text-bone-100"
              {...swap(reduce, 0.2)}
            >
              <Icon size={15} className="mt-px shrink-0 text-ash-300" aria-hidden />
              <span>{line.text}</span>
            </motion.p>
          ) : null}
        </AnimatePresence>

        {/* The requirements, before asking (only while one does not hold). */}
        {!ready && status.state === "off" ? <Needs status={status} /> : null}

        <AnimatePresence mode="popLayout" initial={false}>
          {asking ? (
            <motion.div key="ask" className="mt-3" {...rise(reduce, 4, 0.2)}>
              <p className="text-[13px] leading-snug text-bone-100">{t("discover.block.offAsk")}</p>
              <div className="mt-2.5 flex justify-end gap-2">
                <Button size="sm" variant="ghost" onClick={() => setAsking(false)}>
                  {t("discover.block.cancel")}
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={busy}
                  onClick={() => void turnOff()}
                >
                  {t("discover.block.offConfirm")}
                </Button>
              </div>
            </motion.div>
          ) : editing ? (
            <motion.form
              key="edit"
              className="mt-3"
              onSubmit={(event) => {
                event.preventDefault();
                void send();
              }}
              {...rise(reduce, 4, 0.22)}
            >
              <div className="flex items-baseline justify-between gap-3">
                <label htmlFor={textId} className="text-[12.5px] font-medium text-ash-300">
                  {t("discover.block.textLabel")}
                </label>
                <span
                  className={cn(
                    "text-[12px] tabular-nums",
                    length > DISCOVER_TEXT_MAX ? "text-flare-300" : "text-ash-400",
                  )}
                >
                  {t("discover.block.count", { count: length, max: DISCOVER_TEXT_MAX })}
                </span>
              </div>
              <textarea
                id={textId}
                rows={3}
                value={text}
                maxLength={DISCOVER_TEXT_MAX + 40}
                placeholder={t("discover.block.textPlaceholder")}
                onChange={(event) => setText(event.target.value.replace(/[\r\n]+/g, " "))}
                className={cn(fieldClass, "mt-1.5")}
              />
              <p className="mt-1 text-[12px] leading-snug text-ash-400">
                {t("discover.block.textRules")}
              </p>
              <p className="mt-3 text-[12.5px] font-medium text-ash-300">
                {t("discover.block.tagsLabel")}{" "}
                <span className="font-normal text-ash-400">{t("discover.block.tagsHelp")}</span>
              </p>
              <div className="mt-1.5 flex flex-wrap gap-1.5">
                {DISCOVER_TAGS.map((tag) => {
                  const pressed = tags.includes(tag);
                  return (
                    <ToggleChip
                      key={tag}
                      tone="chosen"
                      pressed={pressed}
                      disabled={!pressed && tags.length >= DISCOVER_MAX_TAGS}
                      onPressedChange={(next) => toggleTag(tag, next)}
                    >
                      {t(`discover.tags.${tag}`)}
                    </ToggleChip>
                  );
                })}
              </div>
              {editsOut ? (
                <p className="mt-3 text-[13px] leading-snug text-bone-100">
                  {t("discover.block.status.edits", {
                    count: daysUntil(status.editsReset, Date.now()),
                  })}
                </p>
              ) : null}
              <div className="mt-3 flex justify-end gap-2">
                <Button type="button" size="sm" variant="ghost" onClick={() => setEditing(false)}>
                  {t("discover.block.cancel")}
                </Button>
                <Button
                  type="submit"
                  size="sm"
                  variant="secondary"
                  disabled={busy || !maySend(status, text, tags)}
                >
                  {status.state === "listed"
                    ? t("discover.block.sendChange")
                    : t("discover.block.send")}
                </Button>
              </div>
            </motion.form>
          ) : on && !locked && status.state !== "held" ? (
            <motion.div key="idle" className="mt-2" {...rise(reduce, 4, 0.2)}>
              <Button
                size="sm"
                variant="ghost"
                icon={<PencilSimple size={14} />}
                className="-ml-3"
                onClick={startEditing}
              >
                {t("discover.block.edit")}
              </Button>
            </motion.div>
          ) : null}
        </AnimatePresence>
      </div>
    </section>
  );
}

/** Before the first answer (never shown: the block draws a skeleton then). */
const EMPTY: DiscoverStatus = {
  state: "off",
  listed: null,
  draft: null,
  changeWaits: false,
  waveAt: 0,
  retryAt: null,
  checks: {
    members: { need: 0, have: 0 },
    crewDays: { need: 0, have: 0 },
    leaderDays: { need: 0, have: 0 },
    visible: true,
  },
  editsLeft: 0,
  editsReset: 0,
};
