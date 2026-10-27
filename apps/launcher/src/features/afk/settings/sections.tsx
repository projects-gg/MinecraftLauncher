// The desktop AFK settings panel's sections (SHEET-UX): Bağlantı (server, version, reconnecting),
// Oyun içi (anti-AFK, eating), Uyarılar ve günlük (alerts, chat log); Komutlar and Otomatik yanıt
// are the list sections in list-sections.tsx. Each feature is a block led by its switch, its fields
// folded away while it is off.
import { ArrowCounterClockwise } from "@phosphor-icons/react";
import { Button, Input, Segmented, Select, Slider } from "@pl/ui";
import { useTranslation } from "react-i18next";
import type { AfkSettingsView, ChatLogFilter, SettingsPage } from "@/lib/api";
import {
  AddressField,
  Block,
  Fold,
  FolderLink,
  Hint,
  LinesField,
  NumberField,
  RangeRow,
  SettingRow,
  ToggleRow,
  WideRow,
} from "./controls";
import { DEFAULT_RETRIES, type Draft, LIMITS } from "./model";

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

function ServerBlock({ draft, setDraft, errorFor, view }: SectionProps) {
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
    <Block>
      <WideRow lead>
        <AddressField
          id="afk-host"
          label={t("afkSettings.server.host")}
          host={p.serverHost}
          port={p.serverPort}
          placeholder="play.projects.gg"
          help={t("afkSettings.server.hostHelp")}
          error={errorFor("serverHost") ?? errorFor("serverPort")}
          onChange={(host, port) =>
            setDraft((d) => ({
              ...d,
              page: {
                ...d.page,
                serverHost: host,
                // Empty: the server's own port (0); more than five digits fails its range check.
                serverPort: port === "" ? 0 : Number(port),
              },
            }))
          }
        />
      </WideRow>
      <SettingRow title={t("afkSettings.server.version")} help={hint}>
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
        <Select
          value={p.minecraftVersion}
          onValueChange={(v) => page.set("minecraftVersion", v)}
          label={t("afkSettings.server.version")}
          className="w-[240px]"
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
      </SettingRow>
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
    </Block>
  );
}

function RelogBlock({ draft, setDraft, errorFor }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const r = draft.page.autoRelog;
  const sec = t("afkSettings.seconds");
  const unlimited = r.retries === -1;
  const noKickMessages = !r.kickMessages.some((m) => m.trim());
  return (
    <Block>
      <ToggleRow
        lead
        title={t("afkSettings.sections.relog")}
        help={t("afkSettings.intro.relog")}
        checked={r.enabled}
        onCheckedChange={(v) => page.patch("autoRelog", { enabled: v })}
      />
      <Fold open={r.enabled}>
        {/* Which drops bring it back: a choice of two, not a second switch that sounded like the
            first ("Her kopuşta yeniden bağlan" under "Otomatik yeniden bağlan"; SHEET-UX). */}
        <SettingRow title={t("afkSettings.relog.when")}>
          <Segmented<"any" | "listed">
            // A quiet plate: gold stays the save button's (the sixth look).
            className="[--segmented-ink:var(--color-bone-50)] [--segmented-plate:rgb(255_255_255/0.16)]"
            value={r.ignoreKickMessage ? "any" : "listed"}
            onValueChange={(v) => page.patch("autoRelog", { ignoreKickMessage: v === "any" })}
            label={t("afkSettings.relog.when")}
            options={[
              { value: "any", label: t("afkSettings.relog.whenAny") },
              { value: "listed", label: t("afkSettings.relog.whenListed") },
            ]}
          />
        </SettingRow>
        <Fold open={!r.ignoreKickMessage}>
          <WideRow>
            <LinesField
              label={t("afkSettings.relog.kickMessages")}
              help={t("afkSettings.relog.kickMessagesHelp")}
              value={r.kickMessages}
              onChange={(v) => page.patch("autoRelog", { kickMessages: v })}
              rows={5}
            />
            {noKickMessages ? (
              <Hint tone="warn" className="mt-2">
                {t("afkSettings.relog.kickEmpty")}
              </Hint>
            ) : null}
          </WideRow>
        </Fold>
        <RangeRow
          title={t("afkSettings.relog.delay")}
          errors={[errorFor("relog.min"), errorFor("relog.max")]}
        >
          <NumberField
            label={t("afkSettings.min")}
            value={r.delayMin}
            onChange={(v) => page.patch("autoRelog", { delayMin: v })}
            limit={LIMITS.relogDelay}
            unit={sec}
            invalid={Boolean(errorFor("relog.min"))}
          />
          <NumberField
            label={t("afkSettings.max")}
            value={r.delayMax}
            onChange={(v) => page.patch("autoRelog", { delayMax: v })}
            limit={LIMITS.relogDelay}
            unit={sec}
            invalid={Boolean(errorFor("relog.max"))}
          />
        </RangeRow>
        <ToggleRow
          title={t("afkSettings.relog.unlimited")}
          checked={unlimited}
          onCheckedChange={(v) => page.patch("autoRelog", { retries: v ? -1 : DEFAULT_RETRIES })}
        />
        <Fold open={!unlimited}>
          <SettingRow title={t("afkSettings.relog.retries")} error={errorFor("relog.retries")}>
            <NumberField
              label={t("afkSettings.relog.retries")}
              value={r.retries}
              onChange={(v) => page.patch("autoRelog", { retries: v })}
              limit={LIMITS.relogRetries}
              invalid={Boolean(errorFor("relog.retries"))}
            />
          </SettingRow>
        </Fold>
      </Fold>
    </Block>
  );
}

/** Bağlantı: where the account connects, and how it comes back after a drop. */
export function ConnectionSection(props: SectionProps) {
  return (
    <>
      <ServerBlock {...props} />
      <RelogBlock {...props} />
    </>
  );
}

function AntiAfkBlock({ draft, setDraft, errorFor }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const a = draft.page.antiAfk;
  const sec = t("afkSettings.seconds");
  return (
    <Block>
      <ToggleRow
        lead
        title={t("afkSettings.sections.antiAfk")}
        help={t("afkSettings.intro.antiAfk")}
        checked={a.enabled}
        onCheckedChange={(v) => page.patch("antiAfk", { enabled: v })}
      />
      <Fold open={a.enabled}>
        <RangeRow
          title={t("afkSettings.antiAfk.interval")}
          errors={[errorFor("antiAfk.min"), errorFor("antiAfk.max")]}
        >
          <NumberField
            label={t("afkSettings.min")}
            value={a.delayMin}
            onChange={(v) => page.patch("antiAfk", { delayMin: v })}
            limit={LIMITS.antiAfkDelay}
            unit={sec}
            invalid={Boolean(errorFor("antiAfk.min"))}
          />
          <NumberField
            label={t("afkSettings.max")}
            value={a.delayMax}
            onChange={(v) => page.patch("antiAfk", { delayMax: v })}
            limit={LIMITS.antiAfkDelay}
            unit={sec}
            invalid={Boolean(errorFor("antiAfk.max"))}
          />
        </RangeRow>
        <SettingRow
          title={t("afkSettings.antiAfk.command")}
          help={t("afkSettings.antiAfk.commandHelp")}
          htmlFor="afk-anti-command"
        >
          <Input
            id="afk-anti-command"
            value={a.command}
            placeholder={t("afkSettings.antiAfk.commandPlaceholder")}
            spellCheck={false}
            autoComplete="off"
            className="w-[240px] font-mono text-[13px]"
            onChange={(e) => page.patch("antiAfk", { command: e.target.value })}
          />
        </SettingRow>
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
        {/* The client's tries to find a spot to walk to stay as they are: a number with no
            meaning for a player ("Deneme sayısı 20"; SHEET-UX). */}
        <Fold open={a.useTerrainHandling}>
          <SettingRow
            title={t("afkSettings.antiAfk.walkRange")}
            error={errorFor("antiAfk.walkRange") ?? errorFor("antiAfk.walkRetries")}
          >
            <NumberField
              label={t("afkSettings.antiAfk.walkRange")}
              value={a.walkRange}
              onChange={(v) => page.patch("antiAfk", { walkRange: v })}
              limit={LIMITS.walkRange}
              unit={t("afkSettings.antiAfk.blocks")}
              invalid={Boolean(errorFor("antiAfk.walkRange"))}
              className="w-[116px]"
            />
          </SettingRow>
        </Fold>
      </Fold>
    </Block>
  );
}

function EatBlock({ draft, setDraft }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const e = draft.page.autoEat;
  return (
    <Block>
      <ToggleRow
        lead
        title={t("afkSettings.sections.eat")}
        help={t("afkSettings.intro.eat")}
        checked={e.enabled}
        onCheckedChange={(v) => page.patch("autoEat", { enabled: v })}
      />
      <Fold open={e.enabled}>
        <WideRow>
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
            className="max-w-[360px]"
          />
          <Hint className="mt-3">{t("afkSettings.eat.note")}</Hint>
        </WideRow>
      </Fold>
    </Block>
  );
}

/** Oyun içi: what the account does while it waits in the game. */
export function GameSection(props: SectionProps) {
  return (
    <>
      <AntiAfkBlock {...props} />
      <EatBlock {...props} />
    </>
  );
}

function AlertsBlock({ draft, setDraft, onOpenFolder }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const a = draft.page.alerts;
  const noWords = !a.matches.some((m) => m.trim());
  // Words are the client's only trigger here: one switch for both (a second "Kelimelerde
  // tetikle" switched the same thing on), the words first, then what an alert does (SHEET-UX).
  const on = a.enabled && a.triggerByWords;
  return (
    <Block>
      <ToggleRow
        lead
        title={t("afkSettings.sections.alerts")}
        help={t("afkSettings.intro.alerts")}
        checked={on}
        onCheckedChange={(v) =>
          page.patch("alerts", v ? { enabled: true, triggerByWords: true } : { enabled: false })
        }
      />
      <Fold open={on}>
        <WideRow>
          <div className="grid grid-cols-2 gap-4">
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
          {noWords ? (
            <Hint tone="warn" className="mt-2">
              {t("afkSettings.alerts.noWords")}
            </Hint>
          ) : null}
        </WideRow>
        <ToggleRow
          title={t("afkSettings.alerts.beep")}
          checked={a.beepEnabled}
          onCheckedChange={(v) => page.patch("alerts", { beepEnabled: v })}
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
      </Fold>
    </Block>
  );
}

function ChatLogBlock({ draft, setDraft, onOpenFolder }: SectionProps) {
  const { t } = useTranslation();
  const page = usePage(setDraft);
  const c = draft.page.chatLog;
  return (
    <Block>
      <ToggleRow
        lead
        title={t("afkSettings.sections.chatLog")}
        help={t("afkSettings.intro.chatLog")}
        checked={c.enabled}
        onCheckedChange={(v) => page.patch("chatLog", { enabled: v })}
      />
      <Fold open={c.enabled}>
        <ToggleRow
          title={t("afkSettings.chatLog.dateTime")}
          checked={c.addDateTime}
          onCheckedChange={(v) => page.patch("chatLog", { addDateTime: v })}
        />
        <SettingRow title={t("afkSettings.chatLog.filter")}>
          <Select<ChatLogFilter>
            value={c.filter}
            onValueChange={(v) => page.patch("chatLog", { filter: v })}
            label={t("afkSettings.chatLog.filter")}
            className="w-[240px]"
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
        </SettingRow>
        <WideRow>
          <Hint>
            {t("afkSettings.chatLog.file")} <FolderLink onOpen={onOpenFolder} />
          </Hint>
        </WideRow>
      </Fold>
      {/* The console's dates follow this even with the log off. */}
      <ToggleRow
        title={t("afkSettings.chatLog.showDate")}
        help={t("afkSettings.chatLog.showDateHelp")}
        checked={c.showDate}
        onCheckedChange={(v) => page.patch("chatLog", { showDate: v })}
      />
    </Block>
  );
}

/** Uyarılar ve günlük: when the account calls the player and what it records. */
export function LogSection(props: SectionProps) {
  return (
    <>
      <AlertsBlock {...props} />
      <ChatLogBlock {...props} />
    </>
  );
}
