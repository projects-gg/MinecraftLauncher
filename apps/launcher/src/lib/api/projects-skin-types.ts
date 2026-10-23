// Projects skins for username accounts and the game account link on Projects (SKIN1). The
// UI-facing shapes and the calls the Görünüm panel, the account center's link row, the palette
// and the toasts make. Implementations: projects-skin-tauri.ts (the app; maps the generated
// bindings, SKIN1-R) and projects-skin-mock.ts (browser previews and tests: every state).
//
// The friends service decides (social.projects.gg `/v1/mc`, see services/social README
// "Minecraft link and Projects skin"); ProBot links and Lobi applies, so a saved skin shows at
// the player's next login on Projects. Times are Unix seconds unless named `…Ms`.
import type { SkinVariant } from "./skins-types";

export type { SkinVariant };

/** The Discord account's link to a game account on Projects. */
export interface ProjectsLink {
  /** `linked` to `name`; `none` (ProBot said there is none); `unknown` (not learned lately). */
  state: "linked" | "none" | "unknown";
  /** The linked game account (the last known one while `unknown`; null for `none`). */
  name: string | null;
  /** When it was linked. */
  at: number | null;
  /** When the service last learned it from ProBot; null: never. A day old or more: check. */
  checkedAt: number | null;
}

/** The service's switches as its last answer listed them (`features`). */
export interface ProjectsSkinService {
  /** `mcLink`: linking from the launcher. */
  link: boolean;
  /** `skins`: Projects skins. */
  skins: boolean;
}

/** What the launcher offers at all. Never waits for the network (the last known state). */
export interface ProjectsSkinOverview {
  /** Signed content has `features.friends` and `features.skins`: the entry points show. */
  shown: boolean;
  /** The service's switches; null while unknown (signed out, or no answer yet). */
  service: ProjectsSkinService | null;
  /** The link as last known; null while signed out or never learned. */
  link: ProjectsLink | null;
}

/** The Projects skin as last changed from a launcher (or reset by staff). */
export interface ProjectsSkin {
  kind: "png" | "premium" | "default";
  variant: SkinVariant;
  /** `premium`: the premium player's name as Mojang writes it. */
  premium: string | null;
  at: number;
  /** `staff`: moderators reset it in game. */
  by: "launcher" | "staff";
  /**
   * The skin as a 64×64 PNG data URL: this computer's copy, or the texture fetched by its id
   * from Mojang's texture host. Null for `default` and when neither is at hand.
   */
  picture: string | null;
}

export type ProjectsSkinJobKind = "link" | "unlink" | "check" | "skin";
export type ProjectsSkinJobState =
  | "pending"
  | "working"
  | "queued"
  | "done"
  | "failed"
  | "cancelled";

/**
 * How a job ended (the service's result codes, camelCase). Codes the launcher does not know
 * arrive as `error`.
 */
export type ProjectsSkinResultCode =
  | "linked"
  | "unlinked"
  | "applied"
  | "notLinked"
  | "otherAccount"
  | "minecraftTaken"
  | "pinInvalid"
  | "pinExpired"
  | "paused"
  | "skinBlocked"
  | "premiumInvalid"
  | "premiumDisabled"
  | "invalidImage"
  | "mineskinBusy"
  | "superseded"
  | "timeout"
  | "cancelled"
  | "error";

export interface ProjectsSkinResult {
  code: ProjectsSkinResultCode;
  /** `otherAccount`: the game account the Discord account is linked to. */
  name?: string;
  /** `skinBlocked`: until when (null: for good). */
  until?: number | null;
}

export interface ProjectsSkinJob {
  /** Never shown. */
  id: string;
  kind: ProjectsSkinJobKind;
  state: ProjectsSkinJobState;
  /** The game account it is for (`link` and `skin`). */
  name: string | null;
  /** `skin`: what it saves. */
  source: "png" | "premium" | "default" | null;
  /** Once closed. */
  result: ProjectsSkinResult | null;
  /** `skin` while open: the skin it saves (64×64 PNG data URL; null for `default`). */
  picture: string | null;
  /** `skin` while open: its arm model. */
  variant: SkinVariant | null;
  createdAtMs: number;
  updatedAtMs: number;
}

export interface ProjectsSkinLimits {
  /** Skin changes a day (Studio, 1–10). */
  perDay: number;
  /** Changes left today (Turkey days; only `applied` counts). */
  left: number;
  /** When today's changes come back (the Turkey day ends). */
  resetAt: number;
  /** Link tries left (the lower of this hour's and today's). */
  linkTriesLeft: number;
  /** After the service turned a link try away as too many: when tries open again. */
  linkRetryAt: number | null;
}

/** Why the panel can do nothing with the service right now. */
export type ProjectsSkinAccess =
  /** Not signed in to friends (Discord): "Projects skini için Discord ile bağlan". */
  | "signedOut"
  /** The service's skin switch is off: "Projects skini şu an kapalı.". */
  | "off"
  /** The service did not answer, or ProBot is away: "Şu an bağlanılamıyor…". */
  | "unreachable"
  /** The Discord session ended (sign in to friends again). */
  | "session"
  | "ready";

/** The Görünüm panel's view of one username account. */
export interface ProjectsSkinView {
  accountId: string;
  /** The game account's name. */
  name: string;
  access: ProjectsSkinAccess;
  /** Lobi did not take ProBot's queue lately: a saved skin waits in line meanwhile. */
  serverAway: boolean;
  link: ProjectsLink;
  /** Null: no Projects skin set from a launcher (or the link moved). */
  skin: ProjectsSkin | null;
  /** The skin job still open (pending, working or queued). */
  job: ProjectsSkinJob | null;
  /** The skin job that closed last (the last day), to tell its result once. */
  lastJob: ProjectsSkinJob | null;
  /** The link family's open job (`link`, `unlink`, `check`). */
  linkJob: ProjectsSkinJob | null;
  limits: ProjectsSkinLimits;
  /** Staff keep the player from changing skins (`until` null: for good). */
  blocked: { until: number | null } | null;
  /** A new Discord account ("Sınırlı hesap") may not change skins until then. */
  limited: { until: number } | null;
  /** "Geri al" can put back the skin before the last change (kept on this computer). */
  canUndo: boolean;
  /** The game this launcher started is on projects.gg right now. */
  onProjects: boolean;
}

/** What an account looks like on Projects without a Projects skin. */
export interface ProjectsSkinLook {
  picture: string;
  variant: SkinVariant;
  /** A premium account of the same name lends its skin (else Minecraft's default skin). */
  premium: boolean;
}

/** A skin the player picked, checked and ready to save. */
export interface ProjectsSkinChoice {
  /** Handed back to `save`; lives in the backend for this run of the launcher. Never shown. */
  id: string;
  kind: "png" | "premium";
  /** 64×64 PNG data URL (a 64×32 file is already converted). */
  picture: string;
  /** `png`: the file's name; `premium`: the player's name as Mojang writes it. */
  name: string;
  /** `premium`: from the texture (fixed). `png`: classic until the UI guesses from the picture. */
  variant: SkinVariant;
  /** `png`: an old 64×32 skin (no slim arms). */
  legacy: boolean;
  /** `png`: more than 5% of the base layer is see-through (may look black in game). */
  blackParts: boolean;
  /** `premium`: the account's own name (its skin is the default already). */
  own: boolean;
}

/** `readFile`: the file as a skin, or why not (§4.1). */
export type ProjectsSkinPick =
  | { status: "ready"; choice: ProjectsSkinChoice }
  | { status: "refused"; reason: "notPng" | "dimensions" | "tooBig" | "empty" | "unreadable" };

/** `lookup`: a premium player's skin, or why not (§4.2). */
export type ProjectsSkinLookup =
  | { status: "found"; choice: ProjectsSkinChoice }
  /** "Bu adda bir Minecraft hesabı yok." */
  | { status: "noAccount" }
  /** The player wears Minecraft's default skin: nothing to keep. */
  | { status: "noSkin" }
  /** Mojang answered 429 or 5xx, or not at all (asked again after 10 minutes at the earliest). */
  | { status: "unreachable" };

/** What `save` sends. */
export type ProjectsSkinSave =
  /** A picked skin; `variant` counts for `png` only (a premium skin keeps its own). */
  | { kind: "choice"; id: string; variant: SkinVariant }
  /** "Varsayılana dön". */
  | { kind: "default" }
  /** "Geri al": the skin before the last change. */
  | { kind: "undo" };

/** Why the service turned a request away before any job. */
export type ProjectsSkinRefusalCode =
  | "off"
  | "unreachable"
  | "session"
  | "limited"
  | "blocked"
  | "restricted"
  | "pending"
  | "notLinked"
  | "otherAccount"
  | "alreadyLinked"
  | "quota"
  | "tries"
  | "invalidPin"
  | "invalidSkin"
  | "choiceGone";

export interface ProjectsSkinRefusal {
  code: ProjectsSkinRefusalCode;
  /** `limited`, `blocked`: until when (null: for good). */
  until?: number | null;
  /** `otherAccount`, `alreadyLinked`: the linked game account. */
  name?: string;
  /** `tries`, `quota`, `unreachable`: seconds until trying again may work. */
  retryAfter?: number;
}

/** `save`, `link`, `unlink`: a job on its way, or a refusal. */
export type ProjectsSkinOutcome =
  | { status: "sent"; job: ProjectsSkinJob }
  | { status: "refused"; refusal: ProjectsSkinRefusal };

/** `onChange`: something the panel, the row or a toast cares about. */
export type ProjectsSkinChange =
  | {
      /** A job of the player's closed. */
      kind: "job";
      job: ProjectsSkinJob;
      /** The launcher account the job was for (by name), if it is still here. */
      accountId: string | null;
      /** "Geri al" can follow an applied skin: the one before is kept and a change is left. */
      undo: boolean;
      /** The game this launcher started is on projects.gg right now. */
      onProjects: boolean;
    }
  /** The Discord account was linked outside a code typed here (in game, codeless, Discord). */
  | { kind: "linked"; name: string }
  /** Anything else (a staff reset, a link removed elsewhere, a refresh): read the state again. */
  | { kind: "state" };

/** `projectsSkinApi`: everything SKIN1's interface asks of the backend. */
export interface ProjectsSkinApi {
  overview(): Promise<ProjectsSkinOverview>;
  /** Asks the service (`GET /v1/mc`) and reads this computer's copy of the account's skin. */
  state(accountId: string): Promise<ProjectsSkinView>;
  /** Mojang's answer for the account's own name (cached 10 minutes), else the default skin. */
  defaultLook(accountId: string): Promise<ProjectsSkinLook>;
  /** The native file picker for a PNG; null when the player closes it. */
  pickFile(): Promise<string | null>;
  /** Reads (at most 1 MB), checks and re-encodes a skin file. */
  readFile(path: string): Promise<ProjectsSkinPick>;
  /** A premium player's skin; the signed texture stays in the backend for `save`. */
  lookup(accountId: string, name: string): Promise<ProjectsSkinLookup>;
  save(accountId: string, save: ProjectsSkinSave): Promise<ProjectsSkinOutcome>;
  /** Cancels a job ProBot has not taken yet; false once it has. */
  cancel(jobId: string): Promise<boolean>;
  /** Links the Discord account to the account's name with the code from `/discord eşle`. */
  link(accountId: string, pin: string): Promise<ProjectsSkinOutcome>;
  unlink(): Promise<ProjectsSkinOutcome>;
  /** Asks ProBot for the link; the answer arrives as a change. */
  linkCheck(): Promise<void>;
  /** Returns an unsubscribe function. */
  onChange(listener: (change: ProjectsSkinChange) => void): () => void;
}
