// "Görünüm" of a username account: its Projects skin (SKIN1, skin1.md §13.2). A right sheet with
// the same width and parts as Microsoft's Görünüm panel (preview, drop zone, arm model); the skin
// is saved on Projects through the player's Discord link and shows at their next login there.
// What shows in each state comes from rules.ts (`panelState`); this file draws it, keeps the pick
// and follows the job it sent until it closes.
import { ArrowCounterClockwise, X } from "@phosphor-icons/react";
import {
  Button,
  Dialog as ConfirmDialog,
  DropZone,
  EntranceGate,
  easeOutExpo,
  focusPanel,
  GatedMotion,
  IconButton,
  Skeleton,
  SwapLabel,
  scrimMotion,
  sheetMotion,
  useInteractionMotion,
  useLiveMotion,
  useMotionPrefs,
} from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";
import { Dialog } from "radix-ui";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AccountFace } from "@/features/accounts/account-face";
import { useAccountsUi } from "@/features/accounts/store";
import { useAccounts } from "@/features/accounts/use-accounts";
import { useDropAnnouncement, useDropZone } from "@/features/drop/store";
import { announcementOf, zoneTitle } from "@/features/drop/text";
import { isSkinFile } from "@/features/drop/zones";
import { inferVariant } from "@/features/skins/skin-art";
import { useSocialUi } from "@/features/social/store";
import type { AccountSummary } from "@/lib/api";
import { projectsSkinApi } from "@/lib/api/projects-skin-api";
import type {
  ProjectsSkinChoice,
  ProjectsSkinSave,
  ProjectsSkinView,
  SkinVariant,
} from "@/lib/api/projects-skin-types";
import {
  CheckingLine,
  ConnectCard,
  LinkCodeBox,
  LinkLine,
  OtherAccountCard,
  UnlinkDialog,
} from "./link-area";
import {
  stateKey,
  useDefaultLook,
  useProjectsSkinOverview,
  useProjectsSkinState,
  useProjectsSkinUi,
} from "./queries";
import {
  LONG_WAIT_MS,
  type Note,
  needsCheck,
  panelState,
  refusalNote,
  SENDING_REVEAL_MS,
  skinResultNote,
  untoldFailure,
} from "./rules";
import { type Draft, SourcePicker, type SourceTab } from "./source-picker";
import { SkinStage, type StageSkin } from "./stage";
import { NoteText } from "./text";

const nowSec = () => Math.floor(Date.now() / 1000);

const fade = {
  initial: { opacity: 0 },
  animate: { opacity: 1, transition: { duration: 0.2, delay: 0.06 } },
  exit: { opacity: 0, transition: { duration: 0.16 } },
};

function Editor({
  account,
  view,
  onClose,
  onDirtyChange,
}: {
  account: AccountSummary;
  view: ProjectsSkinView;
  onClose: () => void;
  onDirtyChange: (dirty: boolean) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { layout: glide } = useMotionPrefs();
  const live = useLiveMotion();
  const { data: accounts = [] } = useAccounts();
  const signedIn = useSocialUi((s) => s.view.auth === "signedIn");
  const told = useProjectsSkinUi((s) => s.told);
  const markTold = useProjectsSkinUi((s) => s.markTold);

  const [tab, setTab] = useState<SourceTab>(view.skin?.kind === "premium" ? "name" : "png");
  const [draft, setDraft] = useState<Draft | null>(null);
  const [fileProblem, setFileProblem] = useState<string | null>(null);
  const [reading, setReading] = useState(false);
  const [sending, setSending] = useState(false);
  const [sendingShown, setSendingShown] = useState(false);
  const [failure, setFailure] = useState<Note | null>(null);
  const [sentJob, setSentJob] = useState<string | null>(null);
  const [ringKey, setRingKey] = useState(0);
  const [resetKey, setResetKey] = useState(0);
  const [waitedLong, setWaitedLong] = useState(false);
  const [unlink, setUnlink] = useState<{ open: boolean; movingFrom: string | null }>({
    open: false,
    movingFrom: null,
  });
  /** The code box shows its gold plate a moment after the link already arrived. */
  const [boxHeld, setBoxHeld] = useState(false);

  const panel = panelState({
    view,
    signedIn,
    draft: draft !== null,
    sending,
    failure,
    waitedLong,
    nowSec: nowSec(),
  });
  const dirty = draft !== null && !view.job && !sending;
  useEffect(() => onDirtyChange(dirty), [dirty, onDirtyChange]);

  const refresh = () => void queryClient.invalidateQueries({ queryKey: ["projects-skin"] });

  // Opening asks ProBot about a link that is not known or a day old (once).
  // biome-ignore lint/correctness/useExhaustiveDependencies: once per panel
  useEffect(() => {
    if (view.access === "ready" && !view.linkJob && needsCheck(view.link, nowSec())) {
      void projectsSkinApi.linkCheck().catch(() => {});
    }
    // A result that waited in the service while the launcher was closed is told once.
    const waited = untoldFailure(view.lastJob, told, Date.now());
    if (waited && view.lastJob) {
      setFailure(waited);
      markTold(view.lastJob.id);
    }
  }, []);

  // The job this panel sent: its end clears the pick (saved) or says why (failed).
  useEffect(() => {
    if (!sentJob || view.job?.id === sentJob) return;
    const last = view.lastJob;
    if (last?.id !== sentJob) return;
    setSentJob(null);
    markTold(last.id);
    if (last.result?.code === "applied") {
      setDraft(null);
      setResetKey((n) => n + 1);
      setRingKey((n) => n + 1);
    } else {
      setFailure(skinResultNote(last.result));
    }
  }, [view, sentJob, markTold]);

  // "Biraz uzun sürüyor" a minute after the job opened (one timer, nothing ticks).
  const jobId = view.job?.id ?? null;
  const jobOpenedAt = view.job?.createdAtMs ?? 0;
  useEffect(() => {
    setWaitedLong(false);
    if (!jobId) return;
    const left = LONG_WAIT_MS - (Date.now() - jobOpenedAt);
    if (left <= 0) {
      setWaitedLong(true);
      return;
    }
    const timer = window.setTimeout(() => setWaitedLong(true), left);
    return () => window.clearTimeout(timer);
  }, [jobId, jobOpenedAt]);

  const busyRef = useRef(false);
  const send = async (what: ProjectsSkinSave, shown: StageSkin | null) => {
    if (busyRef.current) return;
    busyRef.current = true;
    setSending(true);
    setFailure(null);
    const reveal = window.setTimeout(() => setSendingShown(true), SENDING_REVEAL_MS);
    try {
      const sent = await projectsSkinApi.save(account.id, what);
      if (sent.status === "sent") {
        setSentJob(sent.job.id);
        // In line at once, showing what was sent; the service's answer follows.
        queryClient.setQueryData<ProjectsSkinView>(stateKey(account.id), (old) =>
          old
            ? {
                ...old,
                job: {
                  ...sent.job,
                  picture: sent.job.picture ?? shown?.picture ?? null,
                  variant: sent.job.variant ?? shown?.variant ?? null,
                },
              }
            : old,
        );
      } else {
        setFailure(refusalNote(sent.refusal, nowSec()));
      }
    } catch {
      setFailure({ key: "saveFailed", tone: "error" });
    } finally {
      window.clearTimeout(reveal);
      busyRef.current = false;
      setSending(false);
      setSendingShown(false);
      refresh();
    }
  };

  const save = () => {
    if (!draft) return;
    void send(
      { kind: "choice", id: draft.choice.id, variant: draft.variant },
      { picture: draft.choice.picture, variant: draft.variant },
    );
  };

  const backToDefault = () => {
    setDraft(null);
    setResetKey((n) => n + 1);
    void send({ kind: "default" }, null);
  };

  const revert = () => {
    setDraft(null);
    setFileProblem(null);
    setFailure(null);
    setResetKey((n) => n + 1);
  };

  const intake = async (path: string) => {
    setFileProblem(null);
    setFailure(null);
    setReading(true);
    setTab("png");
    try {
      const pick = await projectsSkinApi.readFile(path);
      if (pick.status === "refused") {
        setFileProblem(
          pick.reason === "unreadable"
            ? t("skins.file.readFailed")
            : t(`projectsSkin.file.${pick.reason}`),
        );
        return;
      }
      const { choice } = pick;
      const guess = choice.legacy ? null : await inferVariant(choice.picture).catch(() => null);
      setDraft({ choice, variant: guess ?? "classic", guessed: guess !== null });
    } catch {
      setFileProblem(t("skins.file.readFailed"));
    } finally {
      setReading(false);
    }
  };

  const choose = async () => {
    try {
      const path = await projectsSkinApi.pickFile();
      if (path) await intake(path);
    } catch {
      setFileProblem(t("skins.file.pickFailed"));
    }
  };

  const onPremium = (choice: ProjectsSkinChoice | null) => {
    setFailure(null);
    setDraft((current) =>
      choice
        ? { choice, variant: choice.variant, guessed: false }
        : current?.choice.kind === "premium"
          ? null
          : current,
    );
  };

  // Files dropped anywhere on the window while the picker is open come here (the window-wide
  // drop layer steps aside meanwhile); the panel shows the zone over its body.
  const dropBlocked = panel.body !== "editor" || panel.picker !== "open" || reading;
  const blockedRef = useRef(dropBlocked);
  blockedRef.current = dropBlocked;
  const intakeRef = useRef(intake);
  intakeRef.current = intake;
  const drop = useDropZone({
    layer: "claim",
    accepts: (drag) => !blockedRef.current && drag.paths.some(isSkinFile),
    onDrop: (paths) => {
      if (blockedRef.current) return;
      const path = paths.find(isSkinFile) ?? paths[0];
      if (path) void intakeRef.current(path);
    },
  });
  const dropTitle = zoneTitle(t, drop.state, drop.drag?.paths.length ?? 1);
  const dropHint = drop.state === "refuse" ? t("skins.file.dropRefuse") : t("skins.file.dropHint");
  const dropShown = (drop.state !== "idle" && !dropBlocked) || drop.state === "absorb";
  useDropAnnouncement(
    dropShown && drop.state !== "absorb" ? announcementOf(drop.state, dropTitle, dropHint) : null,
    drop.drag?.id,
  );

  // What the big preview shows (the default look while a default is in line or nothing is set).
  const look = useDefaultLook(
    account.id,
    panel.preview === "default" || (panel.preview === "job" && !view.job?.picture),
  );
  const stageSkin: StageSkin =
    panel.preview === "job" && view.job?.picture
      ? { picture: view.job.picture, variant: view.job.variant ?? "classic" }
      : panel.preview === "draft" && draft
        ? { picture: draft.choice.picture, variant: draft.variant }
        : panel.preview === "projects" && view.skin?.picture
          ? { picture: view.skin.picture, variant: view.skin.variant }
          : { picture: look.data?.picture ?? null, variant: look.data?.variant ?? "classic" };
  const breathing = live && panel.chip === "inLine" && !waitedLong;

  const headOf = (name: string) =>
    accounts.find((a) => a.username.toLowerCase() === name.toLowerCase())?.headUrl ?? null;
  const layoutGlide = glide ? { duration: 0.3, ease: easeOutExpo } : { duration: 0 };

  let top: ReactNode = null;
  let topKey = "";
  if (panel.link.kind === "code" || boxHeld) {
    topKey = "code";
    top = (
      <LinkCodeBox
        accountId={account.id}
        disabled={panel.link.kind === "code" && panel.link.dim}
        retryAt={view.limits.linkRetryAt}
        onSuccess={() => setBoxHeld(true)}
        onLinked={() => {
          setBoxHeld(false);
          refresh();
        }}
        onOther={() => refresh()}
      />
    );
  } else if (panel.link.kind === "other") {
    topKey = "other";
    const name = panel.link.name;
    top = (
      <OtherAccountCard
        name={name}
        headUrl={headOf(name)}
        disabled={panel.link.dim}
        onLinkThis={() => setUnlink({ open: true, movingFrom: name })}
      />
    );
  }

  const bottom: ReactNode =
    panel.link.kind === "checking" ? (
      <CheckingLine />
    ) : panel.link.kind === "linked" && !boxHeld ? (
      <LinkLine
        name={panel.link.name}
        dim={panel.link.dim || view.linkJob?.kind === "unlink"}
        onRemove={() => setUnlink({ open: true, movingFrom: null })}
      />
    ) : null;
  const bottomKey = panel.link.kind === "checking" ? "checking" : bottom ? "linked" : "";

  return (
    <>
      <div ref={drop.ref} className="relative flex min-h-0 flex-1">
        <div className="flex w-[320px] shrink-0 flex-col border-r border-white/7 p-5">
          <SkinStage skin={stageSkin} chip={panel.chip} breathing={breathing} ringKey={ringKey} />
        </div>

        {panel.body === "editor" ? (
          <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto px-7 pt-6 pb-5">
            <LayoutGroup id={`projects-skin-${account.id}`}>
              <AnimatePresence initial={false} mode="popLayout">
                {top ? (
                  <motion.div
                    key={topKey}
                    layout={glide ? "position" : false}
                    transition={{ layout: layoutGlide }}
                    className="mb-6"
                    {...fade}
                  >
                    {top}
                  </motion.div>
                ) : null}
              </AnimatePresence>
              <motion.div layout={glide ? "position" : false} transition={{ layout: layoutGlide }}>
                <SourcePicker
                  accountId={account.id}
                  mode={panel.picker}
                  tab={tab}
                  onTab={setTab}
                  draft={draft}
                  fileProblem={fileProblem}
                  reading={reading}
                  onChooseFile={() => void choose()}
                  onRemoveFile={() => {
                    setDraft((current) => (current?.choice.kind === "png" ? null : current));
                    setFileProblem(null);
                  }}
                  onVariant={(variant: SkinVariant) =>
                    setDraft((current) =>
                      current ? { ...current, variant, guessed: false } : null,
                    )
                  }
                  onPremium={onPremium}
                  resetKey={resetKey}
                />
              </motion.div>
              <AnimatePresence initial={false}>
                {panel.resetShown ? (
                  <motion.div
                    key="reset"
                    layout={glide ? "position" : false}
                    transition={{ layout: layoutGlide }}
                    className="mt-5 flex justify-end"
                    {...fade}
                  >
                    <Button
                      size="sm"
                      variant="ghost"
                      icon={<ArrowCounterClockwise size={15} />}
                      disabled={!panel.resetEnabled}
                      onClick={backToDefault}
                    >
                      {t("projectsSkin.reset")}
                    </Button>
                  </motion.div>
                ) : null}
              </AnimatePresence>
              <div className="mt-auto pt-6">
                <div className="border-t border-white/7 pt-3">
                  <AnimatePresence initial={false} mode="popLayout">
                    {bottom ? (
                      <motion.div key={bottomKey} {...fade}>
                        {bottom}
                      </motion.div>
                    ) : null}
                  </AnimatePresence>
                </div>
              </div>
            </LayoutGroup>
          </div>
        ) : panel.body === "off" ? (
          <div className="flex flex-1 items-center justify-center px-10 pb-10 text-center">
            <p className="max-w-[36ch] text-[14px] text-ash-300">{t("projectsSkin.note.off")}</p>
          </div>
        ) : (
          <ConnectCard
            session={panel.body === "session"}
            onConnect={() => {
              onClose();
              // The account center under it goes too (as from its Discord row): the panel
              // slides out first, the friends panel follows with its way in.
              useAccountsUi.getState().closeCenter();
              window.setTimeout(() => useSocialUi.getState().openPanel({ page: "list" }), 160);
            }}
          />
        )}

        <AnimatePresence>
          {dropShown ? (
            <EntranceGate key="drop">
              <DropZone
                aria-hidden
                variant="cover"
                state={drop.state}
                title={dropTitle}
                hint={dropHint}
                className="pointer-events-none absolute inset-3 z-10"
              />
            </EntranceGate>
          ) : null}
        </AnimatePresence>
      </div>

      {panel.body === "editor" ? (
        <footer className="flex min-h-[63px] items-center gap-3 border-t border-white/8 bg-obsidian-950/40 px-5 py-3.5">
          <div className="min-w-0 flex-1 text-[12.5px] leading-snug" aria-live="polite">
            <AnimatePresence initial={false} mode="popLayout">
              {panel.note ? (
                <motion.p
                  key={`${panel.note.key}-${panel.note.count ?? ""}-${panel.note.until ?? ""}`}
                  role={panel.note.tone === "error" ? "alert" : undefined}
                  className={panel.note.tone === "error" ? "text-flare-400" : "text-ash-300"}
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1, transition: { duration: 0.15, delay: 0.06 } }}
                  exit={{ opacity: 0, transition: { duration: 0.1 } }}
                >
                  <NoteText t={t} note={panel.note} />
                </motion.p>
              ) : null}
            </AnimatePresence>
          </div>
          <AnimatePresence initial={false} mode="popLayout">
            {panel.revertShown ? (
              <motion.span
                key="revert"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: { duration: 0.15 } }}
                exit={{ opacity: 0, transition: { duration: 0.1 } }}
              >
                <Button variant="ghost" onClick={revert}>
                  {t("skins.revert")}
                </Button>
              </motion.span>
            ) : null}
            {panel.saveShown ? (
              <motion.span
                key="save"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: { duration: 0.15 } }}
                exit={{ opacity: 0, transition: { duration: 0.1 } }}
              >
                <Button variant="primary" disabled={!panel.saveEnabled} onClick={save}>
                  <SwapLabel
                    labels={[t("projectsSkin.save"), t("projectsSkin.sending")]}
                    active={sendingShown ? 1 : 0}
                  />
                </Button>
              </motion.span>
            ) : null}
          </AnimatePresence>
        </footer>
      ) : null}

      <UnlinkDialog
        open={unlink.open}
        movingFrom={unlink.movingFrom}
        onOpenChange={(open) => setUnlink((current) => ({ ...current, open }))}
      />
    </>
  );
}

function SheetSkeleton() {
  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex w-[320px] shrink-0 flex-col border-r border-white/7 p-5">
        <div className="relative min-h-0 flex-1 rounded-[var(--radius-panel)] bg-obsidian-950/45 ring-1 ring-white/6" />
        <Skeleton className="mx-auto mt-3 h-4 w-36" />
      </div>
      <div className="flex-1 space-y-4 px-7 pt-6">
        <Skeleton className="h-[34px] w-[220px]" />
        <Skeleton className="h-[86px] w-full" />
        <Skeleton className="h-10 w-full" />
      </div>
    </div>
  );
}

function LoadError({ retrying, onRetry }: { retrying: boolean; onRetry: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 p-10 text-center">
      <p className="font-display text-lg font-semibold text-bone-50">{t("skins.loadFailed")}</p>
      <p role="alert" className="max-w-[48ch] text-ash-400">
        {t("projectsSkin.note.unreachable")}
      </p>
      <Button className="mt-4" variant="secondary" onClick={onRetry} disabled={retrying}>
        {t("common.retry")}
      </Button>
    </div>
  );
}

/** The side sheet of a username account. Closing with a pick not saved asks first. */
function ProjectsSkinSheetPanel({
  account,
  onClose,
}: {
  account: AccountSummary | null;
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
  const state = useProjectsSkinState(account?.id ?? "", open);

  const requestClose = () => {
    const dirty = dirtyRef.current;
    if (account && dirty?.id === account.id && dirty.dirty) setConfirmOpen(true);
    else onClose();
  };

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && requestClose()}>
      <AnimatePresence>
        {open && account ? (
          <EntranceGate key="sheet">
            <Dialog.Portal forceMount>
              <Dialog.Overlay asChild forceMount>
                <GatedMotion.div
                  // One layer below the shared Dialog, so its questions dim the sheet too.
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
                // Toasts live outside the sheet: "Geri al" there must not close it.
                onPointerDownOutside={(event) => {
                  if ((event.target as Element | null)?.closest?.("[data-sonner-toaster]")) {
                    event.preventDefault();
                  }
                }}
                onFocusOutside={(event) => {
                  if ((event.target as Element | null)?.closest?.("[data-sonner-toaster]")) {
                    event.preventDefault();
                  }
                }}
              >
                <GatedMotion.aside
                  tabIndex={-1}
                  className="glass-strong fixed top-0 right-0 bottom-0 z-40 flex w-[min(880px,94vw)] flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none"
                  {...sheetMotion(!animate)}
                >
                  <header className="flex items-center gap-3.5 border-b border-white/8 px-5 py-4">
                    <AccountFace account={account} size={36} />
                    <div className="min-w-0 flex-1">
                      <Dialog.Title className="truncate font-display text-[18px] font-semibold text-bone-50">
                        {t("skins.title")}
                      </Dialog.Title>
                      <p className="truncate text-[12.5px] text-ash-400">
                        <span className="text-ash-300" data-selectable>
                          {account.username}
                        </span>
                        <span aria-hidden className="px-1.5 text-ash-500">
                          ·
                        </span>
                        {t("projectsSkin.subtitle")}
                      </p>
                    </div>
                    <IconButton
                      label={t("common.close")}
                      icon={<X size={18} />}
                      size="sm"
                      onClick={requestClose}
                    />
                  </header>

                  {state.data ? (
                    <Editor
                      key={account.id}
                      account={account}
                      view={state.data}
                      onClose={onClose}
                      onDirtyChange={(dirty) => {
                        dirtyRef.current = { id: account.id, dirty };
                      }}
                    />
                  ) : state.isError ? (
                    <LoadError retrying={state.isFetching} onRetry={() => void state.refetch()} />
                  ) : (
                    <SheetSkeleton />
                  )}

                  <ConfirmDialog
                    open={confirmOpen}
                    onOpenChange={setConfirmOpen}
                    title={t("skins.discard.title")}
                    description={t("projectsSkin.discardBody")}
                    closeLabel={t("common.close")}
                    width={440}
                    footer={
                      <>
                        <Button variant="ghost" onClick={() => setConfirmOpen(false)}>
                          {t("skins.discard.keep")}
                        </Button>
                        <Button
                          variant="danger"
                          onClick={() => {
                            setConfirmOpen(false);
                            onClose();
                          }}
                        >
                          {t("skins.discard.confirm")}
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

/**
 * The Projects skin panel of a username account; `account` null closes it. Only while the signed
 * content offers the feature (Microsoft accounts keep Microsoft's own Görünüm panel).
 */
export function ProjectsSkinSheet({
  account,
  onClose,
}: {
  account: AccountSummary | null;
  onClose: () => void;
}) {
  const overview = useProjectsSkinOverview();
  const shown = Boolean(overview.data?.shown);
  return (
    <ProjectsSkinSheetPanel
      account={shown && account?.kind === "offline" ? account : null}
      onClose={onClose}
    />
  );
}
