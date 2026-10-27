// The desktop AFK settings panel's list sections (SHEET-UX): Komutlar (login commands with stored
// passwords, scheduler tasks) and Otomatik yanıt (rules). Each feature is a block led by its switch.
import {
  ArrowDown,
  ArrowUp,
  Eye,
  EyeSlash,
  LockSimple,
  PencilSimple,
  Plus,
  Trash,
} from "@phosphor-icons/react";
import { Button, Chip, cn, Field, IconButton, Input, Switch } from "@pl/ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Block,
  ErrorText,
  Fold,
  Hint,
  NumberField,
  SettingRow,
  ToggleRow,
  TriggerField,
  WideRow,
} from "./controls";
import {
  isSecretLine,
  LIMITS,
  type LoginRow,
  MAX_PATTERN_CHARS,
  newLoginRow,
  newRule,
  newTask,
  type RuleRow,
  type TaskRow,
} from "./model";
import { type SectionProps, usePage } from "./sections";

function move<T>(list: T[], from: number, to: number): T[] {
  if (to < 0 || to >= list.length) return list;
  const next = [...list];
  const [item] = next.splice(from, 1);
  if (item !== undefined) next.splice(to, 0, item);
  return next;
}

const rowPanel = "rounded-[var(--radius-panel)] bg-white/[0.03] ring-1 ring-white/7";

// --- Login commands --------------------------------------------------------------------------

function TextLine({
  row,
  sensitive,
  onChange,
  label,
}: {
  row: Extract<LoginRow, { kind: "text" }>;
  sensitive: string[];
  onChange: (text: string) => void;
  label: string;
}) {
  const { t } = useTranslation();
  const [focused, setFocused] = useState(false);
  const secret = isSecretLine(row.text, sensitive);
  return (
    <div className="min-w-0 flex-1">
      <Input
        aria-label={label}
        value={row.text}
        // A password typed here is only readable while the field is being edited.
        type={secret && !focused ? "password" : "text"}
        placeholder={t("afkSettings.login.placeholder")}
        spellCheck={false}
        autoComplete="off"
        className="h-9 font-mono text-[13px]"
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onChange={(e) => onChange(e.target.value)}
      />
      {secret ? <Hint className="mt-1.5">{t("afkSettings.login.secretNote")}</Hint> : null}
    </div>
  );
}

function ReplaceLine({
  row,
  onChange,
  onUndo,
  error,
}: {
  row: Extract<LoginRow, { kind: "replace" }>;
  onChange: (args: string) => void;
  onUndo: () => void;
  error: string | null;
}) {
  const { t } = useTranslation();
  const [reveal, setReveal] = useState(false);
  return (
    <div className="min-w-0 flex-1">
      <div className="flex items-center gap-2">
        <span className="shrink-0 font-mono text-[13px] text-bone-100">{row.command}</span>
        <div className="relative min-w-0 flex-1">
          <Input
            aria-label={t("afkSettings.login.newSecret")}
            value={row.args}
            type={reveal ? "text" : "password"}
            placeholder={t("afkSettings.login.newSecret")}
            spellCheck={false}
            autoComplete="new-password"
            invalid={Boolean(error)}
            className="h-9 pr-10 font-mono text-[13px]"
            onChange={(e) => onChange(e.target.value)}
          />
          <IconButton
            label={reveal ? t("afkSettings.login.hide") : t("afkSettings.login.show")}
            icon={reveal ? <EyeSlash size={16} /> : <Eye size={16} />}
            size="sm"
            className="absolute top-1/2 right-0.5 size-8 -translate-y-1/2"
            onClick={() => setReveal((v) => !v)}
          />
        </div>
        <Button size="sm" variant="ghost" onClick={onUndo}>
          {t("afkSettings.login.undoReplace")}
        </Button>
      </div>
      {error ? (
        <div className="mt-1.5">
          <ErrorText>{error}</ErrorText>
        </div>
      ) : null}
    </div>
  );
}

function StoredLine({
  row,
  onReplace,
}: {
  row: Extract<LoginRow, { kind: "stored" }>;
  onReplace: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex min-w-0 flex-1 items-center gap-2.5">
      <div className="flex h-9 min-w-0 flex-1 items-center gap-2 rounded-md border border-white/7 bg-obsidian-850/80 px-3">
        <LockSimple size={15} weight="fill" className="shrink-0 text-gold-300" />
        <span className="truncate font-mono text-[13px] text-bone-100">{row.command}</span>
        <span aria-hidden className="font-mono text-[13px] tracking-widest text-ash-400">
          ••••••
        </span>
        <Chip tone="gold" className="ml-auto">
          {t("afkSettings.login.stored")}
        </Chip>
      </div>
      <Button size="sm" variant="secondary" icon={<PencilSimple size={14} />} onClick={onReplace}>
        {t("afkSettings.login.replace")}
      </Button>
    </div>
  );
}

function LoginBlock({ draft, setDraft, errorFor, view }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const login = draft.page.loginCommands;
  const rows = draft.login;
  const setRows = (update: (rows: LoginRow[]) => LoginRow[]) =>
    setDraft((d) => ({ ...d, login: update(d.login) }));
  const setRow = (key: string, row: LoginRow) =>
    setRows((list) => list.map((r) => (r.key === key ? row : r)));
  const hasStored = rows.some((r) => r.kind !== "text");

  return (
    <Block>
      <ToggleRow
        lead
        title={t("afkSettings.sections.login")}
        help={t("afkSettings.intro.login")}
        checked={login.enabled}
        onCheckedChange={(v) => page.patch("loginCommands", { enabled: v })}
      />
      <Fold open={login.enabled}>
        <WideRow>
          {view.keychainAvailable ? null : (
            <Hint tone="warn" className="mb-3">
              {t("afkSettings.login.keychainMissing")}
            </Hint>
          )}
          {rows.length ? (
            <ol className="flex flex-col gap-2">
              {rows.map((row, i) => (
                <li key={row.key} className="flex items-start gap-2">
                  <span className="mt-2 w-5 shrink-0 text-right text-[12px] text-ash-400 tabular-nums">
                    {i + 1}
                  </span>
                  {row.kind === "text" ? (
                    <TextLine
                      row={row}
                      sensitive={view.sensitiveCommands}
                      label={t("afkSettings.login.lineLabel", { n: i + 1 })}
                      onChange={(text) => setRow(row.key, { ...row, text })}
                    />
                  ) : row.kind === "stored" ? (
                    <StoredLine
                      row={row}
                      onReplace={() => setRow(row.key, { ...row, kind: "replace", args: "" })}
                    />
                  ) : (
                    <ReplaceLine
                      row={row}
                      error={errorFor(`login.${row.key}`)}
                      onChange={(args) => setRow(row.key, { ...row, args })}
                      onUndo={() =>
                        setRow(row.key, {
                          key: row.key,
                          kind: "stored",
                          index: row.index,
                          command: row.command,
                        })
                      }
                    />
                  )}
                  <div className="flex shrink-0 items-center">
                    <IconButton
                      label={t("afkSettings.moveUp")}
                      icon={<ArrowUp size={15} />}
                      size="sm"
                      disabled={i === 0}
                      onClick={() => setRows((list) => move(list, i, i - 1))}
                    />
                    <IconButton
                      label={t("afkSettings.moveDown")}
                      icon={<ArrowDown size={15} />}
                      size="sm"
                      disabled={i === rows.length - 1}
                      onClick={() => setRows((list) => move(list, i, i + 1))}
                    />
                    <IconButton
                      label={t("afkSettings.login.remove")}
                      icon={<Trash size={15} />}
                      size="sm"
                      className="hover:text-flare-400"
                      onClick={() => setRows((list) => list.filter((r) => r.key !== row.key))}
                    />
                  </div>
                </li>
              ))}
            </ol>
          ) : (
            <p className={cn(rowPanel, "px-4 py-3 text-[13px] text-ash-400")}>
              {t("afkSettings.login.empty")}
            </p>
          )}
          {errorFor("login.list") ? (
            <div className="mt-2">
              <ErrorText>{errorFor("login.list")}</ErrorText>
            </div>
          ) : null}
          <div className="mt-3 flex items-center justify-between gap-3">
            <Button
              size="sm"
              variant="secondary"
              icon={<Plus size={14} />}
              onClick={() => setRows((list) => [...list, newLoginRow()])}
            >
              {t("afkSettings.login.add")}
            </Button>
            {rows.length ? (
              <Button
                size="sm"
                variant="ghost"
                icon={<Trash size={14} />}
                onClick={() => setRows(() => [])}
              >
                {t("afkSettings.login.clearAll")}
              </Button>
            ) : null}
          </div>
          {hasStored ? (
            <Hint className="mt-3 max-w-[62ch]">{t("afkSettings.login.storedHint")}</Hint>
          ) : null}
        </WideRow>
        <ToggleRow
          title={t("afkSettings.login.onlyFirst")}
          help={t("afkSettings.login.onlyFirstHelp")}
          checked={login.onlyFirstLogin}
          onCheckedChange={(v) => page.patch("loginCommands", { onlyFirstLogin: v })}
        />
        <SettingRow
          title={t("afkSettings.login.delay")}
          help={t("afkSettings.login.delayHelp")}
          error={errorFor("login.delay")}
        >
          <NumberField
            label={t("afkSettings.login.delay")}
            value={login.delaySeconds}
            onChange={(v) => page.patch("loginCommands", { delaySeconds: v })}
            limit={LIMITS.loginDelay}
            unit={t("afkSettings.seconds")}
            invalid={Boolean(errorFor("login.delay"))}
          />
        </SettingRow>
      </Fold>
    </Block>
  );
}

// --- Scheduler -------------------------------------------------------------------------------

function TaskCard({
  task,
  index,
  onChange,
  onRemove,
  errorFor,
}: {
  task: TaskRow;
  index: number;
  onChange: (task: TaskRow) => void;
  onRemove: () => void;
  errorFor: SectionProps["errorFor"];
}) {
  const { t } = useTranslation();
  const set = (patch: Partial<TaskRow>) => onChange({ ...task, ...patch });
  const sec = t("afkSettings.seconds");
  const nameId = `task-${task.key}-name`;
  const actionId = `task-${task.key}-action`;
  const minError = errorFor(`task.${task.key}.min`);
  const maxError = errorFor(`task.${task.key}.max`);
  return (
    <li className={cn(rowPanel, "p-3.5")}>
      <div className="flex items-start gap-3">
        <Field label={t("afkSettings.scheduler.name")} htmlFor={nameId} className="w-[180px]">
          <Input
            id={nameId}
            value={task.name}
            placeholder={t("afkSettings.scheduler.namePlaceholder", { n: index + 1 })}
            className="h-9"
            onChange={(e) => set({ name: e.target.value })}
          />
        </Field>
        <Field
          label={t("afkSettings.scheduler.action")}
          htmlFor={actionId}
          error={errorFor(`task.${task.key}.action`)}
          className="min-w-0 flex-1"
        >
          <Input
            id={actionId}
            value={task.action}
            placeholder="send /kit"
            spellCheck={false}
            autoComplete="off"
            invalid={Boolean(errorFor(`task.${task.key}.action`))}
            className="h-9 font-mono text-[13px]"
            onChange={(e) => set({ action: e.target.value })}
          />
        </Field>
        <IconButton
          label={t("afkSettings.scheduler.remove")}
          icon={<Trash size={15} />}
          size="sm"
          className="mt-6 hover:text-flare-400"
          onClick={onRemove}
        />
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-x-6 gap-y-2">
        <Switch
          checked={task.onLogin}
          onCheckedChange={(v) => set({ onLogin: v })}
          label={t("afkSettings.scheduler.onLogin")}
        />
        <Switch
          checked={task.onFirstLogin}
          onCheckedChange={(v) => set({ onFirstLogin: v })}
          label={t("afkSettings.scheduler.onFirstLogin")}
        />
        <Switch
          checked={task.intervalEnabled}
          onCheckedChange={(v) => set({ intervalEnabled: v })}
          label={t("afkSettings.scheduler.interval")}
        />
      </div>
      {task.intervalEnabled ? (
        <div className="mt-3">
          <div className="flex items-center gap-2">
            <span className="text-[13px] text-ash-300">{t("afkPanel.interval")}</span>
            <NumberField
              label={t("afkSettings.min")}
              value={task.minSeconds}
              onChange={(v) => set({ minSeconds: v })}
              limit={LIMITS.taskSeconds}
              unit={sec}
              invalid={Boolean(minError)}
            />
            <span aria-hidden className="text-ash-400">
              –
            </span>
            <NumberField
              label={t("afkSettings.max")}
              value={task.maxSeconds}
              onChange={(v) => set({ maxSeconds: v })}
              limit={LIMITS.taskSeconds}
              unit={sec}
              invalid={Boolean(maxError)}
            />
          </div>
          {minError || maxError ? (
            <div className="mt-1.5">
              <ErrorText>{minError ?? maxError}</ErrorText>
            </div>
          ) : null}
        </div>
      ) : null}
    </li>
  );
}

function SchedulerBlock({ draft, setDraft, errorFor }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const enabled = draft.page.scheduler.enabled;
  const setTasks = (update: (tasks: TaskRow[]) => TaskRow[]) =>
    setDraft((d) => ({ ...d, tasks: update(d.tasks) }));
  return (
    <Block>
      <ToggleRow
        lead
        title={t("afkSettings.sections.scheduler")}
        help={t("afkSettings.intro.scheduler")}
        checked={enabled}
        onCheckedChange={(v) => page.patch("scheduler", { enabled: v })}
      />
      <Fold open={enabled}>
        <WideRow>
          {draft.tasks.length ? (
            <ul className="flex flex-col gap-3">
              {draft.tasks.map((task, i) => (
                <TaskCard
                  key={task.key}
                  task={task}
                  index={i}
                  errorFor={errorFor}
                  onChange={(next) =>
                    setTasks((list) => list.map((x) => (x.key === task.key ? next : x)))
                  }
                  onRemove={() => setTasks((list) => list.filter((x) => x.key !== task.key))}
                />
              ))}
            </ul>
          ) : (
            <p className={cn(rowPanel, "px-4 py-3 text-[13px] text-ash-400")}>
              {t("afkSettings.scheduler.empty")}
            </p>
          )}
          {errorFor("scheduler.list") ? (
            <div className="mt-2">
              <ErrorText>{errorFor("scheduler.list")}</ErrorText>
            </div>
          ) : null}
          <Button
            size="sm"
            variant="secondary"
            icon={<Plus size={14} />}
            className="mt-3"
            onClick={() => setTasks((list) => [...list, newTask()])}
          >
            {t("afkSettings.scheduler.add")}
          </Button>
          <Hint className="mt-3 max-w-[62ch]">{t("afkSettings.scheduler.help")}</Hint>
        </WideRow>
      </Fold>
    </Block>
  );
}

/** Komutlar: what the account types as it joins, and on its own schedule. */
export function CommandsSection(props: SectionProps) {
  return (
    <>
      <LoginBlock {...props} />
      <SchedulerBlock {...props} />
    </>
  );
}

// --- Auto respond ----------------------------------------------------------------------------

function RuleCard({
  rule,
  onChange,
  onRemove,
  errorFor,
}: {
  rule: RuleRow;
  onChange: (rule: RuleRow) => void;
  onRemove: () => void;
  errorFor: SectionProps["errorFor"];
}) {
  const { t } = useTranslation();
  const set = (patch: Partial<RuleRow>) => onChange({ ...rule, ...patch });
  const id = (field: string) => `rule-${rule.key}-${field}`;
  const patternError = errorFor(`rule.${rule.key}.pattern`);
  const replyError = errorFor(`rule.${rule.key}.reply`);
  const cooldownError = errorFor(`rule.${rule.key}.cooldown`);
  return (
    <li className={cn(rowPanel, "px-3.5 pt-3.5 pb-0.5")}>
      <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)_auto] items-start gap-3">
        {/* A plain phrase; a pattern is the advanced way under it (SHEET-UX). */}
        <TriggerField
          id={id("pattern")}
          value={rule.pattern}
          regex={rule.isRegex}
          onChange={(pattern, isRegex) => set({ pattern, isRegex })}
          placeholder={t("afkSettings.respond.patternPlaceholder")}
          error={patternError}
          maxLength={MAX_PATTERN_CHARS}
        />
        {/* What "$u" does is said once, under the list. */}
        <Field label={t("afkSettings.respond.reply")} htmlFor={id("reply")} error={replyError}>
          <Input
            id={id("reply")}
            value={rule.reply}
            placeholder={t("afkSettings.respond.replyPlaceholder")}
            spellCheck={false}
            invalid={Boolean(replyError)}
            className="h-9"
            onChange={(e) => set({ reply: e.target.value })}
          />
        </Field>
        <IconButton
          label={t("afkSettings.respond.remove")}
          icon={<Trash size={15} />}
          size="sm"
          className="mt-6 hover:text-flare-400"
          onClick={onRemove}
        />
      </div>
      <div className="mt-3 grid grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)_auto] gap-3">
        <Field
          label={t("afkSettings.respond.privateReply")}
          htmlFor={id("private")}
          help={t("afkSettings.respond.privateReplyHelp")}
        >
          <Input
            id={id("private")}
            value={rule.privateReply}
            spellCheck={false}
            className="h-9"
            onChange={(e) => set({ privateReply: e.target.value })}
          />
        </Field>
        <Field
          label={t("afkSettings.respond.otherReply")}
          htmlFor={id("other")}
          help={t("afkSettings.respond.otherReplyHelp")}
        >
          <Input
            id={id("other")}
            value={rule.otherReply}
            spellCheck={false}
            className="h-9"
            onChange={(e) => set({ otherReply: e.target.value })}
          />
        </Field>
        <span className="w-8" />
      </div>
      <div className="mt-3 flex items-center gap-2 pb-3">
        <span className="text-[13px] text-ash-300">{t("afkPanel.rule.cooldown")}</span>
        <NumberField
          label={t("afkPanel.rule.cooldown")}
          value={rule.cooldownSeconds}
          onChange={(v) => set({ cooldownSeconds: v })}
          limit={LIMITS.cooldown}
          unit={t("afkSettings.seconds")}
          invalid={Boolean(cooldownError)}
        />
      </div>
      {cooldownError ? (
        <div className="-mt-1.5 pb-3">
          <ErrorText>{cooldownError}</ErrorText>
        </div>
      ) : null}
      {/* On the card's right edge, as every switch of the panel. */}
      <ToggleRow
        title={t("afkPanel.rule.ownerOnly")}
        help={t("afkPanel.rule.ownerDesktop")}
        checked={rule.ownersOnly}
        onCheckedChange={(v) => set({ ownersOnly: v })}
      />
    </li>
  );
}

/** Otomatik yanıt: one block, so its switch says "use" rather than repeat the section's name. */
export function RepliesSection({ draft, setDraft, errorFor }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const r = draft.page.autoRespond;
  const setRules = (update: (rules: RuleRow[]) => RuleRow[]) =>
    setDraft((d) => ({ ...d, rules: update(d.rules) }));
  return (
    <Block>
      <ToggleRow
        lead
        title={t("afkSettings.respond.enabled")}
        help={t("afkSettings.intro.respond")}
        checked={r.enabled}
        onCheckedChange={(v) => page.patch("autoRespond", { enabled: v })}
      />
      <Fold open={r.enabled}>
        <ToggleRow
          title={t("afkSettings.respond.sound")}
          checked={r.soundEnabled}
          onCheckedChange={(v) => page.patch("autoRespond", { soundEnabled: v })}
        />
        <ToggleRow
          title={t("afkSettings.respond.colors")}
          help={t("afkSettings.respond.colorsHelp")}
          checked={r.matchColors}
          onCheckedChange={(v) => page.patch("autoRespond", { matchColors: v })}
        />
        <WideRow>
          <p className="mb-2 text-[13.5px] font-medium text-bone-100">
            {t("afkSettings.respond.rules")}
          </p>
          {draft.rules.length ? (
            <ul className="flex flex-col gap-3">
              {draft.rules.map((rule) => (
                <RuleCard
                  key={rule.key}
                  rule={rule}
                  errorFor={errorFor}
                  onChange={(next) =>
                    setRules((list) => list.map((x) => (x.key === rule.key ? next : x)))
                  }
                  onRemove={() => setRules((list) => list.filter((x) => x.key !== rule.key))}
                />
              ))}
            </ul>
          ) : (
            <p className={cn(rowPanel, "px-4 py-3 text-[13px] text-ash-400")}>
              {t("afkSettings.respond.empty")}
            </p>
          )}
          {errorFor("respond.list") ? (
            <div className="mt-2">
              <ErrorText>{errorFor("respond.list")}</ErrorText>
            </div>
          ) : null}
          <Button
            size="sm"
            variant="secondary"
            icon={<Plus size={14} />}
            className="mt-3"
            onClick={() => setRules((list) => [...list, newRule()])}
          >
            {t("afkSettings.respond.add")}
          </Button>
          <Hint className="mt-3 max-w-[62ch]">{t("afkSettings.respond.help")}</Hint>
        </WideRow>
      </Fold>
    </Block>
  );
}
