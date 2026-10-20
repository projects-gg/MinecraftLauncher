// Per-account AFK settings in a side sheet: a section rail, one section at a time, inline
// validation, and a footer that knows about unsaved changes and running sessions.
import {
  ArrowsClockwise,
  BellSimple,
  ChatCircleText,
  Clock,
  Copy,
  ForkKnife,
  Globe,
  Key,
  Notebook,
  PersonSimpleWalk,
  WarningCircle,
  X,
} from "@phosphor-icons/react";
import {
  Button,
  Dialog as ConfirmDialog,
  cn,
  EntranceGate,
  focusPanel,
  focusTabQuietly,
  GatedMotion,
  IconButton,
  Skeleton,
  scrimMotion,
  sheetMotion,
  toast,
  useInteractionMotion,
} from "@pl/ui";
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence } from "motion/react";
import { Dialog, Tabs } from "radix-ui";
import { type ComponentType, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { copyErrorDetails } from "@/features/content/jobs";
import { useDebounced } from "@/features/content/use-content";
import { type AfkAccount, type AfkSettingsView, api } from "@/lib/api";
import { AfkHead } from "../afk-head";
import { afkFailed, isFriendly, runAfk } from "../errors";
import { useIssueText } from "./controls";
import { LoginSection, RespondSection, SchedulerSection } from "./list-sections";
import {
  type Draft,
  fromDraft,
  fromServerIssues,
  type Issue,
  isDirty,
  patternIssue,
  SECTIONS,
  type SectionId,
  sectionEnabled,
  toDraft,
  validate,
} from "./model";
import {
  AlertsSection,
  AntiAfkSection,
  ChatLogSection,
  EatSection,
  RelogSection,
  type SectionProps,
  ServerSection,
} from "./sections";

const ICONS: Record<
  SectionId,
  ComponentType<{ size?: number; weight?: "regular" | "duotone"; className?: string }>
> = {
  server: Globe,
  login: Key,
  scheduler: Clock,
  antiAfk: PersonSimpleWalk,
  relog: ArrowsClockwise,
  eat: ForkKnife,
  respond: ChatCircleText,
  chatLog: Notebook,
  alerts: BellSimple,
};

const BODIES: Record<SectionId, ComponentType<SectionProps>> = {
  server: ServerSection,
  login: LoginSection,
  scheduler: SchedulerSection,
  antiAfk: AntiAfkSection,
  relog: RelogSection,
  eat: EatSection,
  respond: RespondSection,
  chatLog: ChatLogSection,
  alerts: AlertsSection,
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
    <div className="mb-5 flex items-center gap-3 rounded-[var(--radius-panel)] bg-gold-500/10 py-2.5 pr-2.5 pl-4 ring-1 ring-gold-400/25">
      <ArrowsClockwise size={18} className="shrink-0 text-gold-300" aria-hidden />
      <p className="min-w-0 flex-1 text-[13px] text-gold-200">{t("afkSettings.stale")}</p>
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

function SheetSkeleton() {
  return (
    <div className="flex min-h-0 flex-1">
      <div className="w-[208px] shrink-0 space-y-2 border-r border-white/7 p-3">
        {SECTIONS.map((s) => (
          <Skeleton key={s} className="h-9" />
        ))}
      </div>
      <div className="flex-1 space-y-4 p-7">
        <Skeleton className="h-6 w-48" />
        <Skeleton className="h-4 w-80" />
        <Skeleton className="h-10 w-full max-w-[520px]" />
        <Skeleton className="h-10 w-full max-w-[520px]" />
        <Skeleton className="h-10 w-72" />
      </div>
    </div>
  );
}

type SaveMode = "save" | "reconnect";

function Editor({
  account,
  view,
  onDirtyChange,
  onDone,
  onCancel,
}: {
  account: AfkAccount;
  view: AfkSettingsView;
  onDirtyChange: (dirty: boolean) => void;
  onDone: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const issueText = useIssueText();
  const baseline = useMemo(() => toDraft(view.settings), [view.settings]);
  const [draft, setDraftState] = useState<Draft>(baseline);
  const [section, setSection] = useState<SectionId>("server");
  const [submitted, setSubmitted] = useState(false);
  const [serverIssues, setServerIssues] = useState<Issue[]>([]);
  const [saveError, setSaveError] = useState<Failure | null>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const active = isActive(account);

  const dirty = isDirty(draft, baseline);
  useEffect(() => onDirtyChange(dirty), [dirty, onDirtyChange]);

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

  const errorFor = (field: string) => issueText(issues.find((i) => i.field === field));
  const sectionHasIssue = (id: SectionId) => issues.some((i) => i.section === id);
  const openFolder = () => runAfk(api.afkOpenFolder(account.id), t("afk.openFolderFailed"));

  const goTo = (id: SectionId) => {
    setSection(id);
    contentRef.current?.scrollTo({ top: 0 });
  };

  const save = useMutation({
    mutationFn: (_mode: SaveMode) => api.afkSaveSettings(account.id, fromDraft(draft)),
    onSuccess: (result, mode) => {
      if (result.status === "invalid") {
        const found = fromServerIssues(result.issues, draft);
        setServerIssues(found);
        if (found[0]) goTo(found[0].section);
        return;
      }
      // The sheet closes; the next open loads the stored (normalized) settings again.
      void queryClient.invalidateQueries({ queryKey: ["afk"] });
      const saved = t("afkSettings.saved", { name: account.nickname });
      if (result.reconnectNeeded && mode === "reconnect") {
        runAfk(api.afkReconnect(account.id), t("afkSettings.reconnectFailed"));
        toast.success(saved, { description: t("afkSettings.savedReconnecting") });
      } else if (result.reconnectNeeded) {
        toast.success(saved, {
          description: t("afkSettings.savedReconnect"),
          duration: 10_000,
          action: {
            label: t("afkSettings.reconnect"),
            // The sheet is closed by then, so no component-bound mutation here.
            onClick: () => runAfk(api.afkReconnect(account.id), t("afkSettings.reconnectFailed")),
          },
        });
      } else {
        toast.success(saved);
      }
      onDone();
    },
    onError: (error) => setSaveError(failure(error, t("afkSettings.saveFailed"))),
  });

  const submit = (mode: SaveMode) => {
    setSubmitted(true);
    const found = [...validate(draft), ...patternIssues];
    if (found.length) {
      const here = found.some((i) => i.section === section);
      if (!here && found[0]) goTo(found[0].section);
      return;
    }
    save.mutate(mode);
  };

  // Ctrl+S saves (without the reconnect: the safe choice for a shortcut).
  const shortcut = useRef(() => {});
  shortcut.current = () => {
    if (dirty && !save.isPending) submit("save");
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

  const blocking = submitted && issues.length > 0;
  const pending = save.isPending ? save.variables : null;
  // On a running account the main action applies the settings right away.
  const mainMode: SaveMode = active ? "reconnect" : "save";

  return (
    <>
      <Tabs.Root
        value={section}
        onValueChange={(v) => goTo(v as SectionId)}
        orientation="vertical"
        className="flex min-h-0 flex-1"
      >
        <Tabs.List
          aria-label={t("afkSettings.sectionsLabel")}
          className="flex w-[208px] shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-white/7 p-3"
        >
          {SECTIONS.map((id) => {
            const Icon = ICONS[id];
            const on = id === section;
            const enabled = sectionEnabled(draft.page, id);
            const problem = sectionHasIssue(id);
            return (
              <Tabs.Trigger
                key={id}
                value={id}
                onMouseDown={focusTabQuietly}
                className={cn(
                  "flex h-9 w-full cursor-pointer items-center gap-2.5 rounded-md px-2.5 text-left text-[13px] font-medium transition-colors focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none",
                  on
                    ? "bg-white/8 text-bone-50"
                    : "text-ash-300 hover:bg-white/5 hover:text-bone-50",
                )}
              >
                <Icon
                  size={17}
                  weight={on ? "duotone" : "regular"}
                  className={cn("shrink-0", on && "text-gold-300")}
                />
                <span className="min-w-0 flex-1 truncate">{t(`afkSettings.sections.${id}`)}</span>
                {problem ? (
                  <WarningCircle
                    size={16}
                    weight="fill"
                    className="shrink-0 text-flare-400"
                    aria-label={t("afkSettings.sectionHasErrors")}
                  />
                ) : enabled === null ? null : (
                  <span
                    className={cn(
                      "shrink-0 text-[11.5px]",
                      enabled ? "text-bone-100/80" : "text-ash-400",
                    )}
                  >
                    {enabled ? t("afkSettings.on") : t("afkSettings.off")}
                  </span>
                )}
              </Tabs.Trigger>
            );
          })}
        </Tabs.List>
        <div ref={contentRef} className="min-h-0 flex-1 overflow-y-auto px-7 pt-6 pb-8">
          <StaleBanner account={account} />
          {SECTIONS.map((id) => {
            const Body = BODIES[id];
            return (
              <Tabs.Content key={id} value={id} className="max-w-[680px] outline-none">
                <Body
                  draft={draft}
                  setDraft={setDraft}
                  errorFor={errorFor}
                  view={view}
                  onOpenFolder={openFolder}
                />
              </Tabs.Content>
            );
          })}
        </div>
      </Tabs.Root>

      <footer className="flex items-center gap-3 border-t border-white/8 bg-obsidian-950/40 px-5 py-3.5">
        <div className="min-w-0 flex-1 text-[12.5px]">
          {saveError ? (
            <div className="flex items-center gap-2">
              <p role="alert" className="min-w-0 text-flare-400">
                {saveError.message}
              </p>
              <CopyDetails error={saveError.error} />
            </div>
          ) : blocking ? (
            <p role="alert" className="text-flare-400">
              {t("afkSettings.fixErrors")}
            </p>
          ) : dirty ? (
            <p className="text-gold-200">{t("afkSettings.dirty")}</p>
          ) : null}
        </div>
        <Button variant="ghost" onClick={onCancel} disabled={save.isPending}>
          {dirty ? t("afkSettings.cancel") : t("afkSettings.close")}
        </Button>
        {active ? (
          <Button
            variant="secondary"
            onClick={() => submit("save")}
            disabled={!dirty || save.isPending}
          >
            {pending === "save" ? t("afkSettings.saving") : t("afkSettings.save")}
          </Button>
        ) : null}
        <Button
          variant="primary"
          onClick={() => submit(mainMode)}
          disabled={!dirty || save.isPending}
        >
          {pending === mainMode
            ? t("afkSettings.saving")
            : active
              ? t("afkSettings.saveReconnect")
              : t("afkSettings.save")}
        </Button>
      </footer>
    </>
  );
}

/** The settings sheet of one AFK account. Closing with unsaved changes asks first. */
export function AfkSettingsSheet({
  account,
  onClose,
}: {
  account: AfkAccount | undefined;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const animate = useInteractionMotion();
  const open = Boolean(account);
  // Both belong to one account: switching accounts never inherits them.
  const dirtyRef = useRef<{ id: string; dirty: boolean } | null>(null);
  const [confirmFor, setConfirmFor] = useState<string | null>(null);
  const confirmOpen = open && confirmFor === account?.id;
  const setConfirmOpen = (next: boolean) => setConfirmFor(next ? (account?.id ?? null) : null);
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

  const close = () => {
    dirtyRef.current = null;
    setConfirmFor(null);
    onClose();
  };
  const requestClose = () => {
    const dirty = dirtyRef.current;
    if (account && dirty?.id === account.id && dirty.dirty) setConfirmOpen(true);
    else close();
  };

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && requestClose()}>
      <AnimatePresence>
        {open && account ? (
          <EntranceGate key="sheet">
            <Dialog.Portal forceMount>
              <Dialog.Overlay asChild forceMount>
                <GatedMotion.div
                  // One layer below the shared Dialog, so its confirmation dims the sheet too.
                  className="fixed inset-0 z-30 bg-obsidian-950/55"
                  {...scrimMotion(!animate)}
                />
              </Dialog.Overlay>
              <Dialog.Content
                asChild
                forceMount
                aria-describedby={undefined}
                // The panel takes the focus, not the close button (its tooltip would pop open).
                onOpenAutoFocus={(event) => {
                  event.preventDefault();
                  focusPanel(event.currentTarget as HTMLElement);
                }}
                // A form saved as a whole: a stray click beside the panel never closes it (Esc
                // and the close button still do, asking first when something changed).
                onPointerDownOutside={(event) => event.preventDefault()}
                onInteractOutside={(event) => event.preventDefault()}
              >
                <GatedMotion.aside
                  tabIndex={-1}
                  className="glass-strong fixed top-0 right-0 bottom-0 z-40 flex w-[min(940px,94vw)] flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none"
                  {...sheetMotion(!animate)}
                >
                  <header className="flex items-center gap-3.5 border-b border-white/8 px-5 py-4">
                    <AfkHead nickname={account.nickname} size={36} />
                    <div className="min-w-0 flex-1">
                      <Dialog.Title className="truncate font-display text-[18px] font-semibold text-bone-50">
                        {t("afkSettings.title", { name: account.nickname })}
                      </Dialog.Title>
                      <p className="truncate text-[12.5px] text-ash-400" data-selectable>
                        {account.server}
                      </p>
                    </div>
                    <IconButton
                      label={t("common.close")}
                      icon={<X size={18} />}
                      size="sm"
                      onClick={requestClose}
                    />
                  </header>

                  {view ? (
                    <Editor
                      key={account.id}
                      account={account}
                      view={view}
                      onDirtyChange={(dirty) => {
                        dirtyRef.current = { id: account.id, dirty };
                      }}
                      onDone={close}
                      onCancel={close}
                    />
                  ) : loadError ? (
                    <LoadFailed
                      failed={failure(loadError, t("afkSettings.loadFailedHint"))}
                      retrying={isFetching}
                      onRetry={() => void refetch()}
                    />
                  ) : (
                    <SheetSkeleton />
                  )}

                  <ConfirmDialog
                    open={confirmOpen}
                    onOpenChange={setConfirmOpen}
                    title={t("afkSettings.discard.title")}
                    description={t("afkSettings.discard.body")}
                    closeLabel={t("common.close")}
                    width={440}
                    footer={
                      <>
                        <Button variant="ghost" onClick={() => setConfirmOpen(false)}>
                          {t("afkSettings.discard.keep")}
                        </Button>
                        <Button variant="danger" onClick={close}>
                          {t("afkSettings.discard.confirm")}
                        </Button>
                      </>
                    }
                  />
                </GatedMotion.aside>
              </Dialog.Content>
            </Dialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}
