// Per-account desktop AFK settings in the shared settings panel (SHEET-UX): five sections in a rail,
// inline validation, and the change bar docked under the section while something is unsaved. A
// press outside, Esc or the close button closes the panel at any time; the edits wait as a draft.
import {
  ArrowsClockwise,
  BellSimple,
  ChatCircleText,
  Copy,
  Globe,
  type Icon,
  Keyboard,
  PersonSimpleWalk,
} from "@phosphor-icons/react";
import { Button, toast } from "@pl/ui";
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { type ComponentType, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { copyErrorDetails } from "@/features/content/jobs";
import { useDebounced } from "@/features/content/use-content";
import { type AfkAccount, type AfkSettingsView, api } from "@/lib/api";
import { PlusChangeDock } from "../cloud/plus/change-dock";
import { useGuardedConnect } from "../cloud/use-cloud";
import { afkFailed, isFriendly, runAfk } from "../errors";
import { stoppedOnPassword } from "../primary-action";
import { SectionTitle, useIssueText } from "./controls";
import { draftKey, type HeldDraft, rebaseDraft, remindKept, useAfkDrafts } from "./drafts";
import { CommandsSection, RepliesSection } from "./list-sections";
import {
  type Draft,
  fromDraft,
  fromServerIssues,
  GROUP_OF,
  GROUPS,
  type GroupId,
  type Issue,
  isDirty,
  passwordIssue,
  patternIssue,
  toDraft,
  unsavedGroups,
  validate,
} from "./model";
import { PanelBody, PanelSkeleton, SettingsPanel } from "./panel";
import { ConnectionSection, GameSection, LogSection, type SectionProps } from "./sections";

const ICONS: Record<GroupId, Icon> = {
  connection: Globe,
  commands: Keyboard,
  game: PersonSimpleWalk,
  replies: ChatCircleText,
  log: BellSimple,
};

const BODIES: Record<GroupId, ComponentType<SectionProps>> = {
  connection: ConnectionSection,
  commands: CommandsSection,
  game: GameSection,
  replies: RepliesSection,
  log: LogSection,
};

/** A client runs with the saved settings of its start: new ones apply after a reconnect. */
const isActive = (account: AfkAccount) =>
  ["starting", "connecting", "connected"].includes(account.state.kind);

/** A failure shown in place: one sentence, the error itself kept for "Ayrıntıları kopyala". */
interface Failure {
  message: string;
  error: unknown;
}

const failure = (error: unknown, fallback: string): Failure => ({
  // Anything but a worded command error (IPC failure, programming error) is not for the player.
  message: isFriendly(error) ? error.message : fallback,
  error,
});

/** Reconnect (stop + connect) with the same failure handling everywhere. */
export function useAfkReconnect() {
  const { t } = useTranslation();
  return useMutation({
    mutationFn: (id: string) => api.afkReconnect(id),
    onError: (error) => afkFailed(error, t("afkSettings.reconnectFailed")),
  });
}

/** The client still runs with older saved settings (saved without reconnecting). */
function StaleBanner({ account }: { account: AfkAccount }) {
  const { t } = useTranslation();
  const reconnect = useAfkReconnect();
  if (!account.settingsChanged || !isActive(account)) return null;
  return (
    <div className="mb-4 flex items-center gap-3 rounded-[var(--radius-panel)] bg-white/[0.035] py-2.5 pr-2.5 pl-3.5 ring-1 ring-white/7">
      <ArrowsClockwise size={17} className="shrink-0 text-gold-300" aria-hidden />
      <p className="min-w-0 flex-1 text-[13px] text-bone-100">{t("afkSettings.stale")}</p>
      <Button
        size="sm"
        variant="secondary"
        icon={<ArrowsClockwise size={14} />}
        disabled={reconnect.isPending}
        onClick={() => reconnect.mutate(account.id)}
      >
        {t("afkSettings.reconnect")}
      </Button>
    </div>
  );
}

function CopyDetails({ error }: { error: unknown }) {
  const { t } = useTranslation();
  return (
    <Button
      size="sm"
      variant="ghost"
      icon={<Copy size={14} />}
      onClick={() => void copyErrorDetails(error)}
    >
      {t("common.copyDetails")}
    </Button>
  );
}

function LoadFailed({
  failed,
  retrying,
  onRetry,
}: {
  failed: Failure;
  retrying: boolean;
  onRetry: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 p-10 text-center">
      <p className="font-display text-lg font-semibold text-bone-50">
        {t("afkSettings.loadFailed")}
      </p>
      <p className="max-w-[46ch] text-[13px] text-ash-400">{failed.message}</p>
      <div className="mt-3 flex gap-2">
        <Button variant="secondary" onClick={onRetry} disabled={retrying}>
          {t("common.retry")}
        </Button>
        <CopyDetails error={failed.error} />
      </div>
    </div>
  );
}

/**
 * Auto-respond patterns the AFK client could not use, checked by the backend once typing pauses
 * (the same check runs again on save). A result is only shown while the rule still has the
 * pattern it was given for, so no stale error flickers while typing.
 */
function usePatternIssues(draft: Draft): Issue[] {
  const settled = useDebounced(draft.rules, 350);
  const checked = draft.page.autoRespond.enabled
    ? settled.filter((rule) => rule.pattern.trim())
    : [];
  const results = useQueries({
    queries: checked.map((rule) => ({
      queryKey: ["afk-pattern", rule.pattern.trim(), rule.isRegex],
      queryFn: () => api.afkCheckPattern(rule.pattern, rule.isRegex),
      staleTime: Number.POSITIVE_INFINITY,
    })),
  });
  const issues: Issue[] = [];
  checked.forEach((rule, index) => {
    const problem = results[index]?.data;
    const now = draft.rules.find((r) => r.key === rule.key);
    if (
      problem &&
      now &&
      now.pattern.trim() === rule.pattern.trim() &&
      now.isRegex === rule.isRegex
    )
      issues.push(patternIssue(rule.key, problem));
  });
  return issues;
}

/** "reconnect": a running account takes the settings now; "connect": one stopped on its password. */
type SaveMode = "save" | "reconnect" | "connect";

function Editor({
  account,
  view,
  onSaved,
}: {
  account: AfkAccount;
  view: AfkSettingsView;
  /** Saved: the stored settings are read again (the editor then starts from them). */
  onSaved: () => Promise<unknown>;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const issueText = useIssueText();
  const slot = draftKey("desktop", account.id);
  const hold = useAfkDrafts((s) => s.hold);
  const baseline = useMemo(() => toDraft(view.settings), [view.settings]);
  // Edits left when the panel last closed come back over what is stored now.
  const [draft, setDraftState] = useState<Draft>(() => {
    const held = useAfkDrafts.getState().held[slot] as HeldDraft<Draft> | undefined;
    return held ? rebaseDraft(held.draft, held.base, baseline) : baseline;
  });
  // Stopped on a password the server refused: the panel opens where the login line is, marked
  // (AFK-CLEAR, as the 7/24 Bulut panel); else where edits wait; else Bağlantı.
  const onPassword = stoppedOnPassword(account.state);
  const [section, setSection] = useState<GroupId>(() => {
    const line = onPassword ? passwordIssue(draft, baseline, view.sensitiveCommands) : null;
    if (line) return GROUP_OF[line.section];
    if (onPassword) return "commands";
    return GROUPS.find((group) => unsavedGroups(draft, baseline).has(group)) ?? "connection";
  });
  const [submitted, setSubmitted] = useState(false);
  const [serverIssues, setServerIssues] = useState<Issue[]>([]);
  const [saveError, setSaveError] = useState<Failure | null>(null);
  const [dock, setDock] = useState<HTMLElement | null>(null);
  const scroller = useRef<HTMLDivElement | null>(null);
  // A save is read back before the editor starts from it: meanwhile nothing is held and the bar's
  // buttons wait, then the edits give way to the stored settings and the bar slides away (the
  // editor stays: no section rises in again).
  const saved = useRef(false);
  const [settling, setSettling] = useState(false);
  const active = isActive(account);

  // Runs when a save comes back as the stored settings.
  useEffect(() => {
    if (!saved.current) return;
    saved.current = false;
    setDraftState(baseline);
    setSubmitted(false);
    setServerIssues([]);
    setSaveError(null);
  }, [baseline]);

  const dirty = isDirty(draft, baseline);
  useEffect(() => {
    if (!saved.current) hold(slot, dirty ? { draft, base: baseline } : null);
  }, [hold, slot, dirty, draft, baseline]);

  const setDraft = (update: (d: Draft) => Draft) => {
    setDraftState(update);
    setServerIssues([]);
    setSaveError(null);
  };

  const patternIssues = usePatternIssues(draft);
  const issues = useMemo(() => {
    const own = [...validate(draft).filter((i) => submitted || !i.structural), ...patternIssues];
    const fields = new Set(own.map((i) => i.field));
    return [...own, ...serverIssues.filter((i) => !fields.has(i.field))];
  }, [draft, submitted, serverIssues, patternIssues]);

  // Shown with the rest, never blocking (a save is how it gets fixed).
  const password = onPassword ? passwordIssue(draft, baseline, view.sensitiveCommands) : null;
  const shown = password ? [...issues, password] : issues;
  const guardedConnect = useGuardedConnect();
  const errorFor = (field: string) => issueText(shown.find((i) => i.field === field));
  const openFolder = () => runAfk(api.afkOpenFolder(account.id), t("afk.openFolderFailed"));
  const unsaved = unsavedGroups(draft, baseline);

  const goTo = (id: GroupId) => {
    setSection(id);
    scroller.current?.scrollTo({ top: 0 });
  };

  const save = useMutation({
    mutationFn: (_mode: SaveMode) => api.afkSaveSettings(account.id, fromDraft(draft)),
    onSuccess: (result, mode) => {
      if (result.status === "invalid") {
        const found = fromServerIssues(result.issues, draft);
        setServerIssues(found);
        if (found[0]) goTo(GROUP_OF[found[0].section]);
        return;
      }
      saved.current = true;
      setSettling(true);
      hold(slot, null);
      void queryClient.invalidateQueries({ queryKey: ["afk"] });
      const done = t("afkSettings.saved", { name: account.nickname });
      if (mode === "connect") {
        // The password typed again: this connect clears the stop (it asks first while the same
        // account runs in 7/24 Bulut).
        guardedConnect(account);
        toast.success(done, { description: t("afkClear.savedConnecting") });
      } else if (result.reconnectNeeded && mode === "reconnect") {
        runAfk(api.afkReconnect(account.id), t("afkSettings.reconnectFailed"));
        toast.success(done, { description: t("afkSettings.savedReconnecting") });
      } else if (result.reconnectNeeded) {
        toast.success(done, {
          description: t("afkSettings.savedReconnect"),
          duration: 10_000,
          action: {
            label: t("afkSettings.reconnect"),
            onClick: () => runAfk(api.afkReconnect(account.id), t("afkSettings.reconnectFailed")),
          },
        });
      } else {
        toast.success(done);
      }
      void onSaved().finally(() => {
        saved.current = false;
        setSettling(false);
      });
    },
    onError: (error) => setSaveError(failure(error, t("afkSettings.saveFailed"))),
  });

  const submit = (mode: SaveMode) => {
    setSubmitted(true);
    const found = [...validate(draft), ...patternIssues];
    if (found.length) {
      const here = found.some((i) => GROUP_OF[i.section] === section);
      if (!here && found[0]) goTo(GROUP_OF[found[0].section]);
      return;
    }
    save.mutate(mode);
  };

  const discard = () => {
    hold(slot, null);
    setDraftState(baseline);
    setSubmitted(false);
    setServerIssues([]);
    setSaveError(null);
  };

  // Ctrl+S saves (without the reconnect: the safe choice for a shortcut).
  const shortcut = useRef(() => {});
  shortcut.current = () => {
    if (dirty && !save.isPending && !settling) submit("save");
  };
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
      if (event.key.toLowerCase() !== "s") return;
      event.preventDefault();
      shortcut.current();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // A mark on screen holds the save (the refused password's never: saving is its fix).
  const blocking = issues.length > 0;
  const pending = save.isPending ? save.variables : null;
  // On a running account the main action applies the settings right away; on one stopped on its
  // password it connects again (saving alone left the stop in place).
  const mainMode: SaveMode = active ? "reconnect" : onPassword ? "connect" : "save";
  const Body = BODIES[section];

  const bar = (
    <PlusChangeDock
      dock={dock}
      open={dirty || Boolean(saveError) || blocking}
      label={t("afkPanel.barLabel")}
    >
      <div className="min-w-0 flex-1 text-[13px]">
        {saveError ? (
          <div className="flex items-center gap-2">
            <p role="alert" className="min-w-0 text-flare-400">
              {saveError.message}
            </p>
            <CopyDetails error={saveError.error} />
          </div>
        ) : blocking ? (
          <p role="alert" className="text-pretty text-flare-400">
            {t("afkSettings.fixErrors")}
          </p>
        ) : (
          <p className="text-pretty text-ash-300">{t("afkSettings.dirty")}</p>
        )}
      </div>
      <Button variant="ghost" onClick={discard} disabled={save.isPending || settling || !dirty}>
        {t("afkSettings.cancel")}
      </Button>
      {/* While the bar says to fix the marked fields first, saving waits (SHEET-UX). */}
      {mainMode !== "save" ? (
        <Button
          variant="secondary"
          onClick={() => submit("save")}
          disabled={!dirty || blocking || save.isPending || settling}
        >
          {pending === "save" ? t("afkSettings.saving") : t("afkPanel.saveOnly")}
        </Button>
      ) : null}
      <Button
        variant="primary"
        onClick={() => submit(mainMode)}
        disabled={!dirty || blocking || save.isPending || settling}
      >
        {pending === mainMode
          ? t("afkSettings.saving")
          : mainMode === "reconnect"
            ? t("afkSettings.saveReconnect")
            : mainMode === "connect"
              ? t("afkClear.saveConnect")
              : t("afkSettings.save")}
      </Button>
    </PlusChangeDock>
  );

  return (
    <PanelBody
      label={t("afkSettings.sectionsLabel")}
      section={section}
      onSection={goTo}
      scrollerRef={(element) => {
        scroller.current = element;
      }}
      dockRef={setDock}
      bar={bar}
      sections={GROUPS.map((id) => ({
        id,
        label: t(`afkPanel.sections.${id}`),
        icon: ICONS[id],
        unsaved: unsaved.has(id),
        problem: shown.some((i) => GROUP_OF[i.section] === id),
      }))}
    >
      <StaleBanner account={account} />
      <SectionTitle title={t(`afkPanel.sections.${section}`)} />
      <Body
        draft={draft}
        setDraft={setDraft}
        errorFor={errorFor}
        view={view}
        onOpenFolder={openFolder}
      />
    </PanelBody>
  );
}

/** The settings panel of one desktop AFK account. */
export function AfkSettingsSheet({
  account,
  twin = false,
  onClose,
}: {
  account: AfkAccount | undefined;
  /** The same account is a 7/24 Bulut account too: the title says these are this computer's. */
  twin?: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const open = Boolean(account);
  const close = () => {
    if (account) remindKept(t, draftKey("desktop", account.id), account.nickname);
    onClose();
  };
  const {
    data: view,
    error: loadError,
    refetch,
    isFetching,
  } = useQuery({
    queryKey: ["afk-settings", account?.id],
    queryFn: () => api.afkSettings(account?.id ?? ""),
    enabled: open,
    staleTime: 0,
    gcTime: 0,
    refetchOnWindowFocus: false,
  });

  return (
    <SettingsPanel
      open={open}
      onClose={close}
      nickname={account?.nickname ?? ""}
      title={
        twin
          ? t("afkClear.twin.hereTitle", { name: account?.nickname ?? "" })
          : t("afkSettings.title", { name: account?.nickname ?? "" })
      }
      server={account?.server ?? ""}
    >
      {account && view ? (
        <Editor key={account.id} account={account} view={view} onSaved={() => refetch()} />
      ) : loadError ? (
        <LoadFailed
          failed={failure(loadError, t("afkSettings.loadFailedHint"))}
          retrying={isFetching}
          onRetry={() => void refetch()}
        />
      ) : (
        <PanelSkeleton rows={GROUPS.length} />
      )}
    </SettingsPanel>
  );
}
