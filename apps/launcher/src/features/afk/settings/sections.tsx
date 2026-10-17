// Form sections of the AFK settings sheet: server, AFK prevention, reconnect, eating, chat log,
// alerts. The list-based ones (login commands, scheduler, auto respond) are in list-sections.tsx.
import { ArrowCounterClockwise } from "@phosphor-icons/react";
import { Button, Field, Input, Select, Slider } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { AfkSettingsView, ChatLogFilter, SettingsPage } from "@/lib/api";
import {
  Dependent,
  FolderLink,
  Hint,
  LinesField,
  NumberField,
  RangeRow,
  SectionIntro,
  ToggleRow,
} from "./controls";
import { DEFAULT_RETRIES, type Draft, LIMITS, splitHostPort } from "./model";

export interface SectionProps {
  draft: Draft;
  setDraft: (update: (draft: Draft) => Draft) => void;
  /** Visible problem of a field, already translated. */
  errorFor: (field: string) => string | null;
  view: AfkSettingsView;
  /** Opens the account's folder (where the chat log and the alerts are written). */
  onOpenFolder: () => void;
}

type Nested = {
  [K in keyof SettingsPage]: SettingsPage[K] extends object ? K : never;
}[keyof SettingsPage];

/** Setters for the page's top-level fields and its nested option groups. */
export function usePage(setDraft: SectionProps["setDraft"]) {
  return {
    set<K extends keyof SettingsPage>(key: K, value: SettingsPage[K]) {
      setDraft((d) => ({ ...d, page: { ...d.page, [key]: value } }));
    },
    patch<K extends Nested>(key: K, patch: Partial<SettingsPage[K]>) {
      setDraft((d) => ({ ...d, page: { ...d.page, [key]: { ...d.page[key], ...patch } } }));
    },
  };
}

export function ServerSection({ draft, setDraft, errorFor, view }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const p = draft.page;
  const versions = view.versions;
  const recommended = versions.find((v) => v.recommended) ?? versions[0];
  const current = versions.find((v) => v.value === p.minecraftVersion);
  const isDefault = !recommended || p.minecraftVersion === recommended.value;
  // A saved version the list no longer knows shows as the recommended one, said once here. The
  // recommended version needs no words (its badge says it); the others say what can go wrong.
  const hint =
    isDefault && view.unknownVersion
      ? t("afkSettings.server.versionUnknown", { version: view.unknownVersion })
      : isDefault
        ? null
        : current?.auto
          ? t("afkSettings.server.versionHintAuto")
          : t("afkSettings.server.versionHintPinned");

  return (
    <>
      <SectionIntro title={t("afkSettings.sections.server")} />
      <div className="grid grid-cols-[minmax(0,1fr)_170px] gap-4 py-3">
        <Field
          label={t("afkSettings.server.host")}
          help={t("afkSettings.server.hostHelp")}
          error={errorFor("serverHost")}
          htmlFor="afk-host"
        >
          <Input
            id="afk-host"
            value={p.serverHost}
            placeholder="play.projects.gg"
            spellCheck={false}
            autoComplete="off"
            invalid={Boolean(errorFor("serverHost"))}
            onChange={(e) => page.set("serverHost", e.target.value)}
            onBlur={() => {
              // "play.example.net:25566" with no port set: move the port to its own field.
              const split = splitHostPort(p.serverHost);
              if (split && p.serverPort === 0)
                setDraft((d) => ({
                  ...d,
                  page: { ...d.page, serverHost: split[0], serverPort: split[1] },
                }));
            }}
          />
        </Field>
        <NumberField
          label={t("afkSettings.server.port")}
          value={p.serverPort}
          onChange={(v) => page.set("serverPort", v)}
          limit={LIMITS.port}
          help={t("afkSettings.server.portHelp")}
          error={errorFor("serverPort")}
        />
      </div>

      <div className="flex flex-col gap-1.5 py-3">
        <span className="text-[13px] font-medium text-bone-100">
          {t("afkSettings.server.version")}
        </span>
        <div className="flex items-center gap-2">
          <Select
            value={p.minecraftVersion}
            onValueChange={(v) => page.set("minecraftVersion", v)}
            label={t("afkSettings.server.version")}
            className="w-[320px]"
            searchable={{
              searchLabel: t("afkSettings.server.versionSearch"),
              emptyText: t("afkSettings.server.versionNone"),
            }}
            options={versions.map((v) => ({
              value: v.value,
              label: v.auto ? t("afkSettings.server.versionAuto") : v.label,
              badge: v.recommended ? t("afkSettings.server.recommended") : undefined,
            }))}
          />
          {!isDefault && recommended ? (
            <Button
              variant="ghost"
              size="sm"
              icon={<ArrowCounterClockwise size={15} />}
              onClick={() => page.set("minecraftVersion", recommended.value)}
            >
              {t("afkSettings.server.versionReset")}
            </Button>
          ) : null}
        </div>
        {hint ? (
          <Hint tone="warn" className="mt-1 max-w-[60ch]">
            {hint}
          </Hint>
        ) : null}
      </div>

      <ToggleRow
        title={t("afkSettings.server.autoStart")}
        checked={p.autoStart}
        onCheckedChange={(v) => page.set("autoStart", v)}
      />
      <ToggleRow
        title={t("afkSettings.server.resourcePacks")}
        help={t("afkSettings.server.resourcePacksHelp")}
        checked={p.allowResourcePacks}
        onCheckedChange={(v) => page.set("allowResourcePacks", v)}
      />
    </>
  );
}

export function AntiAfkSection({ draft, setDraft, errorFor }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const a = draft.page.antiAfk;
  const sec = t("afkSettings.seconds");
  return (
    <>
      <SectionIntro
        title={t("afkSettings.sections.antiAfk")}
        description={t("afkSettings.intro.antiAfk")}
      />
      <ToggleRow
        emphasis
        title={t("afkSettings.antiAfk.enabled")}
        checked={a.enabled}
        onCheckedChange={(v) => page.patch("antiAfk", { enabled: v })}
      />
      <Dependent enabled={a.enabled}>
        <RangeRow title={t("afkSettings.antiAfk.interval")}>
          <NumberField
            label={t("afkSettings.min")}
            value={a.delayMin}
            onChange={(v) => page.patch("antiAfk", { delayMin: v })}
            limit={LIMITS.antiAfkDelay}
            unit={sec}
            error={errorFor("antiAfk.min")}
          />
          <NumberField
            label={t("afkSettings.max")}
            value={a.delayMax}
            onChange={(v) => page.patch("antiAfk", { delayMax: v })}
            limit={LIMITS.antiAfkDelay}
            unit={sec}
            error={errorFor("antiAfk.max")}
          />
        </RangeRow>
        <Field
          label={t("afkSettings.antiAfk.command")}
          help={t("afkSettings.antiAfk.commandHelp")}
          htmlFor="afk-anti-command"
          className="py-3"
        >
          <Input
            id="afk-anti-command"
            value={a.command}
            placeholder={t("afkSettings.antiAfk.commandPlaceholder")}
            spellCheck={false}
            autoComplete="off"
            className="max-w-[420px] font-mono text-[13px]"
            onChange={(e) => page.patch("antiAfk", { command: e.target.value })}
          />
        </Field>
        <ToggleRow
          title={t("afkSettings.antiAfk.sneak")}
          checked={a.useSneak}
          onCheckedChange={(v) => page.patch("antiAfk", { useSneak: v })}
        />
        <ToggleRow
          title={t("afkSettings.antiAfk.walk")}
          help={t("afkSettings.antiAfk.walkHelp")}
          checked={a.useTerrainHandling}
          onCheckedChange={(v) => page.patch("antiAfk", { useTerrainHandling: v })}
        />
        <Dependent enabled={a.useTerrainHandling}>
          <div className="grid max-w-[420px] grid-cols-2 gap-3 pt-1 pb-3">
            <NumberField
              label={t("afkSettings.antiAfk.walkRange")}
              value={a.walkRange}
              onChange={(v) => page.patch("antiAfk", { walkRange: v })}
              limit={LIMITS.walkRange}
              unit={t("afkSettings.antiAfk.blocks")}
              error={errorFor("antiAfk.walkRange")}
            />
            <NumberField
              label={t("afkSettings.antiAfk.walkRetries")}
              value={a.walkRetries}
              onChange={(v) => page.patch("antiAfk", { walkRetries: v })}
              limit={LIMITS.walkRetries}
              error={errorFor("antiAfk.walkRetries")}
            />
          </div>
        </Dependent>
      </Dependent>
    </>
  );
}

export function RelogSection({ draft, setDraft, errorFor }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const r = draft.page.autoRelog;
  const sec = t("afkSettings.seconds");
  const unlimited = r.retries === -1;
  const noKickMessages = !r.kickMessages.some((m) => m.trim());
  return (
    <>
      <SectionIntro
        title={t("afkSettings.sections.relog")}
        description={t("afkSettings.intro.relog")}
      />
      <ToggleRow
        emphasis
        title={t("afkSettings.relog.enabled")}
        checked={r.enabled}
        onCheckedChange={(v) => page.patch("autoRelog", { enabled: v })}
      />
      <Dependent enabled={r.enabled}>
        <RangeRow title={t("afkSettings.relog.delay")}>
          <NumberField
            label={t("afkSettings.min")}
            value={r.delayMin}
            onChange={(v) => page.patch("autoRelog", { delayMin: v })}
            limit={LIMITS.relogDelay}
            unit={sec}
            error={errorFor("relog.min")}
          />
          <NumberField
            label={t("afkSettings.max")}
            value={r.delayMax}
            onChange={(v) => page.patch("autoRelog", { delayMax: v })}
            limit={LIMITS.relogDelay}
            unit={sec}
            error={errorFor("relog.max")}
          />
        </RangeRow>
        <ToggleRow
          title={t("afkSettings.relog.unlimited")}
          checked={unlimited}
          onCheckedChange={(v) => page.patch("autoRelog", { retries: v ? -1 : DEFAULT_RETRIES })}
        />
        {unlimited ? null : (
          <NumberField
            label={t("afkSettings.relog.retries")}
            value={r.retries}
            onChange={(v) => page.patch("autoRelog", { retries: v })}
            limit={LIMITS.relogRetries}
            error={errorFor("relog.retries")}
            className="max-w-[200px] pb-3"
          />
        )}
        <ToggleRow
          title={t("afkSettings.relog.ignoreKick")}
          help={t("afkSettings.relog.ignoreKickHelp")}
          checked={r.ignoreKickMessage}
          onCheckedChange={(v) => page.patch("autoRelog", { ignoreKickMessage: v })}
        />
        <Dependent enabled={!r.ignoreKickMessage}>
          <div className="flex flex-col gap-2 py-3">
            <LinesField
              label={t("afkSettings.relog.kickMessages")}
              help={t("afkSettings.relog.kickMessagesHelp")}
              value={r.kickMessages}
              onChange={(v) => page.patch("autoRelog", { kickMessages: v })}
              rows={5}
            />
            {!r.ignoreKickMessage && noKickMessages ? (
              <Hint tone="warn">{t("afkSettings.relog.kickEmpty")}</Hint>
            ) : null}
          </div>
        </Dependent>
      </Dependent>
    </>
  );
}

export function EatSection({ draft, setDraft }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const e = draft.page.autoEat;
  return (
    <>
      <SectionIntro
        title={t("afkSettings.sections.eat")}
        description={t("afkSettings.intro.eat")}
      />
      <ToggleRow
        emphasis
        title={t("afkSettings.eat.enabled")}
        checked={e.enabled}
        onCheckedChange={(v) => page.patch("autoEat", { enabled: v })}
      />
      <Dependent enabled={e.enabled}>
        {/* The slider is not a form control, so the disabled fieldset alone would not stop it. */}
        <div className="max-w-[440px] py-3" inert={!e.enabled}>
          <Slider
            label={t("afkSettings.eat.threshold")}
            value={e.threshold}
            onValueChange={(v) => page.patch("autoEat", { threshold: v })}
            min={LIMITS.threshold.min}
            max={LIMITS.threshold.max}
            step={1}
            format={(v) => t("afkSettings.eat.thresholdValue", { value: v })}
            marks={[
              { value: 0, label: "0" },
              { value: 10, label: "10" },
              { value: 20, label: "20" },
            ]}
          />
        </div>
        <Hint>{t("afkSettings.eat.note")}</Hint>
      </Dependent>
    </>
  );
}

export function ChatLogSection({ draft, setDraft, onOpenFolder }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const c = draft.page.chatLog;
  return (
    <>
      <SectionIntro
        title={t("afkSettings.sections.chatLog")}
        description={t("afkSettings.intro.chatLog")}
      />
      <ToggleRow
        emphasis
        title={t("afkSettings.chatLog.enabled")}
        checked={c.enabled}
        onCheckedChange={(v) => page.patch("chatLog", { enabled: v })}
      />
      <Dependent enabled={c.enabled}>
        <ToggleRow
          title={t("afkSettings.chatLog.dateTime")}
          checked={c.addDateTime}
          onCheckedChange={(v) => page.patch("chatLog", { addDateTime: v })}
        />
        <div className="flex flex-col gap-1.5 py-3">
          <span className="text-[13px] font-medium text-bone-100">
            {t("afkSettings.chatLog.filter")}
          </span>
          <Select<ChatLogFilter>
            value={c.filter}
            onValueChange={(v) => page.patch("chatLog", { filter: v })}
            label={t("afkSettings.chatLog.filter")}
            className="w-[280px]"
            disabled={!c.enabled}
            options={[
              {
                value: "all",
                label: t("afkSettings.chatLog.filterAll"),
                hint: t("afkSettings.chatLog.filterAllHint"),
              },
              {
                value: "messages",
                label: t("afkSettings.chatLog.filterMessages"),
                hint: t("afkSettings.chatLog.filterMessagesHint"),
              },
              {
                value: "chat",
                label: t("afkSettings.chatLog.filterChat"),
                hint: t("afkSettings.chatLog.filterChatHint"),
              },
            ]}
          />
        </div>
        <Hint>
          {t("afkSettings.chatLog.file")} <FolderLink onOpen={onOpenFolder} />
        </Hint>
      </Dependent>
      <div className="mt-4 border-t border-white/7 pt-1">
        <ToggleRow
          title={t("afkSettings.chatLog.showDate")}
          help={t("afkSettings.chatLog.showDateHelp")}
          checked={c.showDate}
          onCheckedChange={(v) => page.patch("chatLog", { showDate: v })}
        />
      </div>
    </>
  );
}

export function AlertsSection({ draft, setDraft, onOpenFolder }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const a = draft.page.alerts;
  const noWords = !a.matches.some((m) => m.trim());
  return (
    <>
      <SectionIntro
        title={t("afkSettings.sections.alerts")}
        description={t("afkSettings.intro.alerts")}
      />
      <ToggleRow
        emphasis
        title={t("afkSettings.alerts.enabled")}
        checked={a.enabled}
        onCheckedChange={(v) => page.patch("alerts", { enabled: v })}
      />
      <Dependent enabled={a.enabled}>
        <ToggleRow
          title={t("afkSettings.alerts.beep")}
          checked={a.beepEnabled}
          onCheckedChange={(v) => page.patch("alerts", { beepEnabled: v })}
        />
        <ToggleRow
          title={t("afkSettings.alerts.words")}
          help={t("afkSettings.alerts.wordsHelp")}
          checked={a.triggerByWords}
          onCheckedChange={(v) => page.patch("alerts", { triggerByWords: v })}
        />
        <ToggleRow
          title={t("afkSettings.alerts.file")}
          help={
            <>
              {t("afkSettings.alerts.fileHelp")} <FolderLink onOpen={onOpenFolder} />
            </>
          }
          checked={a.logToFile}
          onCheckedChange={(v) => page.patch("alerts", { logToFile: v })}
        />
        <div className="grid grid-cols-2 gap-4 py-3">
          <LinesField
            label={t("afkSettings.alerts.matches")}
            help={t("afkSettings.alerts.matchesHelp")}
            value={a.matches}
            onChange={(v) => page.patch("alerts", { matches: v })}
            rows={5}
          />
          <LinesField
            label={t("afkSettings.alerts.excludes")}
            help={t("afkSettings.alerts.excludesHelp")}
            value={a.excludes}
            onChange={(v) => page.patch("alerts", { excludes: v })}
            rows={5}
          />
        </div>
        {a.enabled && a.triggerByWords && noWords ? (
          <Hint tone="warn">{t("afkSettings.alerts.noWords")}</Hint>
        ) : null}
      </Dependent>
    </>
  );
}
