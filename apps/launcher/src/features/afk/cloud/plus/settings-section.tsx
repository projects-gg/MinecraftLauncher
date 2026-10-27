// "Ayarlar": what the assistant may do ("İnce ayar": its authority, buying and selling, how deals are
// paid, whom it deals with; afkplus-free.md §3), how it talks, when it works, what it may spend, how
// it finds things, what it does after a trade, who it ignores, and the notices. The form is one draft (design.md G6-P): edits
// wait across sections and the panel closing, the docked change bar saves or drops them, Ctrl+S saves.
// Saving puts only the player's own edits on top of the latest settings, so a rule added meanwhile
// or a change made on the site stays. A break, a blocked player, "Harcananı sıfırla" and the
// notices take effect at once. Amounts read the players' shorthands and say the number in words.
import { ClockCounterClockwise, Plus, X } from "@phosphor-icons/react";
import {
  Button,
  cn,
  Dialog,
  enter,
  IconButton,
  leave,
  Segmented,
  Select,
  scrollBehavior,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { RadioGroup } from "radix-ui";
import { type ReactNode, useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { afkFailed, isFriendly } from "../../errors";
import { Hint, inputClass, ToggleRow } from "../../settings/controls";
import type { CloudAccount } from "../api";
import { type CloudNotifyPrefs, type PlusPanel, plusApi } from "./api";
import { DOCK_HEIGHT, PlusChangeDock } from "./change-dock";
import { AmountField, GroupTitle, IntField, QuietLink, TimeField } from "./fields";
import { HistoryDialog } from "./history-dialog";
import {
  AUTHORITIES,
  applyDraft,
  blockedNameProblem,
  canSpend,
  editDraft,
  isTime,
  limitProblems,
  type PersonaTextProblem,
  PLUS_LIMITS,
  type PlusLimits,
  type PlusWindow,
  PRESETS,
  personaTextProblem,
  type SettingsPart,
  STATUS_PRESETS,
  sortedDays,
  WEEKDAYS,
  windowProblems,
  withSettings,
} from "./model";
import { untilPhrase } from "./time";
import { plusAi } from "./types";
import {
  plusKey,
  usePlusControl,
  usePlusFeatures,
  usePlusInstantSave,
  usePlusSave,
  usePlusUi,
} from "./use-plus";
import { currentLang, formatAmount } from "./words";

type Change = (update: (shown: SettingsPart) => SettingsPart) => void;

/** A group of the form: its title (Tektur), an id to scroll to, and its rows. */
function Group({
  id,
  title,
  aside,
  children,
}: {
  id: string;
  title: string;
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section id={`afk-plus-${id}`} aria-labelledby={`afk-plus-${id}-title`} className="scroll-mt-4">
      <GroupTitle id={`afk-plus-${id}-title`} aside={aside}>
        {title}
      </GroupTitle>
      {children}
    </section>
  );
}

/** A label over its control (selects and segmented choices). */
function Labeled({
  label,
  id,
  help,
  children,
}: {
  label: string;
  id?: string;
  help?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1.5 py-2.5">
      <span id={id} className="text-[13px] font-medium text-bone-100">
        {label}
      </span>
      {children}
      {help ? <p className="text-[12px] text-ash-400">{help}</p> : null}
    </div>
  );
}

/** One line of text the judge reads (intro, own status), with its count and its verdict. */
function PersonaText({
  label,
  value,
  max,
  placeholder,
  help,
  judge,
  problem,
  onChange,
}: {
  label: string;
  value: string;
  max: number;
  placeholder: string;
  help?: string;
  judge?: { state: string; reason: string | null } | null;
  problem: PersonaTextProblem | null;
  onChange: (value: string) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  const length = [...value].length;
  const line = problem
    ? t(`afkPlus.settings.text.${problem}`, { max })
    : judge?.state === "rejected"
      ? t(`afkPlus.settings.judgeRejected.${judge.reason ?? "other"}`, {
          defaultValue: t("afkPlus.settings.judgeRejected.other"),
        })
      : judge?.state === "pending"
        ? t("afkPlus.settings.judgePending")
        : (help ?? null);
  const bad = Boolean(problem) || judge?.state === "rejected";
  return (
    <div className="flex flex-col gap-1.5 py-2.5">
      <label htmlFor={id} className="text-[13px] font-medium text-bone-100">
        {label}
      </label>
      <div className="relative">
        <input
          id={id}
          value={value}
          maxLength={max * 2}
          autoComplete="off"
          spellCheck={false}
          placeholder={placeholder}
          aria-invalid={bad ? true : undefined}
          aria-describedby={`${id}-line`}
          onChange={(event) => onChange(event.target.value)}
          className={cn(
            inputClass,
            "h-[34px] px-3 pr-14",
            bad && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
          )}
        />
        <span
          className={cn(
            "pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-[12px] tabular-nums transition-opacity duration-150",
            length > max * 0.8 ? "opacity-100" : "opacity-0",
            length > max ? "text-flare-300" : "text-ash-400",
          )}
        >
          {length}/{max}
        </span>
      </div>
      <p
        id={`${id}-line`}
        role={problem ? "alert" : undefined}
        className={cn(
          "min-h-[17px] text-[12px] leading-[17px]",
          bad ? "text-flare-300" : "text-ash-400",
        )}
      >
        {line}
      </p>
    </div>
  );
}

// --- Asistan ----------------------------------------------------------------------------------------

function AssistantGroup({
  shown,
  set,
  panel,
  submitted,
  saver,
  aside,
}: {
  shown: SettingsPart;
  set: Change;
  panel: PlusPanel;
  submitted: boolean;
  saver: boolean;
  aside?: ReactNode;
}) {
  const { t } = useTranslation();
  const persona = shown.persona;
  const preset = (PRESETS as string[]).includes(persona.preset) ? persona.preset : "friendly";
  const status = persona.statusPreset ?? (persona.status ? "own" : "none");
  const judge = panel.settings.judge;
  const savedPersona = panel.settings.config.persona;
  const introProblem = personaTextProblem(persona.intro, PLUS_LIMITS.introMax);
  const statusProblem = personaTextProblem(persona.status, PLUS_LIMITS.statusMax);
  const setPersona = (parts: Partial<SettingsPart["persona"]>) =>
    set((s) => ({ ...s, persona: { ...s.persona, ...parts } }));
  return (
    <Group id="assistant" title={t("afkPlus.settings.groups.assistant")} aside={aside}>
      <Labeled label={t("afkPlus.settings.persona")}>
        <Segmented
          label={t("afkPlus.settings.persona")}
          value={preset}
          onValueChange={(value) => setPersona({ preset: value })}
          options={PRESETS.map((value) => ({
            value,
            label: t(`afkPlus.settings.presets.${value}`),
          }))}
          className="self-start"
        />
        {/* What a reply sounds like: the bot's own line, its mark first (every bot line has it). */}
        <p className="mt-1 flex items-baseline gap-2 text-[12px] text-ash-400">
          <span className="shrink-0">{t("afkPlus.settings.sample")}</span>
          <span className="rounded-[10px] bg-obsidian-700/70 px-2.5 py-1 text-[13px] text-bone-50 ring-1 ring-white/8 ring-inset">
            <span className="font-semibold text-gold-200">[AFK+]</span>{" "}
            {t(`afkPlus.settings.samples.${preset}`)}
          </span>
        </p>
      </Labeled>
      <PersonaText
        label={t("afkPlus.settings.intro")}
        value={persona.intro}
        max={PLUS_LIMITS.introMax}
        placeholder={t("afkPlus.settings.introPlaceholder")}
        help={t("afkPlus.settings.introHelp")}
        // The verdict is about the saved words: it shows only while they are on screen.
        judge={persona.intro === savedPersona.intro && persona.intro ? judge.intro : null}
        problem={submitted || introProblem === "length" ? introProblem : null}
        onChange={(intro) => setPersona({ intro })}
      />
      <Labeled label={t("afkPlus.settings.scope")}>
        <Select
          label={t("afkPlus.settings.scope")}
          value={
            ["all", "trade", "off"].includes(shown.replies.scope) ? shown.replies.scope : "trade"
          }
          onValueChange={(scope) => set((s) => ({ ...s, replies: { ...s.replies, scope } }))}
          options={(["all", "trade", "off"] as const).map((value) => ({
            value,
            label: t(`afkPlus.settings.scopes.${value}`),
          }))}
          className="max-w-[300px]"
        />
      </Labeled>
      <ToggleRow
        title={t("afkPlus.settings.smallTalk")}
        help={t("afkPlus.settings.smallTalkHelp")}
        checked={persona.smallTalk}
        onCheckedChange={(smallTalk) => setPersona({ smallTalk })}
      />
      <ToggleRow
        title={t("afkPlus.settings.away")}
        checked={shown.replies.away}
        onCheckedChange={(away) => set((s) => ({ ...s, replies: { ...s.replies, away } }))}
      />
      {/* What the away line says ("Şu an AFK'yım: Akşam dönerim"), right under the switch that sends
          it. Never greyed out with it: breaks and the hours outside the working windows use it too. */}
      <Labeled label={t("afkPlus.settings.status")}>
        <Select
          label={t("afkPlus.settings.status")}
          value={status}
          onValueChange={(value) =>
            value === "own"
              ? setPersona({ statusPreset: null, status: persona.status })
              : value === "none"
                ? setPersona({ statusPreset: null, status: "" })
                : setPersona({ statusPreset: value, status: "" })
          }
          options={[
            { value: "none", label: t("afkPlus.settings.statusNone") },
            ...STATUS_PRESETS.map((value) => ({
              value,
              label: t(`afkPlus.settings.statusPresets.${value}`),
            })),
            { value: "own", label: t("afkPlus.settings.statusOwn") },
          ]}
          className="max-w-[300px]"
        />
      </Labeled>
      {status === "own" ? (
        <PersonaText
          label={t("afkPlus.settings.statusText")}
          value={persona.status}
          max={PLUS_LIMITS.statusMax}
          placeholder={t("afkPlus.settings.statusPlaceholder")}
          judge={persona.status === savedPersona.status && persona.status ? judge.status : null}
          problem={submitted || statusProblem === "length" ? statusProblem : null}
          onChange={(text) => setPersona({ status: text, statusPreset: null })}
        />
      ) : null}
      <Labeled label={t("afkPlus.settings.lang")}>
        <Segmented
          label={t("afkPlus.settings.lang")}
          value={["auto", "tr", "en"].includes(shown.lang) ? shown.lang : "auto"}
          onValueChange={(lang) => set((s) => ({ ...s, lang }))}
          // Language names are written in their own language, never translated (design.md §7).
          options={[
            { value: "auto", label: t("afkPlus.settings.langs.auto") },
            { value: "tr", label: "Türkçe" },
            { value: "en", label: "English" },
          ]}
          className="self-start"
        />
      </Labeled>
      {saver ? (
        <Labeled label={t("afkPlus.settings.saver")}>
          <Segmented
            label={t("afkPlus.settings.saver")}
            value={shown.replies.saveAt === 50 ? "50" : shown.replies.saveAt === 80 ? "80" : "off"}
            onValueChange={(value) =>
              set((s) => ({
                ...s,
                replies: { ...s.replies, saveAt: value === "off" ? null : Number(value) },
              }))
            }
            options={(["80", "50", "off"] as const).map((value) => ({
              value,
              label: t(`afkPlus.settings.savers.${value}`),
            }))}
            className="self-start"
          />
        </Labeled>
      ) : null}
    </Group>
  );
}

// --- Çalışma saatleri ve mola (QoL Q3) ---------------------------------------------------------------

function DayChips({
  days,
  onChange,
  invalid,
}: {
  days: number[];
  onChange: (days: number[]) => void;
  invalid: boolean;
}) {
  const { t } = useTranslation();
  return (
    <fieldset
      aria-invalid={invalid ? true : undefined}
      className="m-0 flex min-w-0 flex-wrap gap-1 border-0 p-0"
    >
      <legend className="sr-only">{t("afkPlus.settings.daysLabel")}</legend>
      {WEEKDAYS.map((day) => {
        const on = days.includes(day);
        return (
          <button
            key={day}
            type="button"
            aria-pressed={on}
            aria-label={t(`afkPlus.settings.daysLong.d${day}`)}
            onClick={() =>
              onChange(sortedDays(on ? days.filter((d) => d !== day) : [...days, day]))
            }
            className={cn(
              "chamfer h-[28px] w-[38px] cursor-pointer text-[12px] font-semibold outline-none [--cut:5px]",
              "transition-[background-color,color,transform,scale] duration-150 ease-out active:scale-[0.96] focus-visible:ring-2 focus-visible:ring-gold-300",
              on
                ? "bg-gold-500 text-gold-ink hover:bg-gold-400"
                : "bg-white/[0.06] text-ash-300 hover:bg-white/10 hover:text-bone-100",
            )}
          >
            {t(`afkPlus.settings.days.d${day}`)}
          </button>
        );
      })}
    </fieldset>
  );
}

function WindowRow({
  window,
  onChange,
  onRemove,
  showProblems,
}: {
  window: PlusWindow;
  onChange: (window: PlusWindow) => void;
  onRemove: () => void;
  showProblems: boolean;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const problems = windowProblems(window);
  const daysBad = showProblems && problems.includes("days");
  const timeBad = showProblems && problems.includes("time");
  return (
    <motion.li
      layout={layout ? "position" : false}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: enter(0.22) }}
      exit={{ opacity: 0, transition: leave(0.15) }}
      className="flex flex-col gap-1.5 rounded-[var(--radius-panel)] bg-white/[0.03] px-3 py-2.5"
    >
      <div className="flex flex-wrap items-end gap-x-4 gap-y-2">
        <DayChips
          days={window.days}
          invalid={daysBad}
          onChange={(days) => onChange({ ...window, days })}
        />
        <div className="flex items-end gap-2">
          <TimeField
            label={t("afkPlus.settings.from")}
            value={window.from}
            invalid={timeBad && !isTime(window.from)}
            onChange={(from) => from && onChange({ ...window, from })}
          />
          <TimeField
            label={t("afkPlus.settings.to")}
            value={window.to}
            invalid={timeBad && !isTime(window.to)}
            onChange={(to) => to && onChange({ ...window, to })}
          />
        </div>
        <IconButton
          label={t("afkPlus.settings.removeWindow")}
          icon={<X size={15} />}
          size="sm"
          className="mb-[3px] ml-auto"
          onClick={onRemove}
        />
      </div>
      {daysBad || timeBad ? (
        <p role="alert" className="text-[12px] text-flare-300">
          {daysBad ? t("afkPlus.settings.windowErrorDays") : t("afkPlus.settings.windowErrorTime")}
        </p>
      ) : null}
    </motion.li>
  );
}

/** "Mola ver": until a day 1 to 30 days ahead; saved at once, ended at once. */
function BreakRow({ account, panel }: { account: CloudAccount; panel: PlusPanel }) {
  const { t } = useTranslation();
  const save = usePlusInstantSave(account.id);
  const [picking, setPicking] = useState(false);
  const [days, setDays] = useState<(typeof PLUS_LIMITS.breakDays)[number]>(3);
  const lang = currentLang();
  const now = Date.now();
  const until = panel.settings.config.pauseUntil;
  const onBreak = until !== null && until * 1000 > now;
  // A break ends at the start of the day after its last day, in the computer's time.
  const end = (() => {
    const date = new Date(now);
    date.setDate(date.getDate() + days);
    date.setHours(0, 0, 0, 0);
    return Math.floor(date.getTime() / 1000);
  })();
  const take = async () => {
    setPicking(false);
    const done = await save(
      (config) => ({ ...config, pauseUntil: end }),
      "afkPlus.settings.failed",
    );
    if (done) toast.success(t("afkPlus.settings.breakSet"));
  };
  const finish = async () => {
    const done = await save(
      (config) => ({ ...config, pauseUntil: null }),
      "afkPlus.settings.failed",
    );
    if (done) toast.success(t("afkPlus.settings.breakEnded"));
  };
  return (
    <div className="mt-3 border-t border-white/7 pt-3">
      <AnimatePresence initial={false} mode="popLayout">
        {onBreak ? (
          <motion.div
            key="on"
            className="flex flex-wrap items-center justify-between gap-3"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.14) }}
          >
            <p className="text-[13.5px] font-medium text-bone-50">
              {t("afkPlus.settings.breakOn", {
                when: untilPhrase(until ?? 0, now, lang),
              })}
            </p>
            <Button size="sm" variant="secondary" onClick={() => void finish()}>
              {t("afkPlus.settings.breakEnd")}
            </Button>
          </motion.div>
        ) : picking ? (
          <motion.div
            key="pick"
            className="flex flex-col gap-2.5"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.14) }}
          >
            {/* The row keeps its name while the length is picked ("Süre" names the choice). */}
            <span className="text-[13.5px] font-medium text-bone-50">
              {t("afkPlus.settings.breakTitle")}
            </span>
            <Segmented
              label={t("afkPlus.settings.breakFor")}
              value={String(days)}
              onValueChange={(value) =>
                setDays(Number(value) as (typeof PLUS_LIMITS.breakDays)[number])
              }
              options={PLUS_LIMITS.breakDays.map((value) => ({
                value: String(value),
                label: t("afkPlus.settings.breakDays", { count: value }),
              }))}
              className="self-start"
            />
            <div className="flex flex-wrap items-center gap-3">
              <p className="text-[12.5px] text-ash-300">
                {t("afkPlus.settings.breakPreview", { when: untilPhrase(end, now, lang) })}
              </p>
              <div className="ml-auto flex gap-2">
                <Button size="sm" variant="ghost" onClick={() => setPicking(false)}>
                  {t("afkPlus.settings.discard")}
                </Button>
                <Button size="sm" variant="primary" onClick={() => void take()}>
                  {t("afkPlus.settings.breakTake")}
                </Button>
              </div>
            </div>
          </motion.div>
        ) : (
          <motion.div
            key="off"
            className="flex flex-wrap items-center justify-between gap-3"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.14) }}
          >
            <div>
              <p className="text-[13.5px] font-medium text-bone-50">
                {t("afkPlus.settings.breakTitle")}
              </p>
              <p className="text-[12.5px] text-ash-400">{t("afkPlus.settings.breakHelp")}</p>
            </div>
            <Button size="sm" variant="secondary" onClick={() => setPicking(true)}>
              {t("afkPlus.settings.breakTake")}
            </Button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function HoursGroup({
  shown,
  set,
  account,
  panel,
  submitted,
}: {
  shown: SettingsPart;
  set: Change;
  account: CloudAccount;
  panel: PlusPanel;
  submitted: boolean;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const schedule = shown.schedule;
  const setWindows = (windows: PlusWindow[]) =>
    set((s) => ({ ...s, schedule: { ...s.schedule, windows } }));
  return (
    <Group id="hours" title={t("afkPlus.settings.groups.hours")}>
      <ToggleRow
        title={t("afkPlus.settings.hoursSwitch")}
        help={t("afkPlus.settings.hoursNote")}
        checked={schedule.on}
        onCheckedChange={(on) =>
          set((s) => ({
            ...s,
            schedule: {
              on,
              // The first time: weekday evenings, so the switch never means "never".
              windows:
                on && !s.schedule.windows.length
                  ? [{ days: [1, 2, 3, 4, 5, 6, 7], from: "16:00", to: "23:00" }]
                  : s.schedule.windows,
            },
          }))
        }
      />
      <AnimatePresence initial={false}>
        {schedule.on ? (
          <motion.div
            key="windows"
            layout={layout ? "position" : false}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.22) }}
            exit={{ opacity: 0, transition: leave(0.15) }}
            className="flex flex-col gap-2 pt-1"
          >
            <ul className="flex flex-col gap-2">
              <AnimatePresence initial={false}>
                {schedule.windows.map((window, index) => (
                  <WindowRow
                    // biome-ignore lint/suspicious/noArrayIndexKey: windows have no ids; a removal re-keys the rest, which is fine for plain fields
                    key={index}
                    window={window}
                    showProblems={submitted}
                    onChange={(next) =>
                      setWindows(schedule.windows.map((w, i) => (i === index ? next : w)))
                    }
                    onRemove={() => setWindows(schedule.windows.filter((_, i) => i !== index))}
                  />
                ))}
              </AnimatePresence>
            </ul>
            <div className="flex flex-wrap items-center gap-3">
              <Button
                size="sm"
                variant="ghost"
                icon={<Plus size={14} weight="bold" />}
                disabled={schedule.windows.length >= PLUS_LIMITS.windowsMax}
                onClick={() =>
                  setWindows([...schedule.windows, { days: [6, 7], from: "12:00", to: "23:00" }])
                }
              >
                {t("afkPlus.settings.addWindow")}
              </Button>
              <span className="text-[12px] text-ash-400">
                {schedule.windows.length >= PLUS_LIMITS.windowsMax
                  ? t("afkPlus.settings.windowsFull")
                  : t("afkPlus.settings.turkeyTime")}
              </span>
            </div>
          </motion.div>
        ) : null}
      </AnimatePresence>
      <BreakRow account={account} panel={panel} />
    </Group>
  );
}

// --- İnce ayar --------------------------------------------------------------------------------------

/** "Kimlerle → Yalnız arkadaşlarım": the in-game names it deals with (part of the draft). */
function FriendsList({
  shown,
  set,
  panel,
  refused,
}: {
  shown: SettingsPart;
  set: Change;
  panel: PlusPanel;
  /** The places of names the service refused at the last save. */
  refused: ReadonlySet<number>;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const id = useId();
  const [name, setName] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const friends = shown.who.friends;
  const max = panel.settings.rules.friendsMax || PLUS_LIMITS.friendsMax;
  const add = () => {
    const value = name.trim();
    const found = blockedNameProblem(value, friends, panel.settings.config.blocked);
    if (found) {
      setProblem(t(`afkPlus.settings.friendsErrors.${found}`));
      return;
    }
    if (friends.length >= max) {
      setProblem(t("afkPlus.settings.friendsErrors.full", { count: max }));
      return;
    }
    set((s) => ({ ...s, who: { ...s.who, friends: [...s.who.friends, value] } }));
    setName("");
  };
  return (
    <div className="flex flex-col gap-2 pb-2.5">
      <div className="flex flex-col gap-1.5">
        <label htmlFor={id} className="sr-only">
          {t("afkPlus.settings.friendsPlaceholder")}
        </label>
        <div className="flex gap-2">
          <input
            id={id}
            value={name}
            maxLength={16}
            autoComplete="off"
            spellCheck={false}
            placeholder={t("afkPlus.settings.friendsPlaceholder")}
            aria-invalid={problem ? true : undefined}
            aria-describedby={problem ? `${id}-error` : undefined}
            onChange={(event) => {
              setName(event.target.value);
              setProblem(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                add();
              }
            }}
            className={cn(
              inputClass,
              "h-[34px] max-w-[240px] px-3",
              problem && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
            )}
          />
          <Button variant="secondary" disabled={!name.trim()} onClick={add}>
            {t("afkPlus.settings.friendsAdd")}
          </Button>
        </div>
        {problem ? (
          <p id={`${id}-error`} role="alert" className="text-[12px] text-flare-300">
            {problem}
          </p>
        ) : null}
      </div>
      {refused.size ? (
        <p role="alert" className="text-[12px] text-flare-300">
          {t("afkPlus.settings.friendsErrors.refused")}
        </p>
      ) : null}
      {friends.length ? (
        <ul className="flex flex-wrap gap-1.5">
          <AnimatePresence initial={false}>
            {friends.map((entry, index) => (
              <motion.li
                key={entry}
                layout={layout ? "position" : false}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: enter(0.2) }}
                exit={{ opacity: 0, transition: leave(0.14) }}
                className={cn(
                  "flex items-center gap-1 rounded-md bg-white/[0.05] py-0.5 pr-0.5 pl-2.5 text-[13px] text-bone-100",
                  refused.has(index) && "ring-1 ring-flare-400/60",
                )}
              >
                <span data-selectable>{entry}</span>
                <IconButton
                  label={t("afkPlus.settings.friendsRemove", { name: entry })}
                  icon={<X size={13} />}
                  size="sm"
                  onClick={() =>
                    set((s) => ({
                      ...s,
                      who: { ...s.who, friends: s.who.friends.filter((f) => f !== entry) },
                    }))
                  }
                />
              </motion.li>
            ))}
          </AnimatePresence>
        </ul>
      ) : (
        <p className="text-[12.5px] text-ash-300">{t("afkPlus.settings.friendsEmpty")}</p>
      )}
    </div>
  );
}

function FineTuneGroup({
  shown,
  set,
  panel,
  aside,
  serverIssues,
}: {
  shown: SettingsPart;
  set: Change;
  panel: PlusPanel;
  submitted?: boolean;
  aside?: ReactNode;
  /** The service's refusals of the last save (`who.friends[2]`, `pay`, …). */
  serverIssues: ReadonlySet<string>;
}) {
  const { t } = useTranslation();
  const id = useId();
  const itemPay = panel.settings.rules.features.itemPay;
  const authority = AUTHORITIES.find((a) => a === shown.authority) ?? "ask";
  const pay = (["money", "items", "both"] as const).find((p) => p === shown.pay) ?? "money";
  const who = shown.who.mode === "friends" ? "friends" : "all";
  const refusedFriends = new Set<number>();
  let refusedHere = false;
  for (const field of serverIssues) {
    const friend = /^who\.friends\[(\d+)\]/.exec(field);
    if (friend) refusedFriends.add(Number(friend[1]));
    else if (/^(authority|allow|pay|who)\b/.test(field)) refusedHere = true;
  }
  return (
    <Group id="fine" title={t("afkPlus.settings.groups.fine")} aside={aside}>
      {refusedHere ? (
        <p role="alert" className="pt-2.5 text-[12.5px] text-flare-300">
          {t("afkPlus.settings.fineRefused")}
        </p>
      ) : null}
      <div className="flex flex-col gap-2 py-2.5">
        <span id={`${id}-authority`} className="text-[13px] font-medium text-bone-100">
          {t("afkPlus.settings.authorityLabel")}
        </span>
        <RadioGroup.Root
          aria-labelledby={`${id}-authority`}
          value={authority}
          onValueChange={(value) => set((s) => ({ ...s, authority: value }))}
          className="flex flex-col gap-1.5"
        >
          {AUTHORITIES.map((value) => (
            <RadioGroup.Item
              key={value}
              value={value}
              className="group flex w-full cursor-pointer items-start gap-3 rounded-[var(--radius-panel)] px-3 py-2.5 text-left outline-none ring-1 ring-white/[0.07] transition-[background-color,box-shadow] duration-150 hover:bg-white/[0.04] focus-visible:ring-2 focus-visible:ring-gold-300 data-[state=checked]:bg-gold-500/[0.07] data-[state=checked]:ring-gold-400/40"
            >
              <span
                aria-hidden
                className="mt-[3px] flex size-4 shrink-0 items-center justify-center rounded-full border border-white/25 transition-colors duration-150 group-data-[state=checked]:border-gold-400"
              >
                <RadioGroup.Indicator className="size-2 rounded-full bg-gold-400" />
              </span>
              <span className="min-w-0">
                <span className="block text-[13.5px] font-medium text-bone-50">
                  {t(`afkPlus.settings.authority.${value}`)}
                </span>
                <span className="block text-[12.5px] text-ash-300">
                  {t(`afkPlus.settings.authorityHelp.${value}`)}
                </span>
              </span>
            </RadioGroup.Item>
          ))}
        </RadioGroup.Root>
      </div>
      <ToggleRow
        title={t("afkPlus.settings.allowBuy")}
        checked={shown.allow.buy}
        onCheckedChange={(buy) => set((s) => ({ ...s, allow: { ...s.allow, buy } }))}
      />
      <ToggleRow
        title={t("afkPlus.settings.allowSell")}
        checked={shown.allow.sell}
        onCheckedChange={(sell) => set((s) => ({ ...s, allow: { ...s.allow, sell } }))}
      />
      {itemPay ? (
        <Labeled label={t("afkPlus.settings.pay")}>
          <Segmented
            label={t("afkPlus.settings.pay")}
            value={pay}
            onValueChange={(next) => set((s) => ({ ...s, pay: next }))}
            className="self-start"
            options={[
              { value: "money", label: t("afkPlus.settings.pays.money") },
              { value: "items", label: t("afkPlus.settings.pays.items") },
              { value: "both", label: t("afkPlus.settings.pays.both") },
            ]}
          />
        </Labeled>
      ) : null}
      <Labeled label={t("afkPlus.settings.who")}>
        <Segmented
          label={t("afkPlus.settings.who")}
          value={who}
          onValueChange={(mode) => set((s) => ({ ...s, who: { ...s.who, mode } }))}
          className="self-start"
          options={[
            { value: "all", label: t("afkPlus.settings.whoAll") },
            { value: "friends", label: t("afkPlus.settings.whoFriends") },
          ]}
        />
      </Labeled>
      {who === "friends" ? (
        <FriendsList shown={shown} set={set} panel={panel} refused={refusedFriends} />
      ) : null}
    </Group>
  );
}

// --- Harcama -----------------------------------------------------------------------------------------

function LimitsGroup({
  shown,
  set,
  account,
  panel,
  submitted,
  serverIssues,
}: {
  shown: SettingsPart;
  set: Change;
  account: CloudAccount;
  panel: PlusPanel;
  submitted: boolean;
  serverIssues: ReadonlySet<string>;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const control = usePlusControl(account.id);
  const [confirmReset, setConfirmReset] = useState(false);
  const limits = shown.limits;
  const maxDeal = panel.settings.server.money.maxDeal;
  const problems = limitProblems(limits, maxDeal);
  const spent = panel.settings.spent ?? plusAi(account)?.spent ?? null;
  const setLimit = (key: keyof PlusLimits, value: number | null | undefined) => {
    // `undefined`: the text is not an amount yet; the field says so and the draft keeps the last good value.
    if (value === undefined) return;
    set((s) => ({ ...s, limits: { ...s.limits, [key]: value } }));
  };
  const error = (key: keyof PlusLimits): string | null => {
    const problem = problems[key];
    if (!(submitted || problem === "approvalAbove") && !serverIssues.has(`limits.${key}`)) {
      return null;
    }
    if (!problem) {
      return serverIssues.has(`limits.${key}`) ? t("afkPlus.settings.limitErrors.amount") : null;
    }
    return t(`afkPlus.settings.limitErrors.${problem}`, { max: formatAmount(maxDeal) });
  };
  const amount = (
    key: keyof PlusLimits,
    label: string,
    optional: boolean,
    more: { empty?: string; extra?: ReactNode } = {},
  ) => (
    <AmountField
      label={label}
      value={limits[key] as number | null}
      optional={optional}
      placeholder={optional ? (more.empty ?? t("afkPlus.settings.noLimitShort")) : undefined}
      error={error(key)}
      extra={more.extra}
      onChange={(value) => setLimit(key, value)}
    />
  );
  const note = (text: string) => <p className="-mt-1 text-[12px] text-ash-400">{text}</p>;
  const reset = async () => {
    setConfirmReset(false);
    const done = await control("reset_spent");
    if (done) {
      toast.success(t("afkPlus.settings.resetSpentDone"));
      void queryClient.invalidateQueries({ queryKey: plusKey(account.id) });
    }
  };
  return (
    <Group id="limits" title={t("afkPlus.settings.groups.limits")}>
      {/* Said once, under the title, so no limit is ever read as lira (the contract has no name
          for a mode's money); not in the title row, where the other groups keep their controls. */}
      <p className="mb-1.5 text-[12.5px] text-ash-400">{t("afkPlus.settings.limitsMoney")}</p>
      {!canSpend(limits) ? (
        <Hint tone="warn" className="mb-1">
          {t("afkPlus.settings.needed")}
        </Hint>
      ) : null}
      {/* A field's label, box and words sit close; the next field starts a clear step lower. Each
          counter sits under the limit it counts against (the site's): the total counts since the
          last reset and a reset empties it only, so it never reads as today's figure gone down. */}
      <div className="grid grid-cols-2 gap-x-4 gap-y-4 pt-1.5">
        {amount("perDealMax", t("afkPlus.settings.perDealMax"), true)}
        {amount("totalBudget", t("afkPlus.settings.totalBudget"), true, {
          extra:
            spent && spent.total > 0 ? (
              <p className="-mt-1 flex flex-wrap items-baseline gap-x-2 text-[12.5px] text-bone-100">
                <span className="tabular-nums">
                  {t(
                    limits.totalBudget === null
                      ? "afkPlus.settings.spentNoBudget"
                      : "afkPlus.settings.spentTotal",
                    { amount: formatAmount(spent.total) },
                  )}
                </span>
                <QuietLink onClick={() => setConfirmReset(true)}>
                  {t("afkPlus.settings.resetSpent")}
                </QuietLink>
              </p>
            ) : null,
        })}
        {amount("dailySpendMax", t("afkPlus.settings.dailySpendMax"), true, {
          extra:
            spent && spent.today > 0 ? (
              <p className="-mt-1 text-[12.5px] text-bone-100 tabular-nums">
                {t("afkPlus.settings.spentToday", { amount: formatAmount(spent.today) })}
              </p>
            ) : null,
        })}
        {amount("balanceFloor", t("afkPlus.settings.balanceFloor"), false)}
        {amount("approvalAbove", t("afkPlus.settings.approvalAbove"), true, {
          empty: t("afkPlus.settings.noAskShort"),
          extra: note(t("afkPlus.settings.approvalHelp")),
        })}
        {amount("newCounterpartApprovalAbove", t("afkPlus.settings.newCounterpart"), true, {
          empty: t("afkPlus.settings.noAskShort"),
          extra: note(t("afkPlus.settings.newCounterpartHelp")),
        })}
      </div>
      <IntField
        label={t("afkPlus.settings.timeout")}
        value={limits.approvalTimeoutMin}
        onChange={(value) => value !== null && setLimit("approvalTimeoutMin", value)}
        after={t("afkPlus.settings.timeoutValue")}
        error={
          submitted && problems.approvalTimeoutMin
            ? t("afkPlus.settings.limitErrors.timeout")
            : null
        }
        className="pt-3"
        inputClassName="w-[72px] text-center"
      />
      {/* afkplus-free.md §3.5: the one quiet line, never a switch. */}
      <p className="pt-3 text-[12.5px] text-ash-400">{t("afkPlus.settings.secret")}</p>
      <Dialog
        open={confirmReset}
        onOpenChange={setConfirmReset}
        title={t("afkPlus.settings.resetSpentTitle")}
        description={t("afkPlus.settings.resetSpentBody")}
        closeLabel={t("common.close")}
        width={420}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirmReset(false)}>
              {t("afkPlus.settings.discard")}
            </Button>
            <Button variant="primary" onClick={() => void reset()}>
              {t("afkPlus.settings.resetSpent")}
            </Button>
          </>
        }
      />
    </Group>
  );
}

// --- Bulma ve Takas ----------------------------------------------------------------------------------

function FindGroup({ shown, set, panel }: { shown: SettingsPart; set: Change; panel: PlusPanel }) {
  const { t } = useTranslation();
  const d = shown.discovery;
  const lang = currentLang();
  const templates = panel.settings.server.ads;
  const setD = (parts: Partial<SettingsPart["discovery"]>) =>
    set((s) => ({ ...s, discovery: { ...s.discovery, ...parts } }));
  return (
    <Group id="discovery" title={t("afkPlus.settings.groups.discovery")}>
      <ToggleRow
        title={t("afkPlus.settings.scanChat")}
        checked={d.scanChat}
        onCheckedChange={(scanChat) => setD({ scanChat, proactivePm: scanChat && d.proactivePm })}
      />
      <ToggleRow
        title={t("afkPlus.settings.proactivePm")}
        checked={d.scanChat && d.proactivePm}
        disabled={!d.scanChat}
        onCheckedChange={(proactivePm) => setD({ proactivePm })}
      />
      <ToggleRow
        title={t("afkPlus.settings.ads")}
        help={t("afkPlus.settings.adsHelp")}
        checked={d.ads}
        disabled={!templates.length}
        onCheckedChange={(ads) =>
          setD({ ads, adTemplate: d.adTemplate || (templates[0]?.id ?? d.adTemplate) })
        }
      />
      {d.ads && templates.length ? (
        <Labeled label={t("afkPlus.settings.adTemplate")}>
          <Select
            label={t("afkPlus.settings.adTemplate")}
            value={
              templates.some((tpl) => tpl.id === d.adTemplate)
                ? d.adTemplate
                : (templates[0]?.id ?? "")
            }
            onValueChange={(adTemplate) => setD({ adTemplate })}
            options={templates.map((tpl) => ({
              value: tpl.id,
              label: lang === "en" ? tpl.en : tpl.tr,
            }))}
            className="max-w-[360px]"
          />
        </Labeled>
      ) : null}
    </Group>
  );
}

function TradeGroup({
  shown,
  set,
  account,
  submitted,
}: {
  shown: SettingsPart;
  set: Change;
  account: CloudAccount;
  submitted: boolean;
}) {
  const { t } = useTranslation();
  // A plain command row of the account to run after a trade (never a secret one).
  const rows = account.config.commands.filter((command) => !command.secret && command.text);
  const value = rows.some((row) => row.id === shown.trade.returnRow)
    ? (shown.trade.returnRow ?? "stay")
    : "stay";
  return (
    <Group id="trade" title={t("afkPlus.settings.groups.trade")}>
      <Labeled label={t("afkPlus.settings.returnRow")} help={t("afkPlus.settings.returnHelp")}>
        <Select
          label={t("afkPlus.settings.returnRow")}
          value={value}
          onValueChange={(next) =>
            set((s) => ({ ...s, trade: { returnRow: next === "stay" ? null : next } }))
          }
          options={[
            { value: "stay", label: t("afkPlus.settings.returnStay") },
            ...rows.map((row) => ({ value: row.id, label: row.text ?? row.id })),
          ]}
          className="max-w-[360px]"
        />
      </Labeled>
      <div className="py-2.5">
        <TimeField
          label={t("afkPlus.settings.summaryAt")}
          value={shown.summaryAt}
          invalid={submitted && !isTime(shown.summaryAt)}
          onChange={(time) => time && set((s) => ({ ...s, summaryAt: time }))}
        />
      </div>
    </Group>
  );
}

// --- Engellenenler (at once) ---------------------------------------------------------------------------

function BlockedGroup({ account, panel }: { account: CloudAccount; panel: PlusPanel }) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const control = usePlusControl(account.id);
  const id = useId();
  const [name, setName] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const blocked = panel.settings.config.blocked;
  const add = async () => {
    const value = name.trim();
    const found = blockedNameProblem(value, blocked);
    if (found) {
      setProblem(
        t(
          found === "format"
            ? "afkPlus.settings.blockedFormat"
            : "afkPlus.settings.blockedDuplicate",
        ),
      );
      return;
    }
    if (blocked.length >= PLUS_LIMITS.blockedMax) return;
    const done = await control("block", value);
    if (done) {
      setName("");
      toast.success(t("afkPlus.messages.blockedDone", { name: value }));
    }
  };
  return (
    <Group id="blocked" title={t("afkPlus.settings.groups.blocked")}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void add();
        }}
        className="flex flex-col gap-1.5 py-2"
      >
        <label htmlFor={id} className="sr-only">
          {t("afkPlus.settings.blockedPlaceholder")}
        </label>
        <div className="flex gap-2">
          <input
            id={id}
            value={name}
            maxLength={16}
            autoComplete="off"
            spellCheck={false}
            placeholder={t("afkPlus.settings.blockedPlaceholder")}
            aria-invalid={problem ? true : undefined}
            aria-describedby={problem ? `${id}-error` : undefined}
            onChange={(event) => {
              setName(event.target.value);
              setProblem(null);
            }}
            className={cn(
              inputClass,
              "h-[34px] max-w-[240px] px-3",
              problem && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
            )}
          />
          <Button type="submit" variant="secondary" disabled={!name.trim()}>
            {t("afkPlus.settings.blockedAdd")}
          </Button>
        </div>
        {problem ? (
          <p id={`${id}-error`} role="alert" className="text-[12px] text-flare-300">
            {problem}
          </p>
        ) : null}
      </form>
      {blocked.length ? (
        <ul className="flex flex-wrap gap-1.5">
          <AnimatePresence initial={false}>
            {blocked.map((entry) => (
              <motion.li
                key={entry}
                layout={layout ? "position" : false}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: enter(0.2) }}
                exit={{ opacity: 0, transition: leave(0.14) }}
                className="flex items-center gap-1 rounded-md bg-white/[0.05] py-0.5 pr-0.5 pl-2.5 text-[13px] text-bone-100"
              >
                <span data-selectable>{entry}</span>
                <IconButton
                  label={t("afkPlus.settings.blockedRemove", { name: entry })}
                  icon={<X size={13} />}
                  size="sm"
                  onClick={() =>
                    void control("unblock", entry).then((done) => {
                      if (done) toast.success(t("afkPlus.messages.unblockedDone", { name: entry }));
                    })
                  }
                />
              </motion.li>
            ))}
          </AnimatePresence>
        </ul>
      ) : (
        <p className="text-[12.5px] text-ash-400">{t("afkPlus.settings.blockedEmpty")}</p>
      )}
    </Group>
  );
}

// --- Bildirimler (at once, every cloud account) ---------------------------------------------------------

const NOTIFY_GROUPS: Array<{ key: string; events: string[] }> = [
  // The assistant pausing is the account's state, not a summary.
  { key: "account", events: ["stopped", "paused", "renewal", "expired"] },
  { key: "deals", events: ["approval", "found", "deal_done", "deal_failed"] },
  { key: "messages", events: ["message", "incident"] },
  { key: "allowance", events: ["quota_full", "quota_80", "gift_ending"] },
  { key: "recaps", events: ["summary", "weekly"] },
];

function NotifyGroup() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: ["afk-plus-notify"],
    queryFn: () => plusApi.notifyPrefs(),
    staleTime: 60_000,
    refetchOnWindowFocus: false,
    retry: false,
  });
  const prefs = query.data;
  const write = async (next: CloudNotifyPrefs) => {
    const before = prefs;
    queryClient.setQueryData(["afk-plus-notify"], next);
    try {
      queryClient.setQueryData(["afk-plus-notify"], await plusApi.setNotifyPrefs(next));
    } catch (error) {
      if (before) queryClient.setQueryData(["afk-plus-notify"], before);
      afkFailed(error, t("afkPlus.notify.failed"));
    }
  };
  return (
    <Group id="notify" title={t("afkPlus.settings.groups.notify")}>
      <p className="-mt-0.5 mb-1 text-[12.5px] text-ash-400">{t("afkPlus.settings.notifyAll")}</p>
      {!prefs ? (
        query.isError ? (
          <p className="py-2 text-[12.5px] text-ash-300">{t("afkPlus.notify.loadFailed")}</p>
        ) : (
          <div className="h-[220px]" aria-hidden />
        )
      ) : (
        <>
          {/* One column, in groups: a switch sits on its own row's line, never beside another's words. */}
          <div className="flex flex-col gap-3">
            {NOTIFY_GROUPS.map((group) => (
              <fieldset key={group.key} className="m-0 min-w-0 border-0 p-0">
                <legend className="p-0 pt-1 text-[12px] font-semibold text-ash-300">
                  {t(`afkPlus.notify.groups.${group.key}`)}
                </legend>
                {group.events.map((event) => (
                  <ToggleRow
                    key={event}
                    title={t(`afkPlus.notify.events.${event}`)}
                    checked={prefs.events[event] ?? true}
                    onCheckedChange={(on) =>
                      void write({ ...prefs, events: { ...prefs.events, [event]: on } })
                    }
                  />
                ))}
              </fieldset>
            ))}
          </div>
          <div className="mt-2 border-t border-white/7 pt-1">
            <ToggleRow
              title={t("afkPlus.notify.quiet")}
              help={t("afkPlus.notify.quietBody")}
              checked={prefs.quiet.enabled}
              onCheckedChange={(enabled) =>
                void write({ ...prefs, quiet: { ...prefs.quiet, enabled } })
              }
            />
            {prefs.quiet.enabled ? (
              <div className="flex gap-2 pb-2">
                <TimeField
                  label={t("afkPlus.settings.from")}
                  value={prefs.quiet.from}
                  onChange={(from) =>
                    from && void write({ ...prefs, quiet: { ...prefs.quiet, from } })
                  }
                />
                <TimeField
                  label={t("afkPlus.settings.to")}
                  value={prefs.quiet.to}
                  onChange={(to) => to && void write({ ...prefs, quiet: { ...prefs.quiet, to } })}
                />
              </div>
            ) : null}
          </div>
        </>
      )}
    </Group>
  );
}

// --- The section ---------------------------------------------------------------------------------------

/** Every problem the launcher can see before sending, so "Kaydet" never sends what will be refused. */
function hasProblems(shown: SettingsPart, maxDeal: number): boolean {
  const limits = limitProblems(shown.limits, maxDeal);
  return (
    Object.keys(limits).length > 0 ||
    personaTextProblem(shown.persona.intro, PLUS_LIMITS.introMax) !== null ||
    (shown.persona.statusPreset === null &&
      personaTextProblem(shown.persona.status, PLUS_LIMITS.statusMax) !== null) ||
    (shown.schedule.on && shown.schedule.windows.some((w) => windowProblems(w).length > 0)) ||
    !isTime(shown.summaryAt)
  );
}

export function SettingsSection({
  account,
  panel,
  dock,
}: {
  account: CloudAccount;
  panel: PlusPanel;
  /** Where the change bar docks (the panel's slot at the bottom of its scroller). */
  dock: HTMLElement | null;
}) {
  const { t } = useTranslation();
  const features = usePlusFeatures();
  const save = usePlusSave(account.id);
  const draft = usePlusUi((s) => s.drafts[account.id] ?? null);
  const setDraft = usePlusUi((s) => s.setDraft);
  const focusGroup = usePlusUi((s) => s.focusGroup);
  const setFocusGroup = usePlusUi((s) => s.setFocusGroup);
  const [submitted, setSubmitted] = useState(false);
  const [serverIssues, setServerIssues] = useState<ReadonlySet<string>>(new Set());
  const [failure, setFailure] = useState<{ message: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const [history, setHistory] = useState(false);
  const saved = panel.settings.config;
  const shown = useMemo(() => applyDraft(saved, draft), [saved, draft]);
  const dirty = draft !== null;
  const maxDeal = panel.settings.server.money.maxDeal;

  const set: Change = (update) => {
    setDraft(account.id, editDraft(saved, usePlusUi.getState().drafts[account.id], update));
    setServerIssues(new Set());
    setFailure(null);
  };

  const submit = async () => {
    if (!draft || saving) return;
    setSubmitted(true);
    if (hasProblems(shown, maxDeal)) return;
    setSaving(true);
    try {
      const sent = draft;
      const outcome = await save((latest) => withSettings(latest, applyDraft(latest, sent)));
      if (outcome.kind === "invalid") {
        setServerIssues(new Set(outcome.issues.map((issue) => issue.field)));
        setFailure({ message: t("afkPlus.settings.fixFirst") });
        return;
      }
      // Edits typed while it saved stay a draft.
      if (usePlusUi.getState().drafts[account.id] === sent) setDraft(account.id, null);
      setSubmitted(false);
      toast.success(t("afkPlus.settings.saved"));
    } catch (error) {
      setFailure({ message: isFriendly(error) ? error.message : t("afkPlus.settings.failed") });
    } finally {
      setSaving(false);
    }
  };

  // Ctrl+S saves while something waits (as the instance settings and the cloud sheet do). The field
  // with the caret lets go first: an amount that is not one goes back to the last good value, so
  // what is saved is what the fields show (a click on "Kaydet" does the same by itself).
  const shortcut = useRef(() => {});
  shortcut.current = () => void submit();
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
      if (event.key.toLowerCase() !== "s") return;
      event.preventDefault();
      const active = document.activeElement;
      if (active instanceof HTMLInputElement) active.blur();
      requestAnimationFrame(() => shortcut.current());
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // "Sınırları ayarla" (Listeler) brings its group into view once.
  useEffect(() => {
    if (!focusGroup) return;
    const node = document.getElementById(`afk-plus-${focusGroup}`);
    node?.scrollIntoView({ block: "start", behavior: scrollBehavior() });
    setFocusGroup(null);
  }, [focusGroup, setFocusGroup]);

  const blocking = submitted && hasProblems(shown, maxDeal);
  const props = { shown, set, panel, submitted };

  return (
    <>
      <div className="flex flex-col gap-8">
        <FineTuneGroup
          {...props}
          serverIssues={serverIssues}
          aside={
            features.history ? (
              <Button
                size="sm"
                variant="ghost"
                icon={<ClockCounterClockwise size={15} />}
                onClick={() => setHistory(true)}
              >
                {t("afkPlus.settings.history")}
              </Button>
            ) : null
          }
        />
        <AssistantGroup {...props} saver={features.saver} />
        {features.schedule ? <HoursGroup {...props} account={account} /> : null}
        <LimitsGroup {...props} account={account} serverIssues={serverIssues} />
        <FindGroup {...props} />
        <TradeGroup {...props} account={account} />
        <BlockedGroup account={account} panel={panel} />
        <NotifyGroup />
      </div>
      {/* Always the dock's room, open or not: the last rows scroll out from under it and its coming
          and going never moves the list. */}
      <div aria-hidden className="shrink-0" style={{ height: DOCK_HEIGHT }} />

      <PlusChangeDock dock={dock} open={dirty} label={t("afkPlus.settings.barLabel")}>
        <p
          role={failure || blocking ? "alert" : undefined}
          className={cn(
            "min-w-0 flex-1 text-[12.5px]",
            failure || blocking ? "text-flare-300" : "text-bone-100",
          )}
        >
          {failure
            ? failure.message
            : blocking
              ? t("afkPlus.settings.fixFirst")
              : t("afkPlus.settings.dirty")}
        </p>
        <Button
          variant="ghost"
          disabled={saving}
          onClick={() => {
            setDraft(account.id, null);
            setSubmitted(false);
            setServerIssues(new Set());
            setFailure(null);
          }}
        >
          {t("afkPlus.settings.discard")}
        </Button>
        <Button variant="primary" disabled={saving} onClick={() => void submit()}>
          {saving ? t("afkPlus.settings.saving") : t("afkPlus.settings.save")}
        </Button>
      </PlusChangeDock>

      <HistoryDialog
        open={history}
        onClose={() => setHistory(false)}
        account={account}
        panel={panel}
      />
    </>
  );
}
