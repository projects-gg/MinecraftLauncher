// The cloud settings panel's sections (SHEET-UX): Bağlantı (name, address, version, console
// language), Komutlar (rows by trigger), Oyun içi (anti-AFK, the inventory view, movement) and
// Otomatik yanıt (the player's own name, rules). With the cloud features (only while the service
// offers them): the inventory view's switch, placeholders in replies with their chips and "Dene",
// and a line while old rules still match the old way.
import { ArrowDown, ArrowUp, Info, Plus, Trash, X } from "@phosphor-icons/react";
import {
  Button,
  cn,
  IconButton,
  Input,
  Segmented,
  Select,
  type SelectOption,
  Switch,
} from "@pl/ui";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { CloudFeatures } from "@/bindings";
import {
  AddressField,
  Block,
  cloudTokens,
  ErrorText,
  Fold,
  inputClass,
  RangeRow,
  SettingRow,
  ToggleRow,
  TriggerField,
  WideRow,
} from "../settings/controls";
import type { CloudEditData } from "./api";
import { pausedRows, refusalReportText, refusedCategory, refusedText } from "./moderation";
import { PausedLine, RefusalReport } from "./moderation-bits";
import { slotKey } from "./movement/model";
import { hasPlaceholder, insertToken } from "./placeholders";
import { ReplyTools } from "./reply-tools";
import { WordsField } from "./secret-field";
import {
  type Command,
  type Draft,
  type Issue,
  issueAt,
  issueKey,
  missingPart,
  newCommand,
  newRule,
  type Rule,
  replyProblem,
  SOURCES,
  TRIGGERS,
  type Trigger,
  typed,
  withTrigger,
} from "./settings-model";

export interface PartProps {
  draft: Draft;
  setDraft: (update: (draft: Draft) => Draft) => void;
  view: CloudEditData;
  issues: Issue[];
  /** Studio's switches of the cloud features (null: a service before them). */
  features: CloudFeatures | null;
}

/** The words of an issue, with Studio's numbers where it names them. */
export function useIssueText(view: CloudEditData, draft?: Draft) {
  const { t, i18n } = useTranslation();
  return (issue: Issue | undefined): string | null => {
    if (!issue) return null;
    // A stop for a wrong password, marked on the login line (SHEET-UX). A stored line (kept in the
    // cloud, or here too: both read-only, AFK-CLEAR) opens with "Değiştir" first.
    if (issue.problem === "wrongPassword") {
      const words =
        issue.index === null
          ? undefined
          : issue.place === "commands"
            ? draft?.config.commands[issue.index]?.words
            : issue.place === "rules"
              ? draft?.config.rules[issue.index]?.reply
              : undefined;
      const stored = words?.kind === "sealed" || (words?.kind === "secret" && words.kept);
      return t(stored ? "afkPanel.wrongPasswordSealed" : "afkPanel.wrongPassword");
    }
    // README "Saving": a content refusal names its category ("content:advertising").
    const refused = refusedCategory(issue.problem);
    if (refused !== null) return refusedText(refused, t, i18n.language);
    if (issue.problem.startsWith("placeholder_")) {
      const rule =
        issue.place === "rules" && issue.index !== null
          ? draft?.config.rules[issue.index]
          : undefined;
      return t(`cloudAfkFeat.settings.issues.${issue.problem}`, {
        n: rule ? (missingPart(rule) ?? "") : "",
      });
    }
    return t(`cloudAfk.settings.issues.${issueKey(issue)}`, {
      min:
        issue.field === "cooldown"
          ? (view.limits.minRuleCooldownSec ?? 10)
          : (view.limits.minIntervalSec ?? 60),
    });
  };
}

/**
 * Whole seconds, keeping what is typed (NaN while it is not a number, which the checks catch), with
 * the unit inside the field: the desktop settings' number field.
 */
function NumberField({
  label,
  value,
  onChange,
  unit,
  className,
  hideLabel,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  unit: string;
  className?: string;
  hideLabel?: boolean;
}) {
  const id = useId();
  const [text, setText] = useState(() => (Number.isFinite(value) ? String(value) : ""));
  // Follow outside changes (a trigger switch) without fighting the typing.
  useEffect(() => {
    setText((current) =>
      Object.is(current.trim() === "" ? Number.NaN : Number(current), value)
        ? current
        : Number.isFinite(value)
          ? String(value)
          : current,
    );
  }, [value]);
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <label
        htmlFor={id}
        className={cn("text-[13px] font-medium text-bone-100", hideLabel && "sr-only")}
      >
        {label}
      </label>
      <div className="relative">
        <input
          id={id}
          inputMode="numeric"
          autoComplete="off"
          spellCheck={false}
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            const typedText = e.target.value.trim();
            onChange(/^\d{1,6}$/.test(typedText) ? Number(typedText) : Number.NaN);
          }}
          // A longer unit ("sn sonra") keeps the digits clear of it.
          className={cn(
            inputClass,
            "h-[34px] px-3 tabular-nums",
            unit.length > 3 ? "pr-[68px]" : "pr-9",
          )}
        />
        <span className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-[12.5px] whitespace-nowrap text-ash-400">
          {unit}
        </span>
      </div>
    </div>
  );
}
const rowPanel = "rounded-[var(--radius-panel)] bg-white/[0.03] ring-1 ring-white/7";

function move<T>(list: T[], from: number, to: number): T[] {
  if (to < 0 || to >= list.length) return list;
  const next = [...list];
  const [item] = next.splice(from, 1);
  if (item !== undefined) next.splice(to, 0, item);
  return next;
}

// --- Oyun içi ---------------------------------------------------------------------------------

/**
 * "Hareket" (README "Movement"): only where the account may move (`features.movement.mode` on),
 * a block of its own led by its switch. Its one sentence says what turning it on means; while no
 * slot is free the switch waits and the sentence says why (the words stay readable: only the
 * switch dims). A save the service refused for it says so under it.
 */
function MovementBlock({
  draft,
  setDraft,
  view,
  issues,
}: Pick<PartProps, "draft" | "setDraft" | "view" | "issues">) {
  const { t } = useTranslation();
  const id = useId();
  const movement = view.account.features?.movement;
  if (movement?.mode !== "on") return null;
  const on = draft.config.movement ?? movement.on ?? false;
  // A slot is missing only for turning it on: the stored "on" never waits.
  const waiting = !movement.on && movement.slot ? slotKey(movement.slot) : null;
  const refused = issueAt(issues, "general", null, "movement");
  const refusal = refused
    ? refused.problem === "off"
      ? { key: "serviceOff" }
      : slotKey(refused.problem, refused.max)
    : null;
  const help =
    waiting && !on
      ? t(`cloudAfkMove.${waiting.key}`, waiting.values)
      : t("cloudAfkMove.setting.line");
  return (
    <Block>
      <div className="switch-row flex items-center justify-between gap-6 py-3.5">
        <div className="min-w-0">
          <label
            htmlFor={id}
            className={cn(
              "block text-[14.5px] font-semibold text-bone-50",
              waiting && !on ? "cursor-default" : "cursor-pointer",
            )}
          >
            {t("cloudAfkMove.setting.title")}
          </label>
          <p id={`${id}-help`} className="mt-0.5 max-w-[52ch] text-[12.5px] text-ash-400">
            {help}
          </p>
        </div>
        <Switch
          id={id}
          describedBy={`${id}-help`}
          checked={on}
          onCheckedChange={(enabled) =>
            setDraft((d) => ({ ...d, config: { ...d.config, movement: enabled } }))
          }
          label={t("cloudAfkMove.setting.title")}
          hideLabel
          disabled={Boolean(waiting) && !on}
        />
      </div>
      {refusal ? (
        <div className="pb-3">
          <ErrorText>{t(`cloudAfkMove.${refusal.key}`, refusal.values)}</ErrorText>
        </div>
      ) : null}
    </Block>
  );
}

/** Oyun içi: anti-AFK with its interval, the inventory view, movement (each where it is offered). */
export function GamePart({ draft, setDraft, view, issues }: PartProps) {
  const { t } = useTranslation();
  const issueText = useIssueText(view, draft);
  const inventory = view.account.features?.inventory;
  const inventoryShown = (inventory?.mode ?? "off") !== "off";
  const general = (field: string) => issueText(issueAt(issues, "general", null, field));
  const anti = draft.config.antiAfk;
  return (
    <>
      <Block>
        <ToggleRow
          lead
          title={t("cloudAfk.settings.antiAfk")}
          help={t("cloudAfk.settings.antiAfkHelp")}
          checked={anti.enabled}
          onCheckedChange={(enabled) =>
            setDraft((d) => ({
              ...d,
              config: { ...d.config, antiAfk: { ...d.config.antiAfk, enabled } },
            }))
          }
        />
        <Fold open={anti.enabled}>
          <RangeRow title={t("cloudAfk.settings.antiAfkInterval")} errors={[general("antiAfk")]}>
            <NumberField
              label={t("cloudAfk.settings.antiAfkMin")}
              hideLabel
              value={anti.delayMin}
              unit={t("cloudAfk.settings.seconds")}
              className="w-[104px]"
              onChange={(delayMin) =>
                setDraft((d) => ({
                  ...d,
                  config: { ...d.config, antiAfk: { ...d.config.antiAfk, delayMin } },
                }))
              }
            />
            <NumberField
              label={t("cloudAfk.settings.antiAfkMax")}
              hideLabel
              value={anti.delayMax}
              unit={t("cloudAfk.settings.seconds")}
              className="w-[104px]"
              onChange={(delayMax) =>
                setDraft((d) => ({
                  ...d,
                  config: { ...d.config, antiAfk: { ...d.config.antiAfk, delayMax } },
                }))
              }
            />
          </RangeRow>
        </Fold>
      </Block>
      {inventoryShown ? (
        <Block>
          <ToggleRow
            lead
            title={t("cloudAfkFeat.settings.inventory")}
            help={t("cloudAfkFeat.settings.inventoryHelp")}
            checked={draft.config.inventory ?? inventory?.on ?? false}
            onCheckedChange={(enabled) =>
              setDraft((d) => ({ ...d, config: { ...d.config, inventory: enabled } }))
            }
          />
        </Block>
      ) : null}
      <MovementBlock draft={draft} setDraft={setDraft} view={view} issues={issues} />
    </>
  );
}

// --- Bağlantı ---------------------------------------------------------------------------------

/** Bağlantı: the account's name, the server it joins, the game version, the console's language. */
export function ConnectionPart({ draft, setDraft, view, issues }: PartProps) {
  const { t } = useTranslation();
  const issueText = useIssueText(view, draft);
  const general = (field: string) => issueText(issueAt(issues, "general", null, field));
  const versions: SelectOption<string>[] = [
    { value: "auto", label: t("cloudAfk.settings.versionAuto") },
    ...view.versions.versions.map((v) => ({
      value: v.value,
      label: v.label,
      badge: v.value === view.versions.recommended ? t("cloudAfk.settings.recommended") : undefined,
    })),
  ];
  // A stored version the machines no longer list stays choosable as it is.
  if (draft.version !== "auto" && !versions.some((v) => v.value === draft.version)) {
    versions.push({ value: draft.version, label: draft.version });
  }
  const changes = view.account.serverChanges;
  return (
    <Block>
      <SettingRow
        lead
        title={t("cloudAfk.settings.name")}
        htmlFor="cloud-name"
        error={general("name")}
      >
        <Input
          id="cloud-name"
          value={draft.name}
          maxLength={16}
          spellCheck={false}
          autoComplete="off"
          invalid={Boolean(general("name"))}
          onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))}
          className="w-[240px]"
        />
      </SettingRow>
      <WideRow>
        <AddressField
          id="cloud-host"
          label={t("cloudAfk.settings.host")}
          host={draft.host}
          port={draft.port}
          error={general("server") ?? general("port")}
          help={
            changes.used > 0 ? (
              <span className="tabular-nums">
                {t("cloudAfk.settings.serverChanges", { used: changes.used, limit: changes.limit })}
              </span>
            ) : undefined
          }
          onChange={(host, port) => setDraft((d) => ({ ...d, host, port }))}
        />
      </WideRow>
      <SettingRow title={t("cloudAfk.settings.version")} error={general("version")}>
        <Select
          value={draft.version}
          onValueChange={(version) => setDraft((d) => ({ ...d, version }))}
          options={versions}
          label={t("cloudAfk.settings.version")}
          className="w-[240px]"
          searchable={
            versions.length > 12
              ? {
                  searchLabel: t("cloudAfk.settings.versionSearch"),
                  emptyText: t("cloudAfk.settings.versionNone"),
                }
              : undefined
          }
        />
      </SettingRow>
      <SettingRow title={t("cloudAfk.settings.lang")}>
        <Segmented
          value={draft.lang === "en" ? "en" : "tr"}
          onValueChange={(lang) => setDraft((d) => ({ ...d, lang }))}
          label={t("cloudAfk.settings.lang")}
          options={[
            { value: "tr", label: "Türkçe", lang: "tr" },
            { value: "en", label: "English", lang: "en" },
          ]}
        />
      </SettingRow>
    </Block>
  );
}

// --- Komutlar ---------------------------------------------------------------------------------

function Times({
  at,
  onChange,
  invalid,
}: {
  at: string[];
  onChange: (at: string[]) => void;
  invalid: boolean;
}) {
  const { t } = useTranslation();
  const [adding, setAdding] = useState("");
  const add = () => {
    const time = adding.trim();
    if (!time) return;
    onChange([...at, time].sort());
    setAdding("");
  };
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {at.map((time) => (
        <span
          key={time}
          className="flex h-7 items-center gap-1 rounded-md bg-white/[0.06] pr-0.5 pl-2 font-mono text-[12.5px] text-bone-100 tabular-nums"
        >
          {time}
          <IconButton
            label={t("cloudAfk.settings.commands.removeTime", { time })}
            icon={<X size={12} weight="bold" />}
            size="sm"
            className="size-6"
            onClick={() => onChange(at.filter((x) => x !== time))}
          />
        </span>
      ))}
      {at.length < 6 ? (
        <form
          className="flex items-center gap-1"
          onSubmit={(event) => {
            event.preventDefault();
            add();
          }}
        >
          <Input
            aria-label={t("cloudAfk.settings.commands.addTime")}
            value={adding}
            placeholder="08:00"
            maxLength={5}
            inputMode="numeric"
            invalid={invalid}
            onChange={(e) => setAdding(e.target.value)}
            className="h-7 w-[72px] px-2 font-mono text-[12.5px]"
          />
          <IconButton
            label={t("cloudAfk.settings.commands.addTime")}
            icon={<Plus size={14} weight="bold" />}
            size="sm"
            type="submit"
            disabled={!adding.trim()}
          />
        </form>
      ) : null}
    </div>
  );
}

function CommandRow({
  command,
  index,
  count,
  part,
  onChange,
  onMove,
  onRemove,
  focus,
}: {
  command: Command;
  index: number;
  count: number;
  part: PartProps;
  onChange: (command: Command) => void;
  onMove: (to: number) => void;
  onRemove: () => void;
  focus: boolean;
}) {
  const { t } = useTranslation();
  const issueText = useIssueText(part.view, part.draft);
  const at = (field: string) => issueText(issueAt(part.issues, "commands", index, field));
  // A moderation case paused it (the view's `paused`): "Durduruldu" until it is edited and saved.
  const paused = pausedRows(part.view.account).get(command.id) ?? null;
  // A save refused its text for content: "Yanlışsa bildir" under the sentence.
  const report = refusalReportText(
    issueAt(part.issues, "commands", index, "text"),
    command.words,
  );
  const minInterval = part.view.limits.minIntervalSec ?? 60;
  const triggers: SelectOption<Trigger>[] = TRIGGERS.map((value) => ({
    value,
    label: t(`cloudAfk.settings.commands.triggers.${value}`),
  }));
  const on = (TRIGGERS as readonly string[]).includes(command.on)
    ? (command.on as Trigger)
    : "join";
  const triggerError = at("delay") ?? at("every") ?? at("at");
  const ids = useId();
  return (
    <li className={cn(rowPanel, "flex flex-col px-3.5 pt-2 pb-3")}>
      {/* Which row it is, quietly, then its own tools and its switch, "Kullan" right beside it on
          the right edge as a reply rule's (SHEET-UX). */}
      <div className="flex items-center gap-3 py-1">
        <span className="min-w-0 flex-1 text-[12.5px] font-medium text-ash-400">
          {t("afkPanel.commandOrdinal", { n: index + 1 })}
        </span>
        <div className="flex items-center gap-0.5">
          <IconButton
            label={t("cloudAfk.settings.commands.moveUp")}
            icon={<ArrowUp size={15} />}
            size="sm"
            disabled={index === 0}
            onClick={() => onMove(index - 1)}
          />
          <IconButton
            label={t("cloudAfk.settings.commands.moveDown")}
            icon={<ArrowDown size={15} />}
            size="sm"
            disabled={index === count - 1}
            onClick={() => onMove(index + 1)}
          />
          <IconButton
            label={t("cloudAfk.settings.commands.remove")}
            icon={<Trash size={15} />}
            size="sm"
            onClick={onRemove}
          />
        </div>
        <span className="switch-row flex items-center gap-2 border-l border-white/8 pl-3">
          <label
            htmlFor={`${ids}-use`}
            className="cursor-pointer text-[13px] font-medium text-bone-100"
          >
            {t("afkPanel.use")}
          </label>
          <Switch
            id={`${ids}-use`}
            checked={command.enabled}
            onCheckedChange={(enabled) => onChange({ ...command, enabled })}
            label={t("cloudAfk.settings.commands.use")}
            hideLabel
          />
        </span>
      </div>
      <div
        className={cn(
          "flex flex-col gap-2.5 pt-2 transition-opacity duration-150",
          !command.enabled && "opacity-60",
        )}
      >
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[13px] text-ash-300">
            {t("cloudAfk.settings.commands.trigger")}
          </span>
          <Select
            value={on}
            onValueChange={(next) => onChange(withTrigger(command, next, minInterval))}
            options={triggers}
            label={t("cloudAfk.settings.commands.trigger")}
            className="w-[196px]"
          />
          {on === "interval" ? (
            <div className="flex items-center gap-1.5">
              <NumberField
                label={t("cloudAfk.settings.commands.everyMin")}
                hideLabel
                value={command.every?.min ?? minInterval}
                unit={t("cloudAfk.settings.seconds")}
                className="w-[110px]"
                onChange={(min) =>
                  onChange({ ...command, every: { min, max: command.every?.max ?? min } })
                }
              />
              {/* "300 sn ile 600 sn arası", as the desktop panel's waits (a bare dash did not say
                  it is a random wait between the two; AFK-CLEAR). */}
              <span aria-hidden className="text-[13px] text-ash-300">
                {t("afkPanel.rangeJoin")}
              </span>
              <NumberField
                label={t("cloudAfk.settings.commands.everyMax")}
                hideLabel
                value={command.every?.max ?? minInterval}
                unit={t("cloudAfk.settings.seconds")}
                className="w-[110px]"
                onChange={(max) =>
                  onChange({ ...command, every: { min: command.every?.min ?? max, max } })
                }
              />
              {t("afkPanel.rangeAfter") ? (
                <span aria-hidden className="text-[13px] text-ash-300">
                  {t("afkPanel.rangeAfter")}
                </span>
              ) : null}
            </div>
          ) : on === "daily" ? (
            <Times
              at={command.at ?? []}
              invalid={Boolean(at("at"))}
              onChange={(times) => onChange({ ...command, at: times })}
            />
          ) : (
            // "3 sn sonra", as the site says it: the wait before the line is sent.
            <NumberField
              label={t("cloudAfk.settings.commands.delay")}
              hideLabel
              value={command.delay ?? 0}
              unit={t("cloudAfk.settings.commands.later")}
              className="w-[124px]"
              onChange={(delay) => onChange({ ...command, delay })}
            />
          )}
        </div>
        {triggerError ? <ErrorText>{triggerError}</ErrorText> : null}
        {on === "daily" ? (
          <p className="-mt-1 text-[12px] text-ash-400">
            {t("cloudAfk.settings.commands.timesHelp")}
          </p>
        ) : null}
        <WordsField
          id={`${ids}-text`}
          showLabel
          words={command.words}
          onChange={(words) => onChange({ ...command, words })}
          accountId={part.view.account.id}
          rowId={command.id}
          label={t("cloudAfk.settings.commands.text")}
          placeholder={t("cloudAfk.settings.commands.textPlaceholder")}
          error={at("text")}
          autoFocus={focus}
        />
        {report ? <RefusalReport accountId={part.view.account.id} text={report} /> : null}
        {paused ? <PausedLine category={paused} account={part.view.account} /> : null}
      </div>
    </li>
  );
}

/** How many command rows the account may have (Studio's limit, at most 50). */
export const commandLimit = (view: CloudEditData) => Math.min(view.limits.maxCommandRows ?? 20, 50);

/** How many reply rules the account may have (Studio's limit, at most 50). */
export const ruleLimit = (view: CloudEditData) => Math.min(view.limits.maxRules ?? 10, 50);

/** Komutlar: the rows by trigger; the section's title counts them. */
export function CommandsPart(part: PartProps) {
  const { t } = useTranslation();
  const { draft, setDraft, view } = part;
  const [added, setAdded] = useState<string | null>(null);
  const commands = draft.config.commands;
  const max = commandLimit(view);
  const set = (next: Command[]) =>
    setDraft((d) => ({ ...d, config: { ...d.config, commands: next } }));
  return (
    <div className="flex flex-col gap-3 pt-2">
      {commands.length ? (
        <ul className="flex flex-col gap-2">
          {commands.map((command, index) => (
            <CommandRow
              key={command.id}
              command={command}
              index={index}
              count={commands.length}
              part={part}
              focus={command.id === added}
              onChange={(next) => set(commands.map((c) => (c.id === command.id ? next : c)))}
              onMove={(to) => set(move(commands, index, to))}
              onRemove={() => set(commands.filter((c) => c.id !== command.id))}
            />
          ))}
        </ul>
      ) : (
        <p className="py-3 text-[13px] text-ash-400">{t("cloudAfk.settings.commands.empty")}</p>
      )}
      <Button
        variant="secondary"
        size="sm"
        icon={<Plus size={14} weight="bold" />}
        className="self-start"
        disabled={commands.length >= max}
        onClick={() => {
          const row = newCommand(draft.config);
          setAdded(row.id);
          set([...commands, row]);
        }}
      >
        {t("cloudAfk.settings.commands.add")}
      </Button>
    </div>
  );
}

// --- Otomatik yanıt ---------------------------------------------------------------------------

function RuleRow({
  rule,
  index,
  part,
  onChange,
  onRemove,
  focus,
}: {
  rule: Rule;
  index: number;
  part: PartProps;
  onChange: (rule: Rule) => void;
  onRemove: () => void;
  focus: boolean;
}) {
  const { t } = useTranslation();
  const issueText = useIssueText(part.view, part.draft);
  const at = (field: string) => issueText(issueAt(part.issues, "rules", index, field));
  const paused = pausedRows(part.view.account).get(rule.id) ?? null;
  const ids = useId();
  const replyRef = useRef<HTMLInputElement>(null);
  const reply = rule.reply;
  const report = refusalReportText(issueAt(part.issues, "rules", index, "reply"), reply);
  // Placeholders: chips and "Dene" while this account may use them. With Studio's switch on, a
  // reply holding them is not sent while its machine cannot fill them or while it fails the save
  // checks (one stored before the switch), which the row says until the check itself shows.
  const placeholders = Boolean(part.view.account.features?.placeholders);
  const problem = replyProblem(rule, {
    on: Boolean(part.features?.placeholders),
    commands: part.features?.placeholderCommands ?? [],
  });
  const problemText = problem
    ? issueText({ place: "rules", index, field: "reply", problem: problem.reason })
    : null;
  const silenced =
    Boolean(part.features?.placeholders) &&
    reply.kind === "plain" &&
    hasPlaceholder(reply.text) &&
    (!placeholders || (problem !== null && !at("reply")));
  // The field shows placeholders by name: a chip goes in where its caret is, in those words.
  const tokens = useMemo(
    () =>
      cloudTokens(t("cloudAfkFeat.settings.chips.sender"), (n) =>
        t("cloudAfkFeat.settings.chips.part", { n }),
      ),
    [t],
  );
  const insert = (token: string) => {
    if (reply.kind !== "plain") return;
    const input = replyRef.current;
    const shown = tokens.toShown(reply.text);
    const start = input?.selectionStart ?? shown.length;
    const end = input?.selectionEnd ?? start;
    const next = insertToken(shown, start, end, tokens.toShown(token));
    onChange({ ...rule, reply: typed(reply, tokens.toStored(next.text)) });
    requestAnimationFrame(() => {
      input?.focus();
      input?.setSelectionRange(next.caret, next.caret);
    });
  };
  const sources: SelectOption<string>[] = SOURCES.map((value) => ({
    value,
    label: t(`cloudAfk.settings.replies.sources.${value}`),
  }));
  // "Yalnız bana yanıt versin" says whose name it listens to, or why it cannot yet (as the site).
  const owner = part.draft.config.owner.trim();
  const ownerHelp =
    rule.from === "server"
      ? t("cloudAfk.settings.issues.ownerServer")
      : owner
        ? t("afkPanel.rule.ownerOnlyHelp", { name: owner })
        : t("afkPanel.rule.ownerFirst");
  const ownerLocked = rule.from === "server" || (!owner && !rule.ownerOnly);
  return (
    <li className={cn(rowPanel, "flex flex-col px-3.5 pt-2")}>
      {/* The rule's own switch in words, on the card's right edge as every switch of the panel,
          with its delete button: three unnamed switches in a row read alike (SHEET-UX). */}
      <div className="flex items-center gap-3 py-1">
        <span className="min-w-0 flex-1 text-[12.5px] font-medium text-ash-400">
          {t("afkPanel.ruleOrdinal", { n: index + 1 })}
        </span>
        <IconButton
          label={t("cloudAfk.settings.replies.remove")}
          icon={<Trash size={15} />}
          size="sm"
          onClick={onRemove}
        />
        <span className="switch-row flex items-center gap-2 border-l border-white/8 pl-3">
          <label
            htmlFor={`${ids}-use`}
            className="cursor-pointer text-[13px] font-medium text-bone-100"
          >
            {t("afkPanel.use")}
          </label>
          <Switch
            id={`${ids}-use`}
            checked={rule.enabled}
            onCheckedChange={(enabled) => onChange({ ...rule, enabled })}
            label={t("cloudAfk.settings.replies.use")}
            hideLabel
          />
        </span>
      </div>
      {/* A rule that is off stays readable, a step quieter; its switch above stays as it is. */}
      <div
        className={cn(
          "flex flex-col transition-opacity duration-150",
          !rule.enabled && "opacity-60",
        )}
      >
        {/* "When a message says this" beside "reply with this". */}
        <div className="grid grid-cols-[minmax(0,2fr)_minmax(0,3fr)] items-start gap-x-4 pt-2">
          <TriggerField
            id={`${ids}-match`}
            value={rule.pattern}
            regex={rule.regex}
            onChange={(pattern, regex) => onChange({ ...rule, pattern, regex })}
            placeholder={t("cloudAfk.settings.replies.matchPlaceholder")}
            error={at("match")}
            maxLength={256}
            autoFocus={focus}
          />
          <div className="flex min-w-0 flex-col gap-2">
            <WordsField
              id={`${ids}-reply`}
              showLabel
              mono={false}
              tokens={placeholders ? tokens : undefined}
              words={rule.reply}
              onChange={(next) => onChange({ ...rule, reply: next })}
              accountId={part.view.account.id}
              rowId={rule.id}
              label={t("cloudAfk.settings.replies.reply")}
              placeholder={t("cloudAfk.settings.replies.replyPlaceholder")}
              error={at("reply")}
              inputRef={replyRef}
            />
            {report ? <RefusalReport accountId={part.view.account.id} text={report} /> : null}
            {placeholders && reply.kind === "plain" ? (
              <ReplyTools
                rule={{
                  pattern: rule.pattern,
                  regex: rule.regex,
                  from: rule.from,
                  reply: reply.text,
                }}
                sender={part.draft.config.owner || t("cloudAfkFeat.settings.sampleSender")}
                blocked={problemText}
                onInsert={insert}
              />
            ) : null}
            {silenced ? (
              <p className="flex items-center gap-1.5 text-[12.5px] text-ash-300">
                <Info size={14} aria-hidden className="shrink-0 text-ash-400" />
                {t("cloudAfkFeat.settings.silenced")}
              </p>
            ) : null}
            {paused ? <PausedLine category={paused} account={part.view.account} /> : null}
          </div>
        </div>
        <div className="mt-3 flex flex-wrap items-center gap-x-5 gap-y-2 border-t border-white/6 pt-3 pb-3">
          <div className="flex items-center gap-2">
            <span className="text-[13px] text-ash-300">{t("cloudAfk.settings.replies.from")}</span>
            <Select
              value={(SOURCES as readonly string[]).includes(rule.from) ? rule.from : "any"}
              // Server lines come from no player: "Yalnız bana" goes off with them (as the site).
              onValueChange={(from) =>
                onChange({ ...rule, from, ownerOnly: from === "server" ? false : rule.ownerOnly })
              }
              options={sources}
              label={t("cloudAfk.settings.replies.from")}
              className="w-[170px]"
            />
          </div>
          <div className="flex items-center gap-2">
            <span className="text-[13px] text-ash-300">{t("afkPanel.rule.cooldown")}</span>
            <NumberField
              label={t("afkPanel.rule.cooldown")}
              hideLabel
              value={rule.cooldown}
              unit={t("cloudAfk.settings.seconds")}
              className="w-[104px]"
              onChange={(cooldown) => onChange({ ...rule, cooldown })}
            />
          </div>
        </div>
        {at("cooldown") ? (
          <div className="-mt-1.5 pb-3">
            <ErrorText>{at("cooldown")}</ErrorText>
          </div>
        ) : null}
        <ToggleRow
          title={t("afkPanel.rule.ownerOnly")}
          help={ownerHelp}
          checked={rule.ownerOnly}
          disabled={ownerLocked}
          onCheckedChange={(ownerOnly) => onChange({ ...rule, ownerOnly })}
        />
        {at("ownerOnly") ? (
          <div className="-mt-1.5 pb-3">
            <ErrorText>{at("ownerOnly")}</ErrorText>
          </div>
        ) : null}
      </div>
    </li>
  );
}

/**
 * Otomatik yanıt: the player's own name ("Yalnız ana hesabımdan" rules know them by it), then the
 * rules; the section's title counts them.
 */
export function RepliesPart(part: PartProps) {
  const { t } = useTranslation();
  const { draft, setDraft, view, issues } = part;
  const issueText = useIssueText(view, draft);
  const owner = issueText(issueAt(issues, "general", null, "owner"));
  const [added, setAdded] = useState<string | null>(null);
  const rules = draft.config.rules;
  const max = ruleLimit(view);
  const set = (next: Rule[]) => setDraft((d) => ({ ...d, config: { ...d.config, rules: next } }));
  return (
    <>
      <Block>
        <SettingRow
          lead
          title={t("cloudAfk.settings.owner")}
          help={t("cloudAfk.settings.ownerHelp")}
          htmlFor="cloud-owner"
          error={owner}
        >
          <Input
            id="cloud-owner"
            value={draft.config.owner}
            maxLength={16}
            spellCheck={false}
            autoComplete="off"
            invalid={Boolean(owner)}
            onChange={(e) =>
              setDraft((d) => ({ ...d, config: { ...d.config, owner: e.target.value.trim() } }))
            }
            className="w-[240px]"
          />
        </SettingRow>
      </Block>
      <Block>
        <div className="flex flex-col gap-3 py-3">
          {view.account.features?.legacyRules ? (
            <p className="flex items-start gap-2 rounded-[var(--radius-panel)] bg-white/[0.035] px-3 py-2.5 text-[12.5px] text-bone-100 ring-1 ring-white/7">
              <Info size={15} aria-hidden className="mt-px shrink-0 text-gold-300" />
              <span>{t("cloudAfkFeat.settings.legacyRules")}</span>
            </p>
          ) : null}
          {rules.length ? (
            <ul className="flex flex-col gap-2">
              {rules.map((rule, index) => (
                <RuleRow
                  key={rule.id}
                  rule={rule}
                  index={index}
                  part={part}
                  focus={rule.id === added}
                  onChange={(next) => set(rules.map((r) => (r.id === rule.id ? next : r)))}
                  onRemove={() => set(rules.filter((r) => r.id !== rule.id))}
                />
              ))}
            </ul>
          ) : (
            <p className="py-3 text-[13px] text-ash-400">{t("cloudAfk.settings.replies.empty")}</p>
          )}
          <Button
            variant="secondary"
            size="sm"
            icon={<Plus size={14} weight="bold" />}
            className="self-start"
            disabled={rules.length >= max}
            onClick={() => {
              const row = newRule(draft.config, view.limits.minRuleCooldownSec ?? 10);
              setAdded(row.id);
              set([...rules, row]);
            }}
          >
            {t("cloudAfk.settings.replies.add")}
          </Button>
        </div>
      </Block>
    </>
  );
}
