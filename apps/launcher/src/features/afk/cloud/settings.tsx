// A cloud account's settings in a side sheet, as the desktop AFK settings: Genel, Komutlar and
// Otomatik yanıt in tabs, problems where they are, and a footer that knows about unsaved changes.
// Secret rows show their words when this computer sealed them (the keychain copy), else "Göster"
// asks the account's machine. Saving restarts a running account with the new settings; a server
// change seals the secrets this computer has again and asks for the others.
import { Copy, X } from "@phosphor-icons/react";
import {
  Button,
  Dialog as ConfirmDialog,
  EntranceGate,
  focusPanel,
  GatedMotion,
  IconButton,
  Skeleton,
  scrimMotion,
  sheetMotion,
  TabPanel,
  Tabs,
  toast,
  useInteractionMotion,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence } from "motion/react";
import { Dialog } from "radix-ui";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { copyErrorDetails } from "@/features/content/jobs";
import { AfkHead } from "../afk-head";
import { isFriendly } from "../errors";
import { type CloudAccount, type CloudEditData, cloudApi } from "./api";
import {
  type Draft,
  heldIssue,
  type Issue,
  isDirty,
  localIssues,
  serverChanges,
  toDraft,
  toEdit,
} from "./settings-model";
import { CommandsPart, GeneralPart, RepliesPart } from "./settings-sections";
import { isCloudActive, serverLabel } from "./status";
import { CLOUD_KEY, storeAccount, useCloudPage } from "./use-cloud";

type Part = "general" | "commands" | "replies";
const PARTS: Part[] = ["general", "commands", "replies"];

const partOf = (issue: Issue): Part =>
  issue.place === "commands" ? "commands" : issue.place === "rules" ? "replies" : "general";

function SheetSkeleton() {
  return (
    <div className="flex-1 space-y-4 px-7 pt-6">
      <Skeleton className="h-6 w-48" />
      <Skeleton className="h-10 w-full max-w-[520px]" />
      <Skeleton className="h-10 w-full max-w-[520px]" />
      <Skeleton className="h-10 w-72" />
    </div>
  );
}

function Editor({
  view,
  onDirtyChange,
  onDone,
  onCancel,
  onReload,
}: {
  view: CloudEditData;
  onDirtyChange: (dirty: boolean) => void;
  onDone: () => void;
  onCancel: () => void;
  onReload: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const baseline = useMemo(() => toDraft(view), [view]);
  const [draft, setDraftState] = useState<Draft>(baseline);
  // A stop that names a row opens the sheet on it.
  const [part, setPart] = useState<Part>(() => {
    const held = heldIssue(view, baseline);
    return held ? partOf(held) : "general";
  });
  const [submitted, setSubmitted] = useState(false);
  const [serverIssues, setServerIssues] = useState<Issue[]>([]);
  const [failure, setFailure] = useState<{ message: string; error: unknown } | null>(null);
  const [conflict, setConflict] = useState(false);
  const [saving, setSaving] = useState(false);
  const account = view.account;
  // Studio's placeholder switch: its checks answer while typing too (the backend checks again).
  const features = useCloudPage().data?.overview.features ?? null;
  const rules = useMemo(
    () => ({
      on: Boolean(features?.placeholders),
      commands: features?.placeholderCommands ?? [],
    }),
    [features],
  );

  const dirty = isDirty(draft, baseline);
  useEffect(() => onDirtyChange(dirty), [dirty, onDirtyChange]);

  const setDraft = (update: (d: Draft) => Draft) => {
    setDraftState(update);
    setServerIssues([]);
    setFailure(null);
  };

  const issues = useMemo(() => {
    const own = submitted ? localIssues(draft, view.limits, rules) : [];
    // A server change needs the secrets only the cloud has typed again (they name the server).
    const reseal = serverChanges(draft, view)
      ? [
          ...draft.config.commands.flatMap((c, index) =>
            c.words.kind === "sealed"
              ? [{ place: "commands", index, field: "text", problem: "reseal" }]
              : [],
          ),
          ...draft.config.rules.flatMap((r, index) =>
            r.reply.kind === "sealed"
              ? [{ place: "rules", index, field: "reply", problem: "reseal" }]
              : [],
          ),
        ]
      : [];
    return [...own, ...reseal, ...serverIssues];
  }, [draft, submitted, serverIssues, view, rules]);
  // Shown with the rest, never blocking: see heldIssue.
  const held = heldIssue(view, draft);
  const shown = held ? [...issues, held] : issues;

  const save = async () => {
    setSubmitted(true);
    const found = [...localIssues(draft, view.limits, rules), ...issues];
    if (found.length) {
      const first = found[0];
      if (first && !found.some((i) => partOf(i) === part)) setPart(partOf(first));
      return;
    }
    setSaving(true);
    try {
      const result = await cloudApi.save(account.id, toEdit(draft, baseline, view));
      if (result.kind === "saved") {
        storeAccount(queryClient, result.account);
        void queryClient.invalidateQueries({ queryKey: CLOUD_KEY });
        toast.success(t("cloudAfk.settings.saved", { name: result.account.name }));
        onDone();
      } else if (result.kind === "invalid") {
        setServerIssues(result.issues);
        const first = result.issues[0];
        if (first) setPart(partOf(first));
      } else {
        storeAccount(queryClient, result.account);
        setConflict(true);
      }
    } catch (error) {
      setFailure({
        message: isFriendly(error) ? error.message : t("cloudAfk.actions.failed"),
        error,
      });
    } finally {
      setSaving(false);
    }
  };

  // Ctrl+S saves, as in the desktop settings.
  const shortcut = useRef(() => {});
  shortcut.current = () => {
    if (dirty && !saving) void save();
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

  const partDirty = (p: Part) =>
    p === "general"
      ? JSON.stringify([
          draft.name,
          draft.host,
          draft.port,
          draft.version,
          draft.lang,
          draft.config.antiAfk,
          draft.config.owner,
          draft.config.inventory,
          draft.config.movement,
        ]) !==
        JSON.stringify([
          baseline.name,
          baseline.host,
          baseline.port,
          baseline.version,
          baseline.lang,
          baseline.config.antiAfk,
          baseline.config.owner,
          baseline.config.inventory,
          baseline.config.movement,
        ])
      : p === "commands"
        ? JSON.stringify(draft.config.commands) !== JSON.stringify(baseline.config.commands)
        : JSON.stringify(draft.config.rules) !== JSON.stringify(baseline.config.rules);

  const props = { draft, setDraft, view, issues: shown, features };
  const blocking = submitted && issues.length > 0;
  return (
    <>
      <div className="flex min-h-0 flex-1 flex-col px-6 pt-2">
        <Tabs
          value={part}
          onValueChange={setPart}
          label={t("cloudAfk.settings.title", { name: account.name })}
          items={PARTS.map((value) => ({
            value,
            label: t(`cloudAfk.settings.sections.${value}`),
            marker: partDirty(value) ? t("cloudAfk.settings.dirty") : undefined,
          }))}
        >
          <TabPanel value="general" className="pb-8">
            <GeneralPart {...props} />
          </TabPanel>
          <TabPanel value="commands" className="pb-8">
            <CommandsPart {...props} />
          </TabPanel>
          <TabPanel value="replies" className="pb-8">
            <RepliesPart {...props} />
          </TabPanel>
        </Tabs>
      </div>

      <footer className="flex items-center gap-3 border-t border-white/8 bg-obsidian-950/40 px-5 py-3.5">
        <div className="min-w-0 flex-1 text-[12.5px]">
          {conflict ? (
            <div className="flex items-center gap-2">
              <p role="alert" className="min-w-0 text-flare-400">
                {t("cloudAfk.settings.conflict")}
              </p>
              <Button size="sm" variant="ghost" onClick={onReload}>
                {t("cloudAfk.settings.loadLatest")}
              </Button>
            </div>
          ) : failure ? (
            <div className="flex items-center gap-2">
              <p role="alert" className="min-w-0 text-flare-400">
                {failure.message}
              </p>
              <Button
                size="sm"
                variant="ghost"
                icon={<Copy size={14} />}
                onClick={() => void copyErrorDetails(failure.error)}
              >
                {t("common.copyDetails")}
              </Button>
            </div>
          ) : blocking ? (
            <p role="alert" className="text-flare-400">
              {serverChanges(draft, view) && issues.some((i) => i.problem === "reseal")
                ? t("cloudAfk.settings.resealTitle")
                : t("cloudAfk.settings.fixFirst")}
            </p>
          ) : dirty ? (
            <p className="text-gold-200">
              {isCloudActive(account)
                ? t("cloudAfk.settings.restartNote")
                : t("cloudAfk.settings.dirty")}
            </p>
          ) : null}
        </div>
        <Button variant="ghost" onClick={onCancel} disabled={saving}>
          {dirty ? t("cloudAfk.settings.cancel") : t("cloudAfk.settings.close")}
        </Button>
        <Button variant="primary" onClick={() => void save()} disabled={!dirty || saving}>
          {saving ? t("cloudAfk.settings.saving") : t("cloudAfk.settings.save")}
        </Button>
      </footer>
    </>
  );
}

/** The settings sheet of one cloud account. Closing with unsaved changes asks first. */
export function CloudSettingsSheet({
  account,
  onClose,
}: {
  account: CloudAccount | undefined;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const animate = useInteractionMotion();
  const open = Boolean(account);
  const dirtyRef = useRef<{ id: string; dirty: boolean } | null>(null);
  const [confirmFor, setConfirmFor] = useState<string | null>(null);
  const [generation, setGeneration] = useState(0);
  const confirmOpen = open && confirmFor === account?.id;
  const {
    data: view,
    error: loadError,
    refetch,
    isFetching,
  } = useQuery({
    queryKey: ["cloud-afk-edit", account?.id],
    queryFn: () => cloudApi.edit(account?.id ?? ""),
    enabled: open,
    staleTime: 0,
    gcTime: 0,
    refetchOnWindowFocus: false,
    retry: false,
  });

  const close = () => {
    dirtyRef.current = null;
    setConfirmFor(null);
    onClose();
  };
  const requestClose = () => {
    const dirty = dirtyRef.current;
    if (account && dirty?.id === account.id && dirty.dirty) setConfirmFor(account.id);
    else close();
  };
  const reload = () => {
    void refetch().then(() => setGeneration((n) => n + 1));
  };

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && requestClose()}>
      <AnimatePresence>
        {open && account ? (
          <EntranceGate key="sheet">
            <Dialog.Portal forceMount>
              <Dialog.Overlay asChild forceMount>
                <GatedMotion.div
                  className="fixed inset-0 z-30 bg-obsidian-950/55"
                  {...scrimMotion(!animate)}
                />
              </Dialog.Overlay>
              <Dialog.Content
                asChild
                forceMount
                aria-describedby={undefined}
                onOpenAutoFocus={(event) => {
                  event.preventDefault();
                  focusPanel(event.currentTarget as HTMLElement);
                }}
                // A form saved as a whole: a stray click beside the panel never closes it.
                onPointerDownOutside={(event) => event.preventDefault()}
                onInteractOutside={(event) => event.preventDefault()}
              >
                <GatedMotion.aside
                  tabIndex={-1}
                  className="glass-strong fixed top-0 right-0 bottom-0 z-40 flex w-[min(780px,94vw)] flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none"
                  {...sheetMotion(!animate)}
                >
                  <header className="flex items-center gap-3.5 border-b border-white/8 px-5 py-4">
                    <AfkHead nickname={account.name} size={36} />
                    <div className="min-w-0 flex-1">
                      <Dialog.Title className="truncate font-display text-[18px] font-semibold text-bone-50">
                        {t("cloudAfk.settings.title", { name: account.name })}
                      </Dialog.Title>
                      <p className="truncate text-[12.5px] text-ash-400" data-selectable>
                        {serverLabel(account.server)}
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
                      key={`${account.id}-${generation}-${view.account.rev}`}
                      view={view}
                      onDirtyChange={(dirty) => {
                        dirtyRef.current = { id: account.id, dirty };
                      }}
                      onDone={close}
                      onCancel={requestClose}
                      onReload={reload}
                    />
                  ) : loadError ? (
                    <div className="flex flex-1 flex-col items-center justify-center gap-2 p-10 text-center">
                      <p className="font-display text-lg font-semibold text-bone-50">
                        {t("cloudAfk.settings.loadFailed")}
                      </p>
                      {isFriendly(loadError) ? (
                        <p className="max-w-[46ch] text-[13px] text-ash-400">{loadError.message}</p>
                      ) : null}
                      <div className="mt-3 flex gap-2">
                        <Button
                          variant="secondary"
                          onClick={() => void refetch()}
                          disabled={isFetching}
                        >
                          {t("common.retry")}
                        </Button>
                      </div>
                    </div>
                  ) : (
                    <SheetSkeleton />
                  )}

                  <ConfirmDialog
                    open={confirmOpen}
                    onOpenChange={(next) => setConfirmFor(next ? (account.id ?? null) : null)}
                    title={t("cloudAfk.settings.discard.title")}
                    description={t("cloudAfk.settings.discard.body")}
                    closeLabel={t("common.close")}
                    width={440}
                    footer={
                      <>
                        <Button variant="ghost" onClick={() => setConfirmFor(null)}>
                          {t("cloudAfk.settings.discard.keep")}
                        </Button>
                        <Button variant="danger" onClick={close}>
                          {t("cloudAfk.settings.discard.confirm")}
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
