// A cloud account's settings in the shared settings panel (SHEET-UX), as the desktop AFK settings:
// Bağlantı, Komutlar, Oyun içi and Otomatik yanıt in the rail, problems where they are, and the
// change bar docked under the section while something is unsaved. Secret rows show their words
// when this computer sealed them (the keychain copy), else "Göster" asks the account's machine.
// Saving restarts a running account with the new settings; a server change seals the secrets this
// computer has again and asks for the others. A press outside, Esc or the close button closes the
// panel at any time; the edits wait as a draft.
import {
  ChatCircleText,
  Copy,
  Globe,
  type Icon,
  Keyboard,
  PersonSimpleWalk,
} from "@phosphor-icons/react";
import { Button, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { copyErrorDetails } from "@/features/content/jobs";
import { isFriendly } from "../errors";
import { SectionTitle } from "../settings/controls";
import { draftKey, type HeldDraft, rebaseDraft, useAfkDrafts } from "../settings/drafts";
import { PanelBody, PanelSkeleton, SettingsPanel } from "../settings/panel";
import { type CloudAccount, type CloudEditData, cloudApi } from "./api";
import { PlusChangeDock } from "./plus/change-dock";
import {
  type Draft,
  heldIssue,
  type Issue,
  isDirty,
  localIssues,
  PARTS,
  type Part,
  partOf,
  serverChanges,
  toDraft,
  toEdit,
  unsavedParts,
} from "./settings-model";
import {
  CommandsPart,
  ConnectionPart,
  commandLimit,
  GamePart,
  RepliesPart,
  ruleLimit,
} from "./settings-sections";
import { isCloudActive, serverLabel } from "./status";
import { CLOUD_KEY, storeAccount, useCloudPage } from "./use-cloud";

const ICONS: Record<Part, Icon> = {
  connection: Globe,
  commands: Keyboard,
  game: PersonSimpleWalk,
  replies: ChatCircleText,
};

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
  // A stop that names a row opens the panel on it; else where edits wait; else Bağlantı.
  const [part, setPart] = useState<Part>(() => {
    const stopped = heldIssue(view, draft);
    if (stopped) return partOf(stopped);
    return PARTS.find((p) => unsavedParts(draft, baseline).has(p)) ?? "connection";
  });
  const [submitted, setSubmitted] = useState(false);
  const [serverIssues, setServerIssues] = useState<Issue[]>([]);
  const [failure, setFailure] = useState<{ message: string; error: unknown } | null>(null);
  const [conflict, setConflict] = useState(false);
  const [saving, setSaving] = useState(false);
  const [dock, setDock] = useState<HTMLElement | null>(null);
  const scroller = useRef<HTMLDivElement | null>(null);
  // A save is read back before the editor starts from it: meanwhile nothing is held and the bar's
  // button waits, then the edits give way to the stored settings and the bar slides away (the
  // editor stays: no section rises in again).
  const saved = useRef(false);
  const [settling, setSettling] = useState(false);
  // Studio's placeholder switch: its checks answer while typing too (the backend checks again).
  const features = useCloudPage().data?.overview.features ?? null;
  const rules = useMemo(
    () => ({
      on: Boolean(features?.placeholders),
      commands: features?.placeholderCommands ?? [],
    }),
    [features],
  );

  // Runs when a save comes back as the stored settings.
  useEffect(() => {
    if (!saved.current) return;
    saved.current = false;
    setDraftState(baseline);
    setSubmitted(false);
    setServerIssues([]);
    setFailure(null);
    setConflict(false);
  }, [baseline]);

  const dirty = isDirty(draft, baseline);
  // "Ayarları bir kez kaydet" (old rules): saving is worth it even with nothing changed.
  const savable = dirty || Boolean(account.features?.legacyRules);
  useEffect(() => {
    if (!saved.current) hold(slot, dirty ? { draft, base: baseline } : null);
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
  // Shown with the rest, never blocking: see heldIssue.
  const held = heldIssue(view, draft);
  const shown = held ? [...issues, held] : issues;
  const unsaved = unsavedParts(draft, baseline);

  const goTo = (next: Part) => {
    setPart(next);
    scroller.current?.scrollTo({ top: 0 });
  };

  const save = async () => {
    setSubmitted(true);
    const found = [...localIssues(draft, view.limits, rules), ...issues];
    if (found.length) {
      const first = found[0];
      if (first && !found.some((i) => partOf(i) === part)) goTo(partOf(first));
      return;
    }
    setSaving(true);
    try {
      const result = await cloudApi.save(account.id, toEdit(draft, baseline, view));
      if (result.kind === "saved") {
        saved.current = true;
        setSettling(true);
        hold(slot, null);
        storeAccount(queryClient, result.account);
        void queryClient.invalidateQueries({ queryKey: CLOUD_KEY });
        toast.success(t("cloudAfk.settings.saved", { name: result.account.name }));
        void onSaved().finally(() => {
          saved.current = false;
          setSettling(false);
        });
      } else if (result.kind === "invalid") {
        setServerIssues(result.issues);
        const first = result.issues[0];
        if (first) goTo(partOf(first));
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

  const discard = () => {
    hold(slot, null);
    setDraftState(baseline);
    setSubmitted(false);
    setServerIssues([]);
    setFailure(null);
  };

  // Ctrl+S saves, as in the desktop settings.
  const shortcut = useRef(() => {});
  shortcut.current = () => {
    if (savable && !saving && !settling) void save();
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

  const props = { draft, setDraft, view, issues: shown, features };
  const blocking = submitted && issues.length > 0;
  // A running account restarts with what is saved: the button says so (no sentence in gold).
  const label = saving
    ? t("cloudAfk.settings.saving")
    : isCloudActive(account)
      ? t("afkSettings.saveReconnect")
      : t("cloudAfk.settings.save");
  const count =
    part === "commands"
      ? t("cloudAfk.settings.commands.count", {
          count: draft.config.commands.length,
          max: commandLimit(view),
        })
      : part === "replies"
        ? t("cloudAfk.settings.replies.count", {
            count: draft.config.rules.length,
            max: ruleLimit(view),
          })
        : null;

  const bar = (
    <PlusChangeDock
      dock={dock}
      open={savable || conflict || Boolean(failure) || blocking}
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
          <p role="alert" className="text-flare-400">
            {serverChanges(draft, view) && issues.some((i) => i.problem === "reseal")
              ? t("cloudAfk.settings.resealTitle")
              : t("cloudAfk.settings.fixFirst")}
          </p>
        ) : dirty ? (
          <p className="text-ash-300">{t("cloudAfk.settings.dirty")}</p>
        ) : null}
      </div>
      {dirty ? (
        <Button variant="ghost" onClick={discard} disabled={saving || settling}>
          {t("cloudAfk.settings.cancel")}
        </Button>
      ) : null}
      <Button
        variant="primary"
        onClick={() => void save()}
        disabled={!savable || saving || settling}
      >
        {label}
      </Button>
    </PlusChangeDock>
  );

  return (
    <PanelBody
      label={t("cloudAfk.settings.title", { name: account.name })}
      section={part}
      onSection={goTo}
      scrollerRef={(element) => {
        scroller.current = element;
      }}
      dockRef={setDock}
      bar={bar}
      sections={PARTS.map((id) => ({
        id,
        label: t(`afkPanel.sections.${id}`),
        icon: ICONS[id],
        unsaved: unsaved.has(id),
        problem: shown.some((i) => partOf(i) === id),
      }))}
    >
      <SectionTitle
        title={t(`afkPanel.sections.${part}`)}
        aside={
          count ? <span className="text-[12.5px] text-ash-400 tabular-nums">{count}</span> : null
        }
      />
      {part === "connection" ? (
        <ConnectionPart {...props} />
      ) : part === "commands" ? (
        <CommandsPart {...props} />
      ) : part === "game" ? (
        <GamePart {...props} />
      ) : (
        <RepliesPart {...props} />
      )}
    </PanelBody>
  );
}

/** The settings panel of one cloud account. */
export function CloudSettingsSheet({
  account,
  onClose,
}: {
  account: CloudAccount | undefined;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const open = Boolean(account);
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
      onClose={onClose}
      nickname={account?.name ?? ""}
      title={t("cloudAfk.settings.title", { name: account?.name ?? "" })}
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
        <PanelSkeleton rows={PARTS.length} />
      )}
    </SettingsPanel>
  );
}
