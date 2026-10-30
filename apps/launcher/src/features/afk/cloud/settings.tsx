// A cloud account's settings as questions (SETUP-UX-L, the site's account page; SHEET-UX before it):
// in the shared right sheet, one quiet panel of rows, each a question with its item and its answer in
// plain words, opening in place into its fields: Nereye girsin?, Nasıl giriş yapsın?, Girince ne
// yapsın?, Biri yazınca ne desin? and Diğer ayarlar. Problems are marked on their question, a
// question holding unsaved edits carries the gold dot, and the change bar docks under the list
// while something is unsaved. While the account types nothing on joining and has no rule, a quiet
// "Kurulumu bitir" over the list asks the first four questions one a step (the same fields, the
// same save). Secret rows show their words when this computer sealed them (the keychain copy), else
// "Göster" asks the account's machine. Saving restarts a running account with the new settings; a
// server change seals the secrets this computer has again and asks for the others. A press
// outside, Esc or the close button closes the panel at any time; the edits wait as a draft.
import { Copy } from "@phosphor-icons/react";
import { Button, Skeleton, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { LayoutGroup, motion } from "motion/react";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { copyErrorDetails } from "@/features/content/jobs";
import { afkFailed, isFriendly } from "../errors";
import { Block } from "../settings/controls";
import {
  draftKey,
  type HeldDraft,
  rebaseDraft,
  remindKept,
  useAfkDrafts,
} from "../settings/drafts";
import { SettingsPanel } from "../settings/panel";
import { type CloudAccount, type CloudEditData, cloudApi } from "./api";
import { withoutRefusal } from "./moderation";
import { DOCK_HEIGHT, PlusChangeDock } from "./plus/change-dock";
import {
  type Command,
  type Draft,
  heldIssue,
  type Issue,
  isDirty,
  localIssues,
  loginPart,
  passwordIssue,
  serverChanges,
  settled,
  toDraft,
  toEdit,
} from "./settings-model";
import {
  CommandList,
  GamePart,
  IdentityPart,
  type PartProps,
  RepliesPart,
  type RowPlaces,
  ServerPart,
} from "./settings-sections";
import {
  ACCOUNT_ITEMS,
  ACCOUNT_STEPS,
  type AccountQuestion,
  type AccountStep,
  accountAnswers,
  accountNeedsSetup,
  type CommandPlace,
  commandPlace,
} from "./setup/answers";
import { QuestionList, QuestionRow } from "./setup/question-list";
import { type FrameStep, SetupDialog } from "./setup/setup-dialog";
import { isCloudActive, serverLabel, stoppedOnPassword } from "./status";
import { CLOUD_KEY, startOrAsk, storeAccount, useCloudPage } from "./use-cloud";

const QUESTIONS: AccountQuestion[] = ["server", "login", "join", "replies", "more"];

/** Accounts whose "Kurulumu bitir" was finished or put away this session: it goes for good then. */
const setupPutAway = new Set<string>();

/** The question a problem shows under. */
function questionOf(
  issue: Issue,
  draft: Draft,
  placeOf: (row: Command) => CommandPlace,
): AccountQuestion {
  if (issue.place === "commands") {
    const row = issue.index === null ? undefined : draft.config.commands[issue.index];
    const place = row ? placeOf(row) : "join";
    return place === "repeat" ? "more" : place;
  }
  if (issue.place === "rules" || issue.field === "owner") return "replies";
  if (issue.field === "server" || issue.field === "port" || issue.field === "version") {
    return "server";
  }
  return "more";
}

/** What each question edits: a question holding edits not saved yet carries the gold dot. */
function questionValue(
  draft: Draft,
  question: AccountQuestion,
  placeOf: (row: Command) => CommandPlace,
): string {
  const rows = (place: CommandPlace) => draft.config.commands.filter((c) => placeOf(c) === place);
  switch (question) {
    case "server":
      return JSON.stringify([draft.host, draft.port, draft.version]);
    case "login":
      return JSON.stringify(rows("login"));
    case "join":
      return JSON.stringify(rows("join"));
    case "replies":
      return JSON.stringify([draft.config.rules, draft.config.owner]);
    case "more":
      return JSON.stringify([
        draft.name,
        draft.lang,
        draft.config.antiAfk,
        draft.config.inventory,
        draft.config.movement,
        rows("repeat"),
      ]);
  }
}

/** One question's fields: the same in the list and in the setup's step. */
function QuestionFields({
  question,
  part,
  rows,
}: {
  question: AccountQuestion;
  part: PartProps;
  rows: RowPlaces;
}) {
  const { t } = useTranslation();
  switch (question) {
    case "server":
      return <ServerPart {...part} />;
    case "login":
      return (
        <CommandList
          part={part}
          rows={rows}
          place="login"
          addLabel={t("setup.account.loginAdd")}
          empty={t("setup.account.loginEmpty")}
        />
      );
    case "join":
      return (
        <CommandList
          part={part}
          rows={rows}
          place="join"
          addLabel={t("cloudAfk.settings.commands.add")}
          empty={t("cloudAfk.settings.commands.empty")}
        />
      );
    case "replies":
      return <RepliesPart {...part} />;
    case "more":
      return (
        <>
          <Block>
            <IdentityPart {...part} />
          </Block>
          <GamePart {...part} />
          <Block>
            <p className="pt-3 pb-1 text-[13px] font-semibold text-bone-100">
              {t("setup.account.repeatTitle")}
            </p>
            <div className="pb-3">
              <CommandList
                part={part}
                rows={rows}
                place="repeat"
                addLabel={t("cloudAfk.settings.commands.add")}
                empty={t("cloudAfk.settings.commands.empty")}
              />
            </div>
          </Block>
        </>
      );
  }
}

function Editor({
  view,
  onReload,
  onSaved,
}: {
  view: CloudEditData;
  /** Reads the stored settings again (after a conflict); the edits stay on top of them. */
  onReload: () => void;
  /** Saved: the stored settings are read again (the editor then starts from them). */
  onSaved: () => Promise<unknown>;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const account = view.account;
  const slot = draftKey("cloud", account.id);
  const hold = useAfkDrafts((s) => s.hold);
  const baseline = useMemo(() => toDraft(view), [view]);
  // Edits left when the panel last closed come back over what is stored now.
  const [draft, setDraftState] = useState<Draft>(() => {
    const held = useAfkDrafts.getState().held[slot] as HeldDraft<Draft> | undefined;
    return held ? rebaseDraft(held.draft, held.base, baseline) : baseline;
  });
  // Where each command row sits: as saved, or where it was added in this visit (until it is saved).
  const [added, setAdded] = useState<ReadonlyMap<string, CommandPlace>>(new Map());
  const saved = useMemo(
    () => new Map(baseline.config.commands.map((row) => [row.id, row])),
    [baseline],
  );
  const placeOf = useCallback((row: Command) => commandPlace(row, saved, added), [saved, added]);
  const rows: RowPlaces = {
    placeOf,
    onAdded: (id, place) => setAdded((map) => new Map(map).set(id, place)),
  };
  const [submitted, setSubmitted] = useState(false);
  const [serverIssues, setServerIssues] = useState<Issue[]>([]);
  const [failure, setFailure] = useState<{ message: string; error: unknown } | null>(null);
  const [conflict, setConflict] = useState(false);
  // Which save runs: "start" also starts an account stopped on its password (AFK-CLEAR).
  const [saving, setSaving] = useState<"save" | "start" | null>(null);
  const [dock, setDock] = useState<HTMLElement | null>(null);
  // A save is read back before the editor starts from it: meanwhile nothing is held and the bar's
  // button waits, then the edits give way to the stored settings and the bar slides away.
  const savedRef = useRef(false);
  const [settling, setSettling] = useState(false);
  // Studio's placeholder switch: its checks answer while typing too (the backend checks again).
  const page = useCloudPage().data;
  const features = page?.overview.features ?? null;
  // "ProBot AI ile kalıp kur" in the rules while the service's model can answer.
  const aiPatterns = page?.overview.aiPatterns ?? false;
  const rules = useMemo(
    () => ({
      on: Boolean(features?.placeholders),
      commands: features?.placeholderCommands ?? [],
    }),
    [features],
  );
  // A stop that names a row opens its question, a wrong password the login's; else where edits wait.
  const [open, setOpen] = useState<AccountQuestion | null>(() => {
    const stopped = heldIssue(view, draft);
    if (stopped) return questionOf(stopped, draft, placeOf);
    const login = loginPart(view, draft);
    if (login) return login === "replies" ? "replies" : "login";
    const now = settled(draft, baseline);
    return (
      QUESTIONS.find(
        (q) => questionValue(now, q, placeOf) !== questionValue(baseline, q, placeOf),
      ) ?? null
    );
  });
  // "Kurulumu bitir": the first four questions one a step, while the account needs them.
  const [setup, setSetup] = useState<{ step: AccountStep; direction: 1 | -1 } | null>(null);
  const offerSetup = accountNeedsSetup(baseline.config) && !setupPutAway.has(account.id);

  // Runs when a save comes back as the stored settings.
  useEffect(() => {
    if (!savedRef.current) return;
    savedRef.current = false;
    setDraftState(baseline);
    setAdded(new Map());
    setSubmitted(false);
    setServerIssues([]);
    setFailure(null);
    setConflict(false);
  }, [baseline]);

  const dirty = isDirty(draft, baseline);
  // "Ayarları bir kez kaydet" (old rules): saving is worth it even with nothing changed.
  const savable = dirty || Boolean(account.features?.legacyRules);
  useEffect(() => {
    if (!savedRef.current) hold(slot, dirty ? { draft, base: baseline } : null);
  }, [hold, slot, dirty, draft, baseline]);

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
  // Shown with the rest, never blocking: see heldIssue and passwordIssue.
  const held = heldIssue(view, draft) ?? passwordIssue(view, draft);
  const shown = held ? [...issues, held] : issues;
  const now = settled(draft, baseline);
  const unsaved = new Set(
    QUESTIONS.filter((q) => questionValue(now, q, placeOf) !== questionValue(baseline, q, placeOf)),
  );
  const marked = new Set(shown.map((issue) => questionOf(issue, draft, placeOf)));

  const goTo = (question: AccountQuestion) => {
    setOpen(question);
    requestAnimationFrame(() =>
      document.getElementById(`question-${question}`)?.scrollIntoView({ block: "nearest" }),
    );
  };

  /** Saves; true once the stored settings took it. */
  const save = async (start = false): Promise<boolean> => {
    setSubmitted(true);
    const found = [...localIssues(draft, view.limits, rules), ...issues];
    const first = found[0];
    if (first) {
      if (setup) {
        const step = questionOf(first, draft, placeOf);
        if (step !== "more") setSetup({ step, direction: -1 });
        else {
          setSetup(null);
          goTo(step);
        }
      } else if (!found.some((i) => questionOf(i, draft, placeOf) === open)) {
        goTo(questionOf(first, draft, placeOf));
      }
      return false;
    }
    setSaving(start ? "start" : "save");
    try {
      const result = await cloudApi.save(account.id, toEdit(draft, baseline, view));
      if (result.kind === "saved") {
        savedRef.current = true;
        setSettling(true);
        hold(slot, null);
        storeAccount(queryClient, result.account);
        void queryClient.invalidateQueries({ queryKey: CLOUD_KEY });
        // A running account restarts with what was saved: the toast says so, as the desktop's does.
        let description = isCloudActive(account) ? t("afkSettings.savedReconnecting") : null;
        if (start) {
          // The start clears the stop (the service lifts it on the next start).
          try {
            if (await startOrAsk(queryClient, result.account)) {
              description = t("afkClear.savedStarting");
            }
          } catch (error) {
            afkFailed(error, t("cloudAfk.actions.failed"));
          }
        }
        toast.success(
          t("cloudAfk.settings.saved", { name: result.account.name }),
          description ? { description } : undefined,
        );
        void onSaved().finally(() => {
          savedRef.current = false;
          setSettling(false);
        });
        return true;
      }
      if (result.kind === "invalid") {
        setServerIssues(result.issues);
        const refused = result.issues[0];
        if (refused && !setup) goTo(questionOf(refused, draft, placeOf));
      } else {
        storeAccount(queryClient, result.account);
        setConflict(true);
      }
      return false;
    } catch (error) {
      setFailure({
        message: isFriendly(error) ? error.message : t("cloudAfk.actions.failed"),
        error,
      });
      return false;
    } finally {
      setSaving(null);
    }
  };

  const discard = () => {
    hold(slot, null);
    setDraftState(baseline);
    setAdded(new Map());
    setSubmitted(false);
    setServerIssues([]);
    setFailure(null);
  };

  // Ctrl+S saves, as in the desktop settings.
  const shortcut = useRef(() => {});
  shortcut.current = () => {
    if (savable && saving === null && !settling) void save();
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

  const part: PartProps = {
    draft,
    setDraft,
    view,
    issues: shown,
    features,
    aiPatterns,
    // "Yanlışsa bildir" heard the text saves now: its refusal no longer holds the save.
    clearRefusal: (at) => setServerIssues((list) => withoutRefusal(list, at)),
  };
  // A mark on screen holds the save (a refused password's or a held row's never: see `held`).
  const blocking = issues.length > 0;
  // Stopped on a password the server refused: saving alone left the stop in place, so the main
  // button starts it too ("Yalnız kaydet" beside it, as on a running desktop account).
  const startAfter = stoppedOnPassword(account);
  // A running account restarts with what is saved: the button says so (no sentence in gold).
  const label =
    saving === (startAfter ? "start" : "save")
      ? t("cloudAfk.settings.saving")
      : isCloudActive(account)
        ? t("afkSettings.saveReconnect")
        : startAfter
          ? t("afkClear.saveStart")
          : t("cloudAfk.settings.save");
  const busy = saving !== null;
  const answers = accountAnswers(draft, placeOf, t);

  const bar = (
    <PlusChangeDock
      dock={dock}
      // While the setup's own steps are on screen, its last step saves: the bar waits under it.
      open={(savable || conflict || Boolean(failure) || blocking) && setup === null}
      label={t("afkPanel.barLabel")}
    >
      <div className="min-w-0 flex-1 text-[13px]">
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
          <p role="alert" className="text-pretty text-flare-400">
            {serverChanges(draft, view) && issues.some((i) => i.problem === "reseal")
              ? t("cloudAfk.settings.resealTitle")
              : t("cloudAfk.settings.fixFirst")}
          </p>
        ) : dirty ? (
          <p className="text-pretty text-ash-300">{t("cloudAfk.settings.dirty")}</p>
        ) : null}
      </div>
      {dirty ? (
        <Button variant="ghost" onClick={discard} disabled={busy || settling}>
          {t("cloudAfk.settings.cancel")}
        </Button>
      ) : null}
      {startAfter ? (
        <Button
          variant="secondary"
          onClick={() => void save(false)}
          disabled={!savable || blocking || busy || settling}
        >
          {saving === "save" ? t("cloudAfk.settings.saving") : t("afkPanel.saveOnly")}
        </Button>
      ) : null}
      {/* While the bar says to fix the marked fields first, saving waits (SHEET-UX). */}
      <Button
        variant="primary"
        onClick={() => void save(startAfter)}
        disabled={!savable || blocking || busy || settling}
      >
        {label}
      </Button>
    </PlusChangeDock>
  );

  const putAway = () => {
    setupPutAway.add(account.id);
    setSetup(null);
  };
  const stepIndex = setup ? ACCOUNT_STEPS.indexOf(setup.step) : 0;
  const setupSteps: FrameStep[] = ACCOUNT_STEPS.map((id) => ({
    id,
    question: t(`setup.account.questions.${id}`),
    item: ACCOUNT_ITEMS[id],
    answer: answers[id],
  }));
  const stepTo = (to: number) => {
    const target = ACCOUNT_STEPS[to];
    if (target) setSetup({ step: target, direction: to > stepIndex ? 1 : -1 });
  };
  const lastStep = stepIndex === ACCOUNT_STEPS.length - 1;

  return (
    <motion.div
      layoutScroll
      className="relative min-h-0 flex-1 overflow-y-auto overscroll-contain [scrollbar-gutter:stable]"
    >
      <LayoutGroup>
        <div className="panel-in flex flex-1 flex-col px-6 pt-5">
          {offerSetup ? (
            <div className="-mt-1 mb-2 flex justify-end">
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setSetup({ step: "server", direction: 1 })}
              >
                {t("setup.account.finish")}
              </Button>
            </div>
          ) : null}
          <QuestionList label={t("setup.questionsLabel")}>
            {QUESTIONS.map((question) => (
              <QuestionRow
                key={question}
                id={question}
                item={ACCOUNT_ITEMS[question]}
                question={
                  question === "more" ? t("setup.other") : t(`setup.account.questions.${question}`)
                }
                answer={answers[question]}
                open={open === question}
                onToggle={() => setOpen((current) => (current === question ? null : question))}
                dirty={unsaved.has(question)}
                alert={marked.has(question)}
              >
                <QuestionFields question={question} part={part} rows={rows} />
              </QuestionRow>
            ))}
          </QuestionList>
          <div aria-hidden className="shrink-0" style={{ height: DOCK_HEIGHT + 16 }} />
        </div>
      </LayoutGroup>
      <div ref={setDock} className="sticky bottom-0 z-10 h-0" />
      {bar}
      <SetupDialog
        open={setup !== null}
        title={t("setup.account.title")}
        steps={setupSteps}
        current={stepIndex}
        direction={setup?.direction ?? 1}
        onGo={stepTo}
        onLater={putAway}
        lede={setup ? t(`setup.account.ledes.${setup.step}`) : null}
        onBack={stepIndex > 0 ? () => stepTo(stepIndex - 1) : undefined}
        primary={{
          label: lastStep
            ? saving
              ? t("cloudAfk.settings.saving")
              : t("setup.account.done")
            : t("setup.next"),
          onClick: () => {
            if (!lastStep) {
              stepTo(stepIndex + 1);
              return;
            }
            void save().then((done) => {
              if (done) putAway();
            });
          },
        }}
        note={lastStep && isCloudActive(account) ? t("setup.account.reconnects") : null}
        busy={busy || settling}
        failure={failure?.message ?? (conflict ? t("cloudAfk.settings.conflict") : null)}
      >
        {setup ? <QuestionFields question={setup.step} part={part} rows={rows} /> : null}
      </SetupDialog>
    </motion.div>
  );
}

/** While the account's settings load: the questions' rows. */
function QuestionsSkeleton({ rows }: { rows: number }): ReactNode {
  return (
    <div className="flex-1 px-6 pt-5" aria-hidden>
      <div className="rounded-[var(--radius-panel)] bg-white/[0.03] ring-1 ring-white/7 ring-inset">
        {Array.from({ length: rows }, (_, i) => (
          <div
            // biome-ignore lint/suspicious/noArrayIndexKey: placeholders have no identity
            key={i}
            className="flex items-start gap-3.5 border-t border-white/6 px-[18px] py-[14px] first:border-t-0"
          >
            <Skeleton className="size-[40px] shrink-0" />
            <div className="flex-1 space-y-2 pt-1">
              <Skeleton className="h-4 w-40" />
              <Skeleton className="h-3.5 w-[60%]" />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

/** The settings panel of one cloud account. */
export function CloudSettingsSheet({
  account,
  twin = false,
  onClose,
}: {
  account: CloudAccount | undefined;
  /** This computer has the same account: the title says these are the cloud's (AFK-CLEAR). */
  twin?: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const open = Boolean(account);
  const close = () => {
    if (account) remindKept(t, draftKey("cloud", account.id), account.name);
    onClose();
  };
  // Bumped once the stored settings were read again (after a save or a conflict).
  const [generation, setGeneration] = useState(0);
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
  const reload = () => {
    void refetch().then(() => setGeneration((n) => n + 1));
  };

  return (
    <SettingsPanel
      open={open}
      onClose={close}
      nickname={account?.name ?? ""}
      title={
        twin
          ? t("afkClear.twin.cloudTitle", { name: account?.name ?? "" })
          : t("cloudAfk.settings.title", { name: account?.name ?? "" })
      }
      server={account ? serverLabel(account.server) : ""}
    >
      {account && view ? (
        <Editor
          key={`${account.id}-${generation}`}
          view={view}
          onReload={reload}
          onSaved={() => refetch()}
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
            <Button variant="secondary" onClick={() => void refetch()} disabled={isFetching}>
              {t("common.retry")}
            </Button>
          </div>
        </div>
      ) : (
        <QuestionsSkeleton rows={QUESTIONS.length} />
      )}
    </SettingsPanel>
  );
}
