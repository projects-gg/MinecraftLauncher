import { CaretUpDown, Check, Plus, SquaresFour } from "@phosphor-icons/react";
import {
  Chip,
  cn,
  easeOutExpo,
  isCut,
  OverflowTooltip,
  PlayButton,
  type PlayState,
  Progress,
  returnFocusAfterKeys,
  Switch,
  Tooltip,
  toast,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import type { TFunction } from "i18next";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { DropdownMenu } from "radix-ui";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AccountFace, type FaceAccount } from "@/features/accounts/account-face";
import { useAccounts } from "@/features/accounts/use-accounts";
import { useOthersInGame, useOwnLaunch } from "@/features/accounts/use-own-launch";
import { RoutePicker } from "@/features/autojoin/route-picker";
import { useRoute } from "@/features/autojoin/use-route";
import { launchErrorAction, WORLD_OPEN } from "@/features/disk/launch-error";
import { helpsWithLaunchError } from "@/features/help/launch";
import { openHelp, useHelp } from "@/features/help/store";
import { sortInstances, useLibraryView } from "@/features/library/library-view";
import { useOfficialOffer } from "@/features/library/official";
import { usePrepareSync } from "@/features/prepare/use-prepare-sync";
import { ResumeButton, useResumeTarget } from "@/features/resume/resume-button";
import { api, type InstanceSummary } from "@/lib/api";
import { copyDetails } from "@/lib/clipboard";
import { formatCount, formatDuration, formatRelative, formatSpeed } from "@/lib/format";
import { detectOs } from "@/lib/platform";
import { ariaKeyShortcuts } from "@/lib/shortcuts";
import { isOpening, type LaunchPhase, useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { type PrepareHint, usePrepare } from "@/stores/prepare";
import { InstanceIcon, prewarmInstanceIcons } from "./instance-icon";

function loaderLine(i: InstanceSummary, loaderName: string) {
  return i.loader === "vanilla" ? `Minecraft ${i.mcVersion}` : `${loaderName} ${i.mcVersion}`;
}

/**
 * The picker's second line: the game and loader version, or, while the instance prepares in the
 * background, a quiet "Hazırlanıyor %42" over a thin grey bar and then "Oynamaya hazır" for a
 * moment. The lines cross-fade in place; nothing around them moves.
 */
function PickerLine({
  selected,
  hint,
}: {
  selected: InstanceSummary | undefined;
  hint: PrepareHint | null;
}) {
  const { t } = useTranslation();
  const reduce = useReducedMotion() ?? false;
  return (
    <span className="grid text-[12px] *:[grid-area:1/1]">
      <AnimatePresence initial={false}>
        {hint?.kind === "preparing" ? (
          <motion.span key="preparing" className="relative block min-w-0" {...swapIn(reduce)}>
            <span className="block truncate text-ash-300 tabular-nums">
              {t("prepare.preparing", { percent: Math.floor(hint.percent) })}
            </span>
            <span
              aria-hidden
              className="absolute inset-x-0 -bottom-[5px] h-[2px] overflow-hidden rounded-full bg-white/8"
            >
              <span
                className="block h-full origin-left rounded-full bg-ash-300/60 transition-transform duration-500 ease-out motion-reduce:transition-none"
                style={{ transform: `scaleX(${Math.min(1, Math.max(0, hint.percent / 100))})` }}
              />
            </span>
          </motion.span>
        ) : hint?.kind === "ready" ? (
          <motion.span
            key="ready"
            className="flex min-w-0 items-center gap-1 text-bone-100"
            {...swapIn(reduce)}
          >
            <Check size={12} weight="bold" className="shrink-0 text-gold-400" />
            <span className="truncate">{t("prepare.ready")}</span>
          </motion.span>
        ) : (
          <motion.span key="version" className="block truncate text-ash-300" {...swapIn(reduce)}>
            {selected ? loaderLine(selected, t(`loaders.${selected.loader}`)) : ""}
          </motion.span>
        )}
      </AnimatePresence>
    </span>
  );
}

/**
 * One instance in the picker. A long name is cut to fit the row; pointing at the row (or reaching
 * it with the arrows, which focus it) shows the whole name beside it, only when it was cut.
 */
function PickerItem({
  instance,
  chosen,
  disabled,
  onSelect,
}: {
  instance: InstanceSummary;
  chosen: boolean;
  disabled: boolean;
  onSelect: () => void;
}) {
  const { t } = useTranslation();
  const nameRef = useRef<HTMLSpanElement>(null);
  return (
    <OverflowTooltip text={instance.name} of={nameRef} side="right">
      <DropdownMenu.Item
        disabled={disabled}
        onSelect={onSelect}
        className="flex cursor-pointer items-center gap-2.5 rounded-md px-2 py-1.5 outline-none data-[disabled]:cursor-default data-[disabled]:opacity-45 data-[highlighted]:bg-white/8"
      >
        <InstanceIcon icon={instance.icon} size={32} />
        <span className="min-w-0 flex-1">
          <span ref={nameRef} className="block truncate text-[13px] font-semibold text-bone-50">
            {instance.name}
          </span>
          <span className="block truncate text-[12px] text-ash-300">
            {loaderLine(instance, t(`loaders.${instance.loader}`))}
          </span>
        </span>
        {instance.managed ? <Chip tone="gold">{t("instances.managed")}</Chip> : null}
        {chosen ? (
          <Check size={16} className="shrink-0 text-gold-400" />
        ) : (
          <span aria-hidden className="size-4 shrink-0" />
        )}
      </DropdownMenu.Item>
    </OverflowTooltip>
  );
}

function InstancePicker({
  instances,
  selected,
  loading,
  locked,
  hint,
  onSelect,
}: {
  instances: InstanceSummary[];
  selected: InstanceSummary | undefined;
  /** Still loading the list (an empty list then reads "Kurulum seç", not "Yükleniyor"). */
  loading: boolean;
  /** A game is being prepared or played: the bar stays on it until it ends. */
  locked: boolean;
  /** Background preparation of the selected instance, when there is something to show. */
  hint: PrepareHint | null;
  onSelect: (id: string) => void;
}) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  // The library's order (official packs first), so both lists read the same way.
  const sort = useLibraryView((s) => s.sort);
  const ordered = useMemo(() => {
    const sorted = sortInstances(instances, sort);
    return [...sorted.filter((i) => i.managed), ...sorted.filter((i) => !i.managed)];
  }, [instances, sort]);
  // A long name is cut here: the tooltip shows all of it (only then). It sits on a wrapper, since
  // on the button itself its `data-state` would replace the menu's (the open look).
  const nameRef = useRef<HTMLSpanElement>(null);
  return (
    <DropdownMenu.Root>
      <OverflowTooltip text={selected?.name ?? ""} of={nameRef}>
        <span className="flex shrink-0">
          <DropdownMenu.Trigger asChild>
            <button
              type="button"
              aria-label={t("instances.pickerLabel")}
              className="group flex h-[52px] w-[clamp(200px,20vw,264px)] shrink-0 cursor-pointer items-center gap-2.5 rounded-[var(--radius-panel)] px-2 text-left transition-[background-color,transform,scale] duration-150 ease-out hover:bg-white/6 active:scale-[0.985] data-[state=open]:bg-white/8"
            >
              {selected ? (
                <InstanceIcon icon={selected.icon} size={36} />
              ) : (
                <span className="size-9" />
              )}
              <span className="min-w-0 flex-1">
                <span
                  ref={nameRef}
                  className="block truncate font-display text-[15px] font-semibold text-bone-50"
                >
                  {selected?.name ?? (loading ? t("common.loading") : t("instances.pickerLabel"))}
                </span>
                <PickerLine selected={selected} hint={hint} />
              </span>
              <CaretUpDown
                size={16}
                className="shrink-0 text-ash-400 transition-colors group-hover:text-bone-100"
              />
            </button>
          </DropdownMenu.Trigger>
        </span>
      </OverflowTooltip>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          onCloseAutoFocus={returnFocusAfterKeys}
          side="top"
          align="start"
          sideOffset={10}
          className="glass-strong pop-motion z-50 w-[300px] rounded-[var(--radius-panel)] p-1 shadow-[0_24px_60px_rgb(34_6_6/0.55)]"
        >
          {/* A long library scrolls here; the two actions below always stay in view. */}
          <div className="max-h-[min(392px,calc(var(--radix-dropdown-menu-content-available-height)-88px))] overflow-y-auto">
            {ordered.map((i) => (
              <PickerItem
                key={i.id}
                instance={i}
                chosen={selected?.id === i.id}
                disabled={locked && selected?.id !== i.id}
                onSelect={() => onSelect(i.id)}
              />
            ))}
          </div>
          <DropdownMenu.Separator className="my-1 h-px bg-white/8" />
          <DropdownMenu.Item
            onSelect={() => void navigate({ to: "/library" })}
            className="flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-[7px] text-[13px] text-ash-300 outline-none data-[highlighted]:bg-white/8 data-[highlighted]:text-bone-50"
          >
            {/* The rail's Library mark: a folder would promise the files on disk ("Klasörü aç"). */}
            <SquaresFour size={16} /> {t("instances.openLibrary")}
          </DropdownMenu.Item>
          <DropdownMenu.Item
            onSelect={() => void navigate({ to: "/library", search: { create: true } })}
            className="flex cursor-pointer items-center gap-2.5 rounded-md px-2.5 py-[7px] text-[13px] text-ash-300 outline-none data-[highlighted]:bg-white/8 data-[highlighted]:text-bone-50"
          >
            <Plus size={16} /> {t("instances.create")}
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

/** In-place swap of the status line and "Devam et" (fades only under reduced motion). */
function swapIn(reduce: boolean) {
  return {
    initial: { opacity: 0, transform: reduce ? "none" : "translateY(4px)" },
    animate: {
      opacity: 1,
      transform: reduce ? "none" : "translateY(0px)",
      transition: { duration: 0.24, ease: easeOutExpo, delay: 0.04 },
    },
    exit: { opacity: 0, transition: { duration: 0.12, ease: easeOutExpo } },
  };
}

/**
 * "Oturum: 12 dk" while the game runs. It renders only when the shown text changes: every second
 * in the first minute ("45 sn"), then once a minute.
 */
function SessionTime({ since }: { since: number }) {
  const { t } = useTranslation();
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const elapsed = now - since;
    const wait = elapsed < 60_000 ? 1000 - (elapsed % 1000) : 60_000 - (elapsed % 60_000);
    const timer = setTimeout(() => setNow(Date.now()), Math.max(50, wait));
    return () => clearTimeout(timer);
  }, [now, since]);
  return <>{t("play.session", { time: formatDuration((now - since) / 1000) })}</>;
}

/** "İlk açılış biraz sürebilir" after this long, on the first start with a version only. */
const FIRST_START_NOTE_MS = 20_000;

/**
 * Whether the first-start note is due: one timer for the time left, nothing ticks meanwhile (a
 * page that was away shows it at once when it comes back).
 */
function useFirstStartNote(phase: LaunchPhase): boolean {
  const since = phase.kind === "running" && !phase.shown && phase.firstStart ? phase.since : null;
  const [due, setDue] = useState<number | null>(null);
  useEffect(() => {
    if (since === null) return;
    const left = since + FIRST_START_NOTE_MS - Date.now();
    if (left <= 0) {
      setDue(since);
      return;
    }
    const timer = window.setTimeout(() => setDue(since), left);
    return () => window.clearTimeout(timer);
  }, [since]);
  return since !== null && due === since;
}

/**
 * While a launch prepares its files and while the game opens: the stage and its figures over the
 * bar, then "Oyun açılıyor". The bar steps back when the files are ready (a full bar that stays
 * reads as stuck); a first start that lasts says so where the bar was. The words stay in place.
 */
function LaunchStatus({ phase }: { phase: LaunchPhase }) {
  const { t } = useTranslation();
  const reduce = useReducedMotion() ?? false;
  const note = useFirstStartNote(phase);
  const working = phase.kind === "working" ? phase : null;
  const label = working ? t(`play.stages.${working.stage}`) : t("playOpen.status");
  const filesLabel = (done: number) =>
    t("play.files", { done: formatCount(done), total: formatCount(working?.totalFiles ?? 0) });
  return (
    <div className="min-w-0" aria-live="polite">
      <div className="flex items-baseline justify-between gap-4 text-[12px]">
        {/* Stages follow each other in place; the opening line cross-fades in. */}
        <span className="grid min-w-0 *:[grid-area:1/1]">
          <AnimatePresence initial={false}>
            <motion.span
              key={working ? "stage" : "opening"}
              className="truncate font-medium text-bone-100"
              {...swapIn(reduce)}
            >
              {label}
            </motion.span>
          </AnimatePresence>
        </span>
        {working ? (
          // Fixed, right-aligned slots: the figures change every tick and come and go between
          // stages, so the line never slides sideways.
          <span className="flex shrink-0 gap-3 text-ash-300 tabular-nums *:text-right">
            {/* As wide as the count gets in this stage ("402 / 402 dosya"). */}
            <span style={{ minWidth: `${filesLabel(working.totalFiles).length}ch` }}>
              {working.totalFiles > 0 ? filesLabel(working.doneFiles) : null}
            </span>
            <span className="min-w-[10.5ch]">
              {working.bytesPerSec > 0 ? formatSpeed(working.bytesPerSec) : null}
            </span>
            <span className="min-w-[11ch]">
              {working.etaSecs ? t("play.eta", { time: formatDuration(working.etaSecs) }) : null}
            </span>
          </span>
        ) : null}
      </div>
      {/* The bar's row keeps its height: the note hangs from it, so the line above stays put.
          The bar leaves once it faded (its sheen would keep the page drawing frames). */}
      <div className="relative mt-2 h-1.5">
        <AnimatePresence initial={false}>
          {working ? (
            <motion.div
              key="bar"
              className="absolute inset-0"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: { duration: 0.15, ease: easeOutExpo } }}
              exit={{ opacity: 0, transition: { duration: 0.32, ease: easeOutExpo } }}
            >
              <Progress value={working.percent} label={label} />
            </motion.div>
          ) : null}
        </AnimatePresence>
        <AnimatePresence initial={false}>
          {note ? (
            <motion.p
              key="first-start"
              className="absolute top-[-4px] left-0 text-[12px] leading-4 whitespace-nowrap text-ash-300"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: { duration: 0.4, ease: easeOutExpo } }}
              exit={{ opacity: 0, transition: { duration: 0.12 } }}
            >
              {t("playOpen.firstStart")}
            </motion.p>
          ) : null}
        </AnimatePresence>
      </div>
    </div>
  );
}

/**
 * Who else is in game (PLAY-AGAIN): with another account selected, the accounts in game while this
 * one is not, so OYNA reads as one more game beside theirs. As the account menu shows it: the
 * face, the name (two or more: "2 hesap") over a green "Oyunda", after "Devam et" behind the bar's
 * divider, away from the switch (second looks: beside the switch it read as the switch's label;
 * one line lost its subject at 1024 px). The sentence with every name is read out and shows under
 * the pointer. Quiet: no dot, no glow, no hover.
 */
function othersWords(
  t: TFunction,
  names: readonly string[],
): { sentence: string; who: string } | null {
  const [first, second] = names;
  if (!first) return null;
  if (!second) return { sentence: t("playAgain.othersOne", { name: first }), who: first };
  const total = String(names.length);
  return {
    sentence:
      names.length === 2
        ? t("playAgain.othersTwo", { first, second })
        : `${t("playAgain.othersMany", { total })}: ${names.join(", ")}`,
    who: t("playAgain.othersCount", { total }),
  };
}

/**
 * The bar's quiet "who is in game" (PLAY-AGAIN): their faces, the name (or the count) over
 * "Oyunda". The names come under the pointer in the app's tooltip: always while the bar counts
 * them ("2 hesap"), a single name only when it is cut (repeating the words on screen is noise). In
 * a narrow bar (1024 px) the face and a name stay and the name is cut: a bare "Oyunda" beside OYNA
 * read as the selected account in game (fourth look), a name without its face as a world (fifth
 * look); "Devam et" keeps its world whole while this line gives (third look: two cut names side by
 * side read as one thing twice).
 */
function OthersInGame({
  sentence,
  who,
  many,
  faces,
}: {
  sentence: string;
  who: string;
  many: boolean;
  faces: readonly (FaceAccount & { id: string })[];
}) {
  const { t } = useTranslation();
  const whoRef = useRef<HTMLSpanElement>(null);
  // Measured when the pointer comes, as OverflowTooltip does; one tooltip either way, so the tree
  // stays when the count changes (design.md §5).
  const [cut, setCut] = useState(false);
  const body = (
    <span
      aria-hidden
      className="flex min-w-0 items-center gap-2.5"
      onPointerEnter={() => setCut(isCut(whoRef.current))}
    >
      {faces.length > 0 ? (
        <span className="flex shrink-0 -space-x-2">
          {faces.map((account) => (
            <AccountFace
              key={account.id}
              account={account}
              size={24}
              className="ring-2 ring-obsidian-950/80"
            />
          ))}
        </span>
      ) : null}
      <span className="min-w-0 leading-tight">
        <span ref={whoRef} className="block truncate text-[13px] text-ash-300">
          {who}
        </span>
        {/* One word that never breaks (round 4: "Oyund / a" when the bar was full). */}
        <span className="block whitespace-nowrap text-[12px] text-jade-400">
          {t("playAcct.inGame")}
        </span>
      </span>
    </span>
  );
  return (
    <>
      <span className="sr-only">{sentence}</span>
      <Tooltip content={sentence} disabled={!many && !cut}>
        {body}
      </Tooltip>
    </>
  );
}

function StatusArea({
  selected,
  phase,
  others,
}: {
  selected: InstanceSummary | undefined;
  /** The launch as the selected account sees it (idle while another account's game runs). */
  phase: LaunchPhase;
  /** The other accounts in game while this one is not (PLAY-AGAIN). */
  others: readonly string[];
}) {
  const { t } = useTranslation();
  const reduce = useReducedMotion() ?? false;
  const autoJoin = usePrefs((s) => s.autoJoin);
  const setAutoJoin = usePrefs((s) => s.setAutoJoin);
  const othersText = othersWords(t, others);
  const { data: accounts } = useAccounts();
  const faces = others
    .map((name) => accounts?.find((account) => account.username === name))
    .filter((account) => account !== undefined)
    .slice(0, 3);
  // "Devam et" only while nothing runs (a click would start nothing otherwise).
  const target = useResumeTarget(selected);
  const resume = phase.kind === "idle" ? target : target === undefined ? undefined : null;

  // The launch's lines and the resting ones (last played, the session) cross-fade in one cell.
  const launch = phase.kind === "working" || isOpening(phase);
  return (
    <div className="grid min-w-0 flex-1 items-center *:[grid-area:1/1]">
      <AnimatePresence initial={false}>
        {launch ? (
          <motion.div key="launch" className="min-w-0" {...swapIn(reduce)}>
            <LaunchStatus phase={phase} />
          </motion.div>
        ) : (
          <motion.div
            key="rest"
            className="@container/bar flex min-w-0 items-center justify-between gap-6"
            {...swapIn(reduce)}
          >
            <div className="flex min-w-0 items-center gap-4">
              {/* "Devam et" and the last-played line share one cell and cross-fade in place. Beside
                  the quiet "who is in game" line it keeps its width (capped) and that line gives
                  instead: one cut name at most (third and fourth looks). */}
              <div
                className={cn(
                  "grid *:[grid-area:1/1]",
                  othersText
                    ? "max-w-[200px] shrink-0 @min-[600px]/bar:max-w-[300px]"
                    : "min-w-[120px] shrink",
                )}
              >
                <AnimatePresence initial={false}>
                  {selected && resume ? (
                    <motion.div key="resume" className="min-w-0" {...swapIn(reduce)}>
                      <ResumeButton instance={selected} target={resume} />
                    </motion.div>
                  ) : selected && resume === null ? (
                    <motion.div
                      key="status"
                      className="min-w-0 text-[12px] text-ash-300"
                      {...swapIn(reduce)}
                    >
                      <span className="block truncate tabular-nums">
                        {phase.kind === "running" ? (
                          <SessionTime since={phase.since} />
                        ) : selected.lastPlayedAt ? (
                          t("play.lastPlayed", { when: formatRelative(selected.lastPlayedAt) })
                        ) : // The session is recorded when the game closes: no "Henüz oynanmadı"
                        // while the first one starts.
                        phase.kind === "idle" ? (
                          t("play.neverPlayed")
                        ) : null}
                      </span>
                      {selected.playTimeSecs > 0 ? (
                        <span className="block truncate">
                          {t("play.playTime", { time: formatDuration(selected.playTimeSecs) })}
                        </span>
                      ) : null}
                    </motion.div>
                  ) : null}
                </AnimatePresence>
              </div>
              <AnimatePresence initial={false}>
                {othersText ? (
                  <motion.div
                    key="others"
                    className="flex max-w-[200px] min-w-0 shrink items-center border-l border-white/8 pl-4"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1, transition: { duration: 0.24, ease: easeOutExpo } }}
                    exit={{ opacity: 0, transition: { duration: 0.12, ease: easeOutExpo } }}
                  >
                    <OthersInGame
                      sentence={othersText.sentence}
                      who={othersText.who}
                      many={others.length > 1}
                      faces={faces}
                    />
                  </motion.div>
                ) : null}
              </AnimatePresence>
            </div>
            <div className="flex shrink-0 items-center">
              {selected?.autoJoin ? (
                <AutoJoinSwitch
                  checked={autoJoin}
                  onCheckedChange={setAutoJoin}
                  address={selected.autoJoin}
                  instance={selected}
                />
              ) : null}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

/**
 * "Doğrudan sunucuya bağlan" beside OYNA: a bone switch, so OYNA stays the bar's one gold, and its
 * words on one line (design.md §3; QA-TASTE). A narrow bar (1024 px) says it shorter and drops the
 * address (it is in the instance's settings), so "who is in game" keeps a whole name. While Studio
 * offers Projects' sub-servers (AUTOJOIN), the address line picks where the join goes after the
 * lobby's login ("play.projects.gg → SkyBlock"); the narrow bar keeps only "SkyBlock" ("Lobi").
 */
function AutoJoinSwitch({
  checked,
  onCheckedChange,
  address,
  instance,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  address: string;
  instance: InstanceSummary;
}) {
  const { t } = useTranslation();
  const id = useId();
  const { rows, current } = useRoute(instance);
  const words = (
    <>
      <span className="block text-[13px] font-medium text-bone-100 @max-[560px]/bar:hidden">
        {t("play.autoJoin")}
      </span>
      <span className="hidden text-[13px] font-medium text-bone-100 @max-[560px]/bar:block">
        {t("qaFix.bar.autoJoinShort")}
      </span>
    </>
  );
  return (
    <div className="flex shrink-0 items-center gap-3">
      <Switch
        id={id}
        checked={checked}
        onCheckedChange={onCheckedChange}
        label={t("play.autoJoin")}
        hideLabel
        tone="bone"
        describedBy={`${id}-address`}
      />
      {current ? (
        <div className="whitespace-nowrap select-none">
          <label htmlFor={id} className="block cursor-pointer">
            {words}
          </label>
          <RoutePicker
            id={`${id}-address`}
            instance={instance}
            rows={rows}
            current={current}
            on={checked}
          />
        </div>
      ) : (
        <label htmlFor={id} className="cursor-pointer whitespace-nowrap select-none">
          {words}
          <span
            id={`${id}-address`}
            className="block text-[12px] text-ash-400 @max-[560px]/bar:hidden"
          >
            {address}
          </span>
        </label>
      )}
    </div>
  );
}

/**
 * Bottom command bar: what to play, progress, and the OYNA button. The account is the title
 * bar's (SMALL10: it showed in three places, and only the title bar's chooses), and the bar is
 * that account's (PLAY-ACCT): while another account's game runs it shows this account's own
 * instance and OYNA, which asks before closing the other game.
 */
export function CommandBar() {
  const { t, i18n } = useTranslation();
  const { data: instances = [], isPending: instancesLoading } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
  });
  const selectedId = usePrefs((s) => s.selectedInstanceId);
  const selectInstance = usePrefs((s) => s.selectInstance);
  const navigate = useNavigate();
  // Only what the bar shows: the rest of the launch store (crash, notes, screenshots) changes
  // without it.
  const { phase, instanceId: launchId } = useOwnLaunch();
  // Other accounts in game while this one is not: the quiet "X oyunda" (PLAY-AGAIN).
  const others = useOthersInGame();
  const play = useLaunch((s) => s.play);
  const cancel = useLaunch((s) => s.cancel);
  const reset = useLaunch((s) => s.reset);
  const queryClient = useQueryClient();
  // While a game is prepared or played the bar shows that one: its progress, its session.
  const busy = phase.kind === "working" || phase.kind === "launching" || phase.kind === "running";
  // The picker and the Ctrl+K palette show every instance icon at once on their first opening.
  useEffect(() => prewarmInstanceIcons(), []);

  const selected = useMemo(
    () =>
      (busy ? instances.find((i) => i.id === launchId) : undefined) ??
      instances.find((i) => i.id === selectedId) ??
      instances[0],
    [instances, selectedId, busy, launchId],
  );
  // The instance OYNA plays is the one the launcher prepares in the background; its hint gives
  // way to the launch's own progress.
  usePrepareSync(selected, !instancesLoading);
  const prepareHint = usePrepare((s) => s.hint);
  const hint = !busy && prepareHint?.instanceId === selected?.id ? prepareHint : null;

  // A launch installs the instance and records when and how long it was played: once the game
  // starts and once the launch or the game ends, the bar, the library and "Devam et" read it
  // again instead of keeping "Henüz oynanmadı" until their next refresh, and the accounts their
  // "Son oynama" (the account in game says "az önce" beside its "Oyunda"). The launch itself, not
  // this account's view of it: another account's game ends too, and switching accounts is no end.
  const launchKind = useLaunch((s) => s.phase.kind);
  // One of several games ending changes no phase (PLAY-AGAIN): its end is read the same way.
  const gameCount = useLaunch((s) => s.games.length);
  const lastKind = useRef(launchKind);
  const lastCount = useRef(gameCount);
  useEffect(() => {
    const was = lastKind.current;
    const wasCount = lastCount.current;
    lastKind.current = launchKind;
    lastCount.current = gameCount;
    const ended = gameCount < wasCount;
    if (!ended) {
      if (was === launchKind || was === "idle") return;
      if (launchKind !== "running" && launchKind !== "idle") return;
    }
    for (const queryKey of [["instances"], ["instance"], ["resume"], ["accounts"]]) {
      void queryClient.invalidateQueries({ queryKey });
    }
  }, [launchKind, gameCount, queryClient]);

  const selectedLaunchId = selected?.id;
  useEffect(() => {
    if (phase.kind !== "error") return;
    if (phase.code === WORLD_OPEN) {
      // The world is open in the player's other game (PLAY-AGAIN): nothing to copy or to fix.
      toast(phase.message, { id: "launch-world-open" });
    } else if (launchErrorAction(phase.code) === "makeSpace") {
      // The disk cannot hold the download: nothing was fetched yet (or the half-done file went).
      // Settings → Veri shows what can go and how much room it makes; OYNA tries again after.
      toast.error(phase.message, {
        id: "launch-disk",
        duration: 10_000,
        action: {
          label: t("disk.makeSpace"),
          onClick: () => void navigate({ to: "/settings", search: { section: "data" } }),
        },
      });
    } else if (helpsWithLaunchError(phase.code)) {
      // "Sorunu bul" (HELP1): the troubleshooter checks at once, so its answers are there when the
      // player taps; its panel keeps this error's details for "Ayrıntıları kopyala".
      useHelp.getState().launchFailed(launchId ?? selectedLaunchId ?? null, {
        code: phase.code,
        message: phase.message,
        details: phase.details,
      });
      toast.error(phase.message, {
        id: "launch-help",
        duration: 10_000,
        action: { label: t("help.find"), onClick: openHelp },
      });
    } else {
      toast.error(phase.message, {
        action: {
          label: t("common.copyDetails"),
          onClick: () => void copyDetails(phase.details),
        },
      });
    }
    reset();
  }, [phase, reset, t, navigate, launchId, selectedLaunchId]);

  const state: PlayState = (() => {
    switch (phase.kind) {
      case "working":
        return { kind: "working", percent: phase.percent, label: t(`play.stages.${phase.stage}`) };
      // From the files being ready until the game's own window is on screen: "Açılıyor".
      case "launching":
        return { kind: "opening", label: t("playOpen.opening") };
      case "running":
        return phase.shown
          ? { kind: "running" }
          : { kind: "opening", label: t("playOpen.opening") };
      default:
        // "Güncelle" only for an installed game whose version or loader changed; a first
        // install is still "Oyna" (it downloads, then plays), as the welcome flow promises.
        return selected?.state === "needsUpdate" ? { kind: "update" } : { kind: "ready" };
    }
  })();

  return (
    <footer className="glass-strong relative z-20 flex h-commandbar shrink-0 items-center gap-4 border-x-0 border-b-0 pr-4 pl-3">
      <InstancePicker
        instances={instances}
        selected={selected}
        loading={instancesLoading}
        locked={busy}
        hint={hint}
        onSelect={selectInstance}
      />
      <div className={cn("flex min-w-0 flex-1 items-center border-l border-white/8 pl-4")}>
        <StatusArea selected={selected} phase={phase} others={others} />
      </div>
      <PlayButton
        state={state}
        // "Oyunda" takes a press: one more game, after the owner's question (PLAY-AGAIN).
        runningPressable={state.kind === "running"}
        onPlay={() => {
          if (selected) void play(selected.id);
          // Nothing to play: the offer "Hiç kurulum yok" with "Kur ve oyna" and "Yeni kurulum"
          // (LIB1). Never an error.
          else if (!instancesLoading) useOfficialOffer.getState().show({ kind: "play" });
        }}
        onCancel={() => void cancel()}
        // Ctrl+Enter presses it (app-shell.tsx), only when a press starts a game.
        keyShortcuts={ariaKeyShortcuts("play", detectOs())}
        labels={{
          play: t("play.play"),
          update: t("play.update"),
          cancel: t("play.cancel"),
          running: t("play.running"),
          runningAction: t("playAgain.runningAria"),
          runningHover: t("playAgain.runningHover"),
          percentFirst: i18n.language !== "en",
        }}
      />
    </footer>
  );
}
