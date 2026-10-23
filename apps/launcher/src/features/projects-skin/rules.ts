// SKIN1's rules, pure (rules.test.ts): what the Görünüm panel of a username account shows in
// each state (skin1.md §13.2), the code box's digits, the premium name field, the words a
// refusal or a job's result gets (§12), when ProBot is asked about the link again and which toast
// a closed job brings. The components only draw what these say.
import { checkOfflineName, type NameProblem } from "@/features/accounts/name-rules";
import type {
  ProjectsLink,
  ProjectsSkinChange,
  ProjectsSkinJob,
  ProjectsSkinRefusal,
  ProjectsSkinResult,
  ProjectsSkinView,
} from "@/lib/api/projects-skin-types";

/** The code `/discord eşle` gives in game. */
export const PIN_LENGTH = 6;
/** The command players type in game: the same in both languages (it is the real command). */
export const LINK_COMMAND = "/discord eşle";
/** The command that frees a game account linked to another Discord account. */
export const UNLINK_COMMAND = "/discord eşlemekaldır";
/** A typing pause before a premium name is looked up. */
export const LOOKUP_DELAY_MS = 450;
/** "Gönderiliyor" replaces "Kaydet" only when sending takes longer than this. */
export const SENDING_REVEAL_MS = 150;
/** After this long in line the panel says it takes a while, and the chip's dot rests. */
export const LONG_WAIT_MS = 60_000;
/** The link row offers "Projects skinini ayarla" this long after linking. */
export const SET_SKIN_OFFER_MS = 6000;
/** ProBot is asked about the link again when the service learned it longer ago (seconds). */
export const CHECK_AFTER_SEC = 86_400;

/** The digits of what was typed or pasted, at most six. */
export function pinDigits(raw: string): string {
  return raw.replace(/\D/g, "").slice(0, PIN_LENGTH);
}

/** The link needs asking: not known, never learned, or learned a day ago or more. */
export function needsCheck(link: ProjectsLink | null | undefined, nowSec: number): boolean {
  if (!link) return false;
  if (link.state === "unknown" || link.checkedAt === null) return true;
  return nowSec - link.checkedAt >= CHECK_AFTER_SEC;
}

const sameName = (a: string | null | undefined, b: string | null | undefined) =>
  Boolean(a && b && a.toLowerCase() === b.toLowerCase());

/** The Discord account is linked to this game account's name (ignoring case). */
export function linkedTo(link: ProjectsLink | null | undefined, name: string): boolean {
  return link?.state === "linked" && sameName(link.name, name);
}

/** A premium name's problem as the player types it (the account name's rule and words). */
export function premiumNameProblem(raw: string, final = false): NameProblem | null {
  return checkOfflineName(raw, [], { final }).problem;
}

/** A name worth asking Mojang about. */
export function lookupReady(raw: string): boolean {
  const name = raw.trim();
  return name.length >= 3 && premiumNameProblem(name, true) === null;
}

/** "{{count}} dakika" or "{{count}} saat" for a wait in seconds (at least a minute). */
export function waitWords(seconds: number): { unit: "minutes" | "hours"; count: number } {
  const minutes = Math.max(1, Math.ceil(seconds / 60));
  if (minutes < 60) return { unit: "minutes", count: minutes };
  return { unit: "hours", count: Math.ceil(minutes / 60) };
}

/** Whole days until `until` (at least one), for "{{count}} gün sonra açılır". */
export function daysLeft(untilSec: number, nowSec: number): number {
  return Math.max(1, Math.ceil((untilSec - nowSec) / 86_400));
}

/** A line the panel says: a key under `projectsSkin.note`, its tone and what it names. */
export interface Note {
  key: NoteKey;
  /** `error` is said politely and in flare; `info` quietly. */
  tone: "error" | "info";
  count?: number;
  name?: string;
  /** Unix seconds; the component says it as a date. */
  until?: number;
  /** `tries`: the wait, in words. */
  wait?: { unit: "minutes" | "hours"; count: number };
}

export type NoteKey =
  | "unreachable"
  | "session"
  | "off"
  | "linkFirst"
  | "otherAccount"
  | "quota"
  | "lastChange"
  | "limited"
  | "blocked"
  | "blockedForever"
  | "serverAway"
  | "longWait"
  | "pending"
  | "saveFailed"
  | "invalidImage"
  | "busy"
  | "premiumInvalid"
  | "premiumDisabled"
  | "pinInvalid"
  | "pinExpired"
  | "tries"
  | "paused"
  | "taken"
  | "linkFailed"
  | "restricted";

const blockNote = (until: number | null | undefined): Note =>
  until === null || until === undefined
    ? { key: "blockedForever", tone: "info" }
    : { key: "blocked", tone: "info", until };

/** What a refusal of a skin change says (before any job). */
export function refusalNote(refusal: ProjectsSkinRefusal, nowSec: number): Note {
  switch (refusal.code) {
    case "off":
      return { key: "off", tone: "info" };
    case "session":
      return { key: "session", tone: "error" };
    case "limited":
      return {
        key: "limited",
        tone: "info",
        count: daysLeft(refusal.until ?? nowSec, nowSec),
      };
    case "blocked":
      return blockNote(refusal.until);
    case "restricted":
      return { key: "restricted", tone: "error" };
    case "pending":
      return { key: "pending", tone: "info" };
    case "notLinked":
      return { key: "linkFirst", tone: "info" };
    case "otherAccount":
      return { key: "otherAccount", tone: "info", name: refusal.name ?? "" };
    case "quota":
      return { key: "quota", tone: "info" };
    case "tries":
      return { key: "tries", tone: "error", wait: waitWords(refusal.retryAfter ?? 60) };
    case "invalidPin":
      return { key: "pinInvalid", tone: "error" };
    case "choiceGone":
    case "invalidSkin":
      return { key: "premiumInvalid", tone: "error" };
    case "alreadyLinked":
    case "unreachable":
      return { key: "unreachable", tone: "error" };
  }
}

/** What a closed skin job's result says in the panel; null when it says nothing. */
export function skinResultNote(result: ProjectsSkinResult | null): Note | null {
  switch (result?.code) {
    case undefined:
    case "applied":
    case "superseded":
    case "cancelled":
      return null;
    case "notLinked":
      return { key: "linkFirst", tone: "info" };
    case "otherAccount":
      return { key: "otherAccount", tone: "info", name: result.name ?? "" };
    case "skinBlocked":
      return blockNote(result.until);
    case "premiumInvalid":
      return { key: "premiumInvalid", tone: "error" };
    case "premiumDisabled":
      return { key: "premiumDisabled", tone: "error" };
    case "invalidImage":
      return { key: "invalidImage", tone: "error" };
    case "mineskinBusy":
      return { key: "busy", tone: "error" };
    default:
      return { key: "saveFailed", tone: "error" };
  }
}

/** How a link job (or its refusal) ended for the code box. */
export type LinkOutcome =
  | { kind: "linked"; name: string }
  | { kind: "other"; name: string }
  | { kind: "error"; note: Note };

export function linkResultOutcome(job: ProjectsSkinJob): LinkOutcome {
  const result = job.result;
  switch (result?.code) {
    case "linked":
      return { kind: "linked", name: job.name ?? "" };
    case "otherAccount":
      return { kind: "other", name: result.name ?? "" };
    case "pinInvalid":
      return { kind: "error", note: { key: "pinInvalid", tone: "error" } };
    case "pinExpired":
      return { kind: "error", note: { key: "pinExpired", tone: "error" } };
    case "paused":
      return { kind: "error", note: { key: "paused", tone: "error" } };
    case "minecraftTaken":
      return { kind: "error", note: { key: "taken", tone: "error" } };
    default:
      return { kind: "error", note: { key: "linkFailed", tone: "error" } };
  }
}

export function linkRefusalOutcome(refusal: ProjectsSkinRefusal, nowSec: number): LinkOutcome {
  if (refusal.code === "alreadyLinked") return { kind: "linked", name: refusal.name ?? "" };
  if (refusal.code === "otherAccount") return { kind: "other", name: refusal.name ?? "" };
  if (refusal.code === "pending") return { kind: "error", note: { key: "paused", tone: "error" } };
  return { kind: "error", note: refusalNote(refusal, nowSec) };
}

// ── The panel ────────────────────────────────────────────────────────────────────────────────

/** The link part of the panel. */
export type LinkArea =
  /** "Discord eşlemesi · Ahmet"; dimmed while the service cannot be reached. */
  | { kind: "linked"; name: string; dim: boolean }
  /** The code box ("Hesabını eşle"); dimmed while the service cannot be reached. */
  | { kind: "code"; dim: boolean }
  /** "Discord hesabın Mehmet hesabına eşli." + "Bu hesabı eşle". */
  | { kind: "other"; name: string; dim: boolean }
  /** A skeleton row while ProBot is asked. */
  | { kind: "checking" }
  /** Nothing known and nobody to ask right now. */
  | { kind: "none" };

export type ChipKind = "projects" | "preview" | "inLine";

export interface PanelInput {
  view: ProjectsSkinView;
  /** Signed in to friends (the friends panel's word; the view may lag behind it). */
  signedIn: boolean;
  /** A picked skin is shown and not saved. */
  draft: boolean;
  /** The save request is on its way. */
  sending: boolean;
  /** A note from the last attempt (a refusal or a failed job), said until the next change. */
  failure: Note | null;
  /** The open job has waited `LONG_WAIT_MS`. */
  waitedLong: boolean;
  nowSec: number;
}

export interface PanelState {
  /** What fills the editor column: the Discord card, the "off" line, or the editor. */
  body: "connect" | "session" | "off" | "editor";
  link: LinkArea;
  /** `locked` while a change is on its way; `dim` while the player may not change skins. */
  picker: "open" | "locked" | "dim";
  /** What the big preview shows. */
  preview: "job" | "draft" | "projects" | "default";
  chip: ChipKind | null;
  /** Hidden while a job is in line (it comes back when the job closes). */
  saveShown: boolean;
  saveEnabled: boolean;
  /** "Vazgeç" beside it: a picked skin may be dropped. */
  revertShown: boolean;
  /** "Varsayılana dön": only while a Projects skin is set. */
  resetShown: boolean;
  resetEnabled: boolean;
  /** The footer's line. */
  note: Note | null;
}

export function panelState(input: PanelInput): PanelState {
  const { view, draft, sending, failure, waitedLong, nowSec } = input;
  const signedIn = input.signedIn && view.access !== "signedOut";
  const editor = signedIn && view.access !== "session" && view.access !== "off";
  const unreachable = view.access === "unreachable";
  const job = view.job;
  const busy = Boolean(job) || sending;
  const mine = linkedTo(view.link, view.name);
  const limited = view.limited && view.limited.until > nowSec ? view.limited : null;
  const blocked =
    view.blocked && (view.blocked.until === null || view.blocked.until > nowSec)
      ? view.blocked
      : null;
  const quota = view.limits.left <= 0;

  const link: LinkArea =
    view.link.state === "unknown"
      ? !unreachable
        ? { kind: "checking" }
        : sameName(view.link.name, view.name)
          ? { kind: "linked", name: view.link.name ?? view.name, dim: true }
          : { kind: "none" }
      : view.link.state === "none"
        ? { kind: "code", dim: unreachable }
        : mine
          ? { kind: "linked", name: view.link.name ?? view.name, dim: unreachable }
          : { kind: "other", name: view.link.name ?? "", dim: unreachable };

  // The player may change skins now (whatever is picked).
  const mayChange = editor && !unreachable && mine && !limited && !blocked && !quota && !busy;

  const note: Note | null = !editor
    ? null
    : (failure ??
      (unreachable
        ? { key: "unreachable", tone: "error" }
        : job
          ? waitedLong
            ? { key: "longWait", tone: "info" }
            : view.serverAway
              ? { key: "serverAway", tone: "info" }
              : null
          : blocked
            ? blockNote(blocked.until)
            : limited
              ? { key: "limited", tone: "info", count: daysLeft(limited.until, nowSec) }
              : link.kind === "code"
                ? { key: "linkFirst", tone: "info" }
                : !mine
                  ? null
                  : quota
                    ? { key: "quota", tone: "info" }
                    : view.serverAway
                      ? { key: "serverAway", tone: "info" }
                      : view.limits.left === 1
                        ? { key: "lastChange", tone: "info", count: 1 }
                        : null));

  const skinShown = view.skin && (view.skin.kind === "default" || view.skin.picture);
  // Without the editor nothing on Projects is known: the account's default look.
  const preview: PanelState["preview"] = !editor
    ? "default"
    : job
      ? "job"
      : draft
        ? "draft"
        : view.skin?.picture
          ? "projects"
          : "default";

  return {
    body: !signedIn
      ? "connect"
      : view.access === "session"
        ? "session"
        : view.access === "off"
          ? "off"
          : "editor",
    link,
    picker: !editor ? "open" : busy ? "locked" : limited || blocked ? "dim" : "open",
    preview,
    chip: !editor
      ? null
      : job
        ? "inLine"
        : draft
          ? "preview"
          : skinShown && mine
            ? "projects"
            : null,
    saveShown: editor && !job,
    saveEnabled: mayChange && draft,
    revertShown: editor && draft && !busy,
    resetShown: editor && mine && Boolean(view.skin && view.skin.kind !== "default"),
    resetEnabled: mayChange,
    note,
  };
}

// ── Closed jobs: toasts ──────────────────────────────────────────────────────────────────────

/** Where the player is looking when a change arrives. */
export interface ToastContext {
  /** The account whose Görünüm panel is open, if any. */
  panelFor: string | null;
  /** A code box is on screen (it tells its own result). */
  codeBox: boolean;
}

export type ToastPlan =
  | {
      kind: "saved";
      /** "Varsayılana döndü" rather than "kaydedildi". */
      backToDefault: boolean;
      /** "Yeniden girince görünür." rather than "Projects'e girince görünür.". */
      rejoin: boolean;
      /** "Geri al" for this account. */
      undoFor: string | null;
    }
  | { kind: "skinFailed"; note: Note; accountId: string | null }
  | { kind: "linked"; name: string }
  | { kind: "linkFailed"; note: Note }
  | { kind: "unlinked" };

/** The toast a change brings, or null (the panel or the box tells it, or nothing to tell). */
export function toastFor(change: ProjectsSkinChange, context: ToastContext): ToastPlan | null {
  if (change.kind === "linked") return { kind: "linked", name: change.name };
  if (change.kind !== "job") return null;
  const { job } = change;
  if (job.kind === "skin") {
    if (job.result?.code === "applied") {
      return {
        kind: "saved",
        backToDefault: job.source === "default",
        rejoin: change.onProjects,
        undoFor: change.undo ? change.accountId : null,
      };
    }
    const note = skinResultNote(job.result);
    if (!note) return null;
    // The open panel says it in place.
    if (change.accountId && context.panelFor === change.accountId) return null;
    return { kind: "skinFailed", note, accountId: change.accountId };
  }
  if (job.kind === "unlink") return job.result?.code === "unlinked" ? { kind: "unlinked" } : null;
  if (job.kind === "link") {
    if (context.codeBox) return null;
    const outcome = linkResultOutcome(job);
    if (outcome.kind === "linked") return { kind: "linked", name: outcome.name };
    if (outcome.kind === "error") return { kind: "linkFailed", note: outcome.note };
    return null;
  }
  return null;
}

/**
 * A failed skin job of the last day the player has not been told about (its result waited in
 * the service while the launcher was closed): told once when the panel opens.
 */
export function untoldFailure(
  lastJob: ProjectsSkinJob | null,
  told: ReadonlySet<string>,
  nowMs: number,
): Note | null {
  if (lastJob?.kind !== "skin" || told.has(lastJob.id)) return null;
  if (nowMs - lastJob.updatedAtMs > 86_400_000) return null;
  return skinResultNote(lastJob.result);
}
