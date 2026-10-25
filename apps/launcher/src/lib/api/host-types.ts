// "Dünyanı arkadaşlarına aç" (HOST1, docs: host1.md, crates/pl-host/PROTOCOL.md): UI-facing shapes
// of world sharing. The Tauri adapter (host-tauri.ts) maps HOST1-L1's commands onto them; the mock
// (host-mock.ts) fakes every state for browser previews. Nothing here carries a path, a relay, an
// address, a port the player sees, a key or a code: the interface never shows them (host1.md §1.1).

/** The game mode guests play in (the world's own mode is picked first). */
export type WorldGameMode = "survival" | "creative" | "adventure";

/**
 * Whether an instance's worlds offer "Arkadaşlarınla oyna" (host1.md §1.3): the signed content and
 * the service offer worlds, the player is signed in to friends, the instance follows the official
 * pack on 26.1.2, 26.2 or 26.3, its overlay mod knows hosting and no e4mc or e4all jar is in it.
 * The overlay switch is not part of it: with the switch off the button still shows and its window
 * says the overlay must be on, with "Aç" (the UI reads the switch itself).
 */
export interface HostPlace {
  shown: boolean;
  /**
   * The game runs this instance with this world (its folder in `saves/`) open now and the overlay
   * is connected: the world opens in place, nothing restarts.
   */
  playing?: string;
}

/**
 * A friend in the player's world session: `pending` while the opening has not sent its invites
 * yet, `invited` once it did, `joining` on their way in, `in` in the world.
 */
export type GuestState = "pending" | "invited" | "joining" | "in";

export interface WorldGuest {
  /** The friend's id on the friends service. */
  friend: string;
  /** Their shown name and picture (the friends service's). */
  name: string;
  avatar?: string;
  avatarUrl?: string;
  /** The in-game name they came with ("Notch adıyla"), once they reached the world. */
  gameName?: string;
  state: GuestState;
}

/** Why the player's world did not open: today's opens are used up, or no way works now. */
export type OpenFailure = "limit" | "unavailable";

/** Where an opening from the launcher is (host1.md §1.3, PROTOCOL §3.1). */
export type OpeningStep =
  /** The world's last backup is older than a day: one is made first (labelled for this). */
  | "backup"
  /** The game starts into the world (OYNA shows the launch itself). */
  | "starting"
  /** The world loaded: it opens on this computer only, a way is prepared, the invites go out. */
  | "publishing";

interface SessionBase {
  instanceId: string;
  /** The world's folder in `saves/` and its name. */
  folder: string;
  worldName: string;
  /** The friends invited (or to be invited), in the order they were picked. */
  guests: WorldGuest[];
  /** How many guests may be in at once (Studio, 1–7; the host not counted). */
  max: number;
}

export type HostSession =
  | (SessionBase & {
      phase: "opening";
      step: OpeningStep;
      /** The backup's progress (0–100) while `step` is "backup". */
      percent?: number;
    })
  | (SessionBase & {
      phase: "open";
      mode: WorldGameMode;
      /** "Arkadaşların komut kullanabilsin". */
      commands: boolean;
    })
  /** It did not open; kept until "Tekrar dene", "Projects'te buluşun" or "Kapat". */
  | (SessionBase & { phase: "failed"; reason: OpenFailure });

/**
 * Where a "Katıl" on a world invite is (host1.md §1.4, §1.8; PROTOCOL §3.2, §10.2). The rows show
 * it in place; a window opens only for a decision (`ask`).
 */
export type JoinStage =
  /** The way, the world's details and the install it needs are prepared (`percent` while files
   * download). */
  | { kind: "preparing"; percent?: number }
  /** The player must decide (a window asks). */
  | { kind: "ask"; ask: JoinAsk }
  /**
   * Ready: the game starts with `instanceId` straight into the world through the launcher's own
   * way to it on this computer (`port`), or moves there in game. `missing`: mods of the world that
   * could not be found (joining is tried anyway).
   */
  | { kind: "ready"; instanceId: string; port: number; missing: number }
  /** The game reached the way: it connects. */
  | { kind: "connecting" }
  /** In the world. */
  | { kind: "in" }
  /** The way broke and did not come back within 15 s: "Yeniden katıl" (the same invite). */
  | { kind: "lost" }
  | { kind: "failed"; failure: JoinFailure };

export type JoinAsk =
  /**
   * The world needs both-side mods no install of the player's has: a new install with exactly
   * them is made ("Kopyala ve katıl"); the player's own installs never change. `bytes`: the
   * download; `unknown`: mods that could not be found. A copy made for this friend before that
   * differs now is set up again without a question (PROTOCOL §10.2 `update-copy`: "Hazırlanıyor").
   */
  | { kind: "copy"; mods: number; bytes: number; unknown: number }
  /** No install of the world's Minecraft version (`mc`): the official pack is installed for it. */
  | { kind: "install"; mc: string }
  /**
   * The account's in-game name cannot come in: `own` when it is the host's own (or not a valid
   * name), `taken` when it is bound to another friend in this world. Another account may join.
   */
  | { kind: "name"; verdict: "own" | "taken"; name: string };

export type JoinAnswer =
  /** `name`: the new install's name in the player's language ("Arkadaş dünyası: Ayşe"). */
  | { kind: "copy"; name: string }
  | { kind: "install" }
  /** Join with another of the player's accounts (it becomes the active account). */
  | { kind: "account"; accountId: string };

/** How a "Katıl" ended (PROTOCOL §6.9, §10.2). */
export type JoinFailure =
  /** "Dünya dolu." */
  | "full"
  /** The host closed the world. */
  | "closed"
  /** The host removed the player (or a block or a restriction did). */
  | "removed"
  /** No way worked, the host's side was busy, or something unexpected. */
  | "unreachable"
  /** The host's pack is older than the one the player's content names. */
  | "hostPackOld"
  /** After joining, the game's log names mods the player lacks ("Kopyala ve katıl"). */
  | "mods";

/** A friend's world invite: only invitees ever learn that the world is open (host1.md §1.5). */
export interface WorldInvite {
  /** The world session; one invite per session. */
  session: string;
  /** The friend who opened their world. */
  from: string;
  name: string;
  avatar?: string;
  avatarUrl?: string;
  /** Their Minecraft version, shown small under the invite. */
  mc: string;
  /** Guests in the world now (the host not counted) and the guest limit: shown as "2/8". */
  guests: number;
  max: number;
  /** Unix seconds. */
  at: number;
  /** It came while the player was on do-not-disturb: listed quietly, never a toast. */
  quiet: boolean;
  /** "Katıl" under way, or how it ended; absent until it is pressed. */
  join?: JoinStage;
}

export interface HostView {
  /** Grows with every view; older ones are ignored. */
  seq: number;
  /** World sharing runs (signed content, service, signed in to friends); false: nothing shows. */
  enabled: boolean;
  /** Guest limit of a new session (Studio, 1–7). */
  maxGuests: number;
  /** The player's own world session, or none. */
  session: HostSession | null;
  /** Friends' world invites, newest first. */
  invites: WorldInvite[];
}

export const HIDDEN_HOST_VIEW: HostView = {
  seq: -1,
  enabled: false,
  maxGuests: 7,
  session: null,
  invites: [],
};

/** Why invites did not go out: today's are used up, the session's live ones (20), or a limited
 * account's daily ones (10). */
export type InviteRefusal = "daily" | "session" | "limited";

/** Moments worth a toast or a notification. */
export type HostNotice =
  /** A world invite arrived live (a quiet one never comes as a notice). */
  | { kind: "invite"; invite: WorldInvite }
  /** The player's world opened. */
  | { kind: "opened"; worldName: string }
  /** It did not open. */
  | { kind: "openFailed"; reason: OpenFailure }
  /** The invites an opening was to send did not go. */
  | { kind: "inviteRefused"; why: InviteRefusal }
  /** A "Katıl" ended (`name`: the friend whose world it was). */
  | { kind: "joinFailed"; session: string; name: string; failure: JoinFailure };

/** "Dünyayı aç" from the launcher. */
export interface OpenRequest {
  instanceId: string;
  folder: string;
  mode: WorldGameMode;
  commands: boolean;
  /** Friend ids, at most the guest limit. */
  invite: string[];
}

export interface HostApi {
  state(): Promise<HostView>;
  /** Every change, as a full view. Returns an unsubscribe function. */
  onChange(listener: (view: HostView) => void): () => void;
  /** Moments worth a toast or a notification. Returns an unsubscribe function. */
  onNotice(listener: (notice: HostNotice) => void): () => void;
  /** Whether an instance's worlds offer "Arkadaşlarınla oyna". */
  place(instanceId: string): Promise<HostPlace>;
  /**
   * "Dünyayı aç": records the opening (world, mode, commands, the friends to invite) and makes the
   * backup a world needs first (its last one older than a day). Answers "start" when the game
   * should start into the world now (the caller launches it with quick play), or "running" when
   * the game already runs with the world open (it opens in place), or "cancelled" when the opening
   * was left during its backup (`cancelOpen`). The world opens once it has loaded; invites go out
   * once a way is ready. Refusals come as `openFailed`.
   */
  open(request: OpenRequest): Promise<"start" | "running" | "cancelled">;
  /** "Vazgeç" during the backup, or the game did not start after all (cancelled or failed before
   * the world loaded). */
  cancelOpen(): Promise<void>;
  /** "Davet et" while the world is open; a refusal is an answer, not an error. */
  invite(friends: string[]): Promise<"sent" | InviteRefusal>;
  /** "Dünyadan çıkar": the guest leaves at once and needs a new invite to come back. */
  remove(friend: string): Promise<void>;
  /** "Paylaşımı kapat". */
  stop(): Promise<void>;
  /** Clears a failed opening ("Kapat"). */
  dismissFailure(): Promise<void>;
  /** "Projects'te buluşun": these friends get a "Birlikte oyna" invite to projects.gg (the caller
   * takes the player there with "Bağlan"). */
  meetOnProjects(friends: string[]): Promise<void>;
  /**
   * "Katıl" on a world invite, also "Tekrar dene" and "Yeniden katıl". `copy`: a copy of the
   * host's install even where another install would do ("Kopyala ve katıl"), with its name in the
   * player's language.
   */
  join(session: string, options?: { copy?: { name: string } }): Promise<void>;
  /** The answer to a join's question. */
  answer(session: string, answer: JoinAnswer): Promise<void>;
  /** "Vazgeç" (a question, a preparation) or the game that never started. */
  cancelJoin(session: string): Promise<void>;
  /** "Kapat" on a world invite: it leaves every list on this computer. */
  dismissInvite(session: string): Promise<void>;
  /**
   * "Bağlan" in game (FIX2 with the overlay): the overlay moves the running game into the world;
   * the mod shows `label` ("Arkadaş dünyası: Ayşe", in the player's language), never an address.
   * False: it could not.
   */
  connectInGame(session: string, label: string): Promise<boolean>;
}
