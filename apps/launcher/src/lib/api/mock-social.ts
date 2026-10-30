// Browser preview of the friends feature: every state the panel can be in, with friends in the
// launcher, on projects.gg, in singleplayer, on another server and offline, requests both ways,
// blocks and invites. Actions change the data like the app would.
//
// Preview switch (read once, from the page URL): `?social=<scenario>`
// - (none) / `friends`: signed in, a lively list, requests and a quiet invite;
// - `off`: the feature is off (nothing shows); `signedOut`: the opt-in screen;
// - `signingIn`: waiting for the browser; `loading`: skeletons; `empty`: no friends yet;
// - `down`: the service is unavailable; `replaced`: another device took over;
// - `invite`: a live invite (to projects.gg) arrives shortly after start, `inviteOther` one to
//   another server (`&inviteAt=<ms>` delays it); `restore`: the "getireyim mi?" offer;
// - `playing` / `playingOther`: the player is on projects.gg / on another server, its address
//   shared ("Birlikte oyna" is offered only then);
// - `groups`: the friend list in the player's categories;
// - `team`: crews on, the player leads a crew (with categories); `teamMember`: a member of one;
//   `teamModerator`: a moderator of one;
// - `teamNew`: crews on, no crew yet, an invite to one waiting; `teamInvite`: one arrives live;
// - `crewCall`: a crewmate's call from projects.gg arrives live (`&callAt=<ms>`);
//   `crewCallOpen`: one is on already, unanswered ("Hazır mısın?"); `crewCallMine`: the player's
//   own call is on, with answers;
// - `crewCallOther`: a crewmate's call from another server is on (CALL1);
// - `crews`: several crews (TEAM4): the player leads one and is in two more, the last muted on its
//   own, an invite to a fourth waiting (`&crewLimit=<n>`, 5 without it); `crewsFull`: at the limit
//   (3); `crewCallMulti`: a call to
//   two of them arrives live (`&callAt=<ms>`); `&multi=1` turns several crews on in any crew
//   scenario;
// - `&at=projects|other|hidden` on any scenario: the player plays on projects.gg, on another
//   server with its address shared, or on one with it hidden ("Ekibi çağır" goes from the first
//   two; CALL1); without it they are in the launcher;
// - `crewOld`: crews on a service from before looks, calls and roles (today's editor and card);
// - `&muted=<minutes>|forever` on any crew scenario: the player muted their crew (`crewMuted`:
//   a member who did, for about two and a half hours); `&noMute=1`: a service with calls but
//   without crew notification settings (the launcher's own switches);
// - `legacy`: a service that offers neither categories nor teams (their menu rows stay away);
// - chat (CHAT1): `chat`, `chatLive`, `chatFailed`, `chatEmpty`, `chatMuted`, `chatOff` and
//   `chatMailbox` (mock-chat.ts; `&chat=1` turns chat on in any scenario);
// - `limited`: a new Discord account ("Sınırlı hesap"; SPAM1) without a crew: its chip, no crew
//   founding, no settings backup, no more code lookups today;
// - `warning` / `profileReset`: a moderators' one-time note waits (MOD1); `skinReset`: staff
//   reset the Projects skin, with a 7-day block (SKIN1); `crewReset`: moderators
//   reset the crew's name, locked for three days; `crewGone`: they disband the crew live
//   (`&liveAt=<ms>`, as `restrictedLive`);
// - `restricted` / `banned`: the sign-in screen of a suspended / banned account, which signing in
//   again does not change; `restrictedLive`: it happens while signed in; `signupsClosed`: no new
//   accounts today;
// - `backupWaits`: storage protection: the settings backup waits;
// - `&reports=limit` on any scenario: "Rapor et" says the day's reports are used up;
// - the crews' second wave (TEAM5: join requests, invite links, plans, time together): its
//   scenarios and switches are in mock-crew-wave2.ts (`crewApproval`, `crewPlans`, `crewAsk`…);
// - crew tags (TEAM6: tags before names, their cards, hidden crews, requests from a tag, "Hoş
//   geldin"): mock-crew-badges.ts (`badges`, `badgesLive`, `badgesHidden`, `badgesJoined`).
import { CHAT_SCENARIOS, mockChat } from "./mock-chat";
import { BADGE_SCENARIOS, badgesApi, badgesSnapshot, startBadges } from "./mock-crew-badges";
import { startWave2, WAVE2_SCENARIOS, wave2Api, wave2Snapshot } from "./mock-crew-wave2";
import { pick } from "./mock-lang";
import type {
  BackupMeta,
  CallPlace,
  CodeLookup,
  CrewCall,
  CrewLook,
  Friend,
  FriendGroups,
  FriendRequest,
  LimitedAccount,
  ProfileMark,
  SocialApi,
  SocialInvite,
  SocialNotice,
  SocialSession,
  SocialSnapshot,
  SocialTeam,
  StatusMode,
  TeamInvite,
  TeamMember,
} from "./social-types";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const now = () => Math.floor(Date.now() / 1000);

class MockSocialError extends Error {
  readonly code: string;
  readonly details: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
    this.details = `Kod: ${code}\nÖnizleme verisi`;
  }
}

/** Nobody has the friend code, worded like the backend. */
const codeNotFound = () =>
  new MockSocialError(
    "SOSYAL-KOD-YOK",
    pick("Bu koda ait kimse bulunamadı.", "Nobody has this code."),
  );

function searchParam(key: string): string | null {
  if (typeof window === "undefined") return null;
  const fromSearch = new URLSearchParams(window.location.search).get(key);
  const fromHash = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get(key);
  return fromSearch ?? fromHash;
}

/**
 * `&marks=1` (SOC-VERIFY): some people carry a mark, GeceKurdu_ and ZumrutKilic the verified one, KorAtesi the
 * admin one, the request from YildizTozu and two crewmates the verified one; `&marks=me` the player's own
 * admin mark too.
 */
const MARKS: Record<string, ProfileMark> = {
  "301": "verified",
  "302": "admin",
  "307": "verified",
  "401": "verified",
  "703": "verified",
  "802": "verified",
};

function markOf(id: string): { mark?: ProfileMark } {
  const asked = searchParam("marks");
  if (asked === null) return {};
  if (id === "100") return asked === "me" ? { mark: "admin" } : {};
  const mark = MARKS[id];
  return mark ? { mark } : {};
}

/** `&muted=`: until when the preview's crew is muted (Unix seconds; null: until unmuted), or
 * undefined when it is not. `crewMuted` mutes it to the half hour after the next two hours. */
function previewMute(): number | null | undefined {
  const asked = searchParam("muted") ?? (scenario() === "crewMuted" ? "auto" : null);
  if (asked === null) return undefined;
  if (asked === "forever") return null;
  const minutes = Number(asked);
  if (Number.isFinite(minutes) && minutes > 0) return now() + Math.round(minutes * 60);
  const halfHour = 30 * 60;
  return Math.ceil((now() + 2 * 3600) / halfHour) * halfHour;
}

/** A new Discord account's limits (SPAM1), lifting in five days. */
function previewLimited(): LimitedAccount {
  return {
    until: now() + 5 * 86400,
    items: [
      { key: "crewFound" },
      { key: "crewInvites", perDay: 5 },
      { key: "codeLookups", perDay: 10 },
      { key: "settingsBackup" },
      { key: "chatMessages", perDay: 100 },
      { key: "crewLink" },
      { key: "crewPlans" },
      { key: "skinChange" },
    ],
  };
}

function scenario(): string {
  if (typeof window === "undefined") return "friends";
  const fromSearch = new URLSearchParams(window.location.search).get("social");
  const fromHash = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get("social");
  return fromSearch ?? fromHash ?? "friends";
}

const discord = (index: number) => `https://cdn.discordapp.com/embed/avatars/${index}.png`;

/** QR modules of the two preview codes (made with the same encoder as the app). */
const QR_FIRST = {
  size: 29,
  path: "M0 0h7v1h-7zM8 0h1v1h-1zM12 0h2v1h-2zM15 0h2v1h-2zM19 0h2v1h-2zM22 0h7v1h-7zM0 1h1v1h-1zM6 1h1v1h-1zM8 1h2v1h-2zM12 1h1v1h-1zM16 1h1v1h-1zM20 1h1v1h-1zM22 1h1v1h-1zM28 1h1v1h-1zM0 2h1v1h-1zM2 2h3v1h-3zM6 2h1v1h-1zM11 2h6v1h-6zM18 2h3v1h-3zM22 2h1v1h-1zM24 2h3v1h-3zM28 2h1v1h-1zM0 3h1v1h-1zM2 3h3v1h-3zM6 3h1v1h-1zM8 3h1v1h-1zM12 3h2v1h-2zM15 3h4v1h-4zM20 3h1v1h-1zM22 3h1v1h-1zM24 3h3v1h-3zM28 3h1v1h-1zM0 4h1v1h-1zM2 4h3v1h-3zM6 4h1v1h-1zM10 4h1v1h-1zM13 4h3v1h-3zM19 4h1v1h-1zM22 4h1v1h-1zM24 4h3v1h-3zM28 4h1v1h-1zM0 5h1v1h-1zM6 5h1v1h-1zM10 5h5v1h-5zM17 5h4v1h-4zM22 5h1v1h-1zM28 5h1v1h-1zM0 6h7v1h-7zM8 6h1v1h-1zM10 6h1v1h-1zM12 6h1v1h-1zM14 6h1v1h-1zM16 6h1v1h-1zM18 6h1v1h-1zM20 6h1v1h-1zM22 6h7v1h-7zM8 7h1v1h-1zM15 7h1v1h-1zM17 7h1v1h-1zM19 7h2v1h-2zM0 8h1v1h-1zM2 8h2v1h-2zM5 8h3v1h-3zM13 8h7v1h-7zM22 8h1v1h-1zM25 8h1v1h-1zM27 8h2v1h-2zM0 9h1v1h-1zM4 9h2v1h-2zM11 9h2v1h-2zM14 9h3v1h-3zM18 9h7v1h-7zM28 9h1v1h-1zM2 10h1v1h-1zM6 10h1v1h-1zM8 10h3v1h-3zM12 10h1v1h-1zM14 10h1v1h-1zM16 10h1v1h-1zM18 10h1v1h-1zM26 10h2v1h-2zM2 11h3v1h-3zM8 11h1v1h-1zM11 11h1v1h-1zM13 11h3v1h-3zM18 11h5v1h-5zM24 11h1v1h-1zM28 11h1v1h-1zM1 12h1v1h-1zM6 12h1v1h-1zM13 12h1v1h-1zM15 12h3v1h-3zM25 12h2v1h-2zM0 13h4v1h-4zM9 13h1v1h-1zM11 13h1v1h-1zM13 13h1v1h-1zM15 13h1v1h-1zM17 13h1v1h-1zM19 13h1v1h-1zM22 13h1v1h-1zM26 13h3v1h-3zM1 14h2v1h-2zM5 14h4v1h-4zM11 14h3v1h-3zM17 14h3v1h-3zM21 14h1v1h-1zM23 14h2v1h-2zM26 14h3v1h-3zM0 15h4v1h-4zM5 15h1v1h-1zM7 15h1v1h-1zM9 15h2v1h-2zM14 15h3v1h-3zM19 15h1v1h-1zM21 15h1v1h-1zM24 15h1v1h-1zM27 15h1v1h-1zM0 16h3v1h-3zM4 16h3v1h-3zM8 16h1v1h-1zM11 16h2v1h-2zM15 16h1v1h-1zM19 16h2v1h-2zM23 16h3v1h-3zM27 16h1v1h-1zM1 17h1v1h-1zM4 17h1v1h-1zM9 17h1v1h-1zM12 17h6v1h-6zM20 17h1v1h-1zM23 17h1v1h-1zM25 17h3v1h-3zM0 18h1v1h-1zM3 18h1v1h-1zM6 18h1v1h-1zM9 18h1v1h-1zM11 18h2v1h-2zM14 18h2v1h-2zM17 18h1v1h-1zM20 18h1v1h-1zM22 18h2v1h-2zM26 18h1v1h-1zM2 19h3v1h-3zM7 19h2v1h-2zM10 19h1v1h-1zM12 19h1v1h-1zM14 19h2v1h-2zM17 19h3v1h-3zM22 19h1v1h-1zM24 19h1v1h-1zM26 19h1v1h-1zM1 20h1v1h-1zM5 20h2v1h-2zM8 20h2v1h-2zM13 20h6v1h-6zM20 20h7v1h-7zM8 21h1v1h-1zM10 21h1v1h-1zM12 21h1v1h-1zM15 21h2v1h-2zM18 21h1v1h-1zM20 21h1v1h-1zM24 21h5v1h-5zM0 22h7v1h-7zM8 22h3v1h-3zM18 22h3v1h-3zM22 22h1v1h-1zM24 22h2v1h-2zM27 22h1v1h-1zM0 23h1v1h-1zM6 23h1v1h-1zM8 23h6v1h-6zM15 23h2v1h-2zM20 23h1v1h-1zM24 23h2v1h-2zM0 24h1v1h-1zM2 24h3v1h-3zM6 24h1v1h-1zM9 24h1v1h-1zM12 24h1v1h-1zM15 24h4v1h-4zM20 24h5v1h-5zM26 24h3v1h-3zM0 25h1v1h-1zM2 25h3v1h-3zM6 25h1v1h-1zM8 25h2v1h-2zM11 25h2v1h-2zM14 25h1v1h-1zM16 25h1v1h-1zM21 25h1v1h-1zM23 25h3v1h-3zM28 25h1v1h-1zM0 26h1v1h-1zM2 26h3v1h-3zM6 26h1v1h-1zM8 26h3v1h-3zM13 26h1v1h-1zM16 26h4v1h-4zM21 26h1v1h-1zM23 26h1v1h-1zM26 26h1v1h-1zM28 26h1v1h-1zM0 27h1v1h-1zM6 27h1v1h-1zM9 27h1v1h-1zM13 27h3v1h-3zM19 27h3v1h-3zM23 27h1v1h-1zM25 27h1v1h-1zM27 27h1v1h-1zM0 28h7v1h-7zM8 28h1v1h-1zM12 28h3v1h-3zM16 28h5v1h-5zM23 28h1v1h-1zM25 28h1v1h-1zM27 28h1v1h-1z",
};
const QR_ROTATED = {
  size: 29,
  path: "M0 0h7v1h-7zM8 0h4v1h-4zM13 0h1v1h-1zM15 0h3v1h-3zM22 0h7v1h-7zM0 1h1v1h-1zM6 1h1v1h-1zM8 1h1v1h-1zM10 1h1v1h-1zM16 1h5v1h-5zM22 1h1v1h-1zM28 1h1v1h-1zM0 2h1v1h-1zM2 2h3v1h-3zM6 2h1v1h-1zM8 2h1v1h-1zM11 2h1v1h-1zM13 2h1v1h-1zM15 2h2v1h-2zM19 2h2v1h-2zM22 2h1v1h-1zM24 2h3v1h-3zM28 2h1v1h-1zM0 3h1v1h-1zM2 3h3v1h-3zM6 3h1v1h-1zM9 3h1v1h-1zM11 3h5v1h-5zM17 3h2v1h-2zM22 3h1v1h-1zM24 3h3v1h-3zM28 3h1v1h-1zM0 4h1v1h-1zM2 4h3v1h-3zM6 4h1v1h-1zM8 4h2v1h-2zM12 4h2v1h-2zM15 4h1v1h-1zM18 4h2v1h-2zM22 4h1v1h-1zM24 4h3v1h-3zM28 4h1v1h-1zM0 5h1v1h-1zM6 5h1v1h-1zM11 5h1v1h-1zM13 5h1v1h-1zM15 5h2v1h-2zM20 5h1v1h-1zM22 5h1v1h-1zM28 5h1v1h-1zM0 6h7v1h-7zM8 6h1v1h-1zM10 6h1v1h-1zM12 6h1v1h-1zM14 6h1v1h-1zM16 6h1v1h-1zM18 6h1v1h-1zM20 6h1v1h-1zM22 6h7v1h-7zM9 7h1v1h-1zM13 7h2v1h-2zM18 7h1v1h-1zM20 7h1v1h-1zM0 8h1v1h-1zM3 8h12v1h-12zM17 8h1v1h-1zM19 8h1v1h-1zM21 8h1v1h-1zM24 8h1v1h-1zM26 8h3v1h-3zM1 9h1v1h-1zM9 9h1v1h-1zM12 9h5v1h-5zM18 9h2v1h-2zM23 9h2v1h-2zM26 9h2v1h-2zM0 10h4v1h-4zM6 10h7v1h-7zM14 10h2v1h-2zM21 10h1v1h-1zM24 10h1v1h-1zM26 10h1v1h-1zM0 11h1v1h-1zM2 11h4v1h-4zM8 11h1v1h-1zM11 11h4v1h-4zM16 11h2v1h-2zM20 11h4v1h-4zM25 11h1v1h-1zM28 11h1v1h-1zM0 12h1v1h-1zM2 12h2v1h-2zM6 12h1v1h-1zM9 12h1v1h-1zM11 12h1v1h-1zM13 12h1v1h-1zM15 12h2v1h-2zM19 12h2v1h-2zM22 12h2v1h-2zM28 12h1v1h-1zM2 13h1v1h-1zM5 13h1v1h-1zM7 13h1v1h-1zM9 13h1v1h-1zM11 13h1v1h-1zM13 13h2v1h-2zM16 13h1v1h-1zM18 13h1v1h-1zM22 13h7v1h-7zM0 14h1v1h-1zM3 14h2v1h-2zM6 14h1v1h-1zM8 14h3v1h-3zM12 14h2v1h-2zM15 14h1v1h-1zM17 14h1v1h-1zM19 14h1v1h-1zM23 14h1v1h-1zM26 14h1v1h-1zM28 14h1v1h-1zM5 15h1v1h-1zM7 15h3v1h-3zM14 15h1v1h-1zM19 15h2v1h-2zM22 15h1v1h-1zM24 15h1v1h-1zM26 15h1v1h-1zM28 15h1v1h-1zM1 16h2v1h-2zM4 16h4v1h-4zM9 16h1v1h-1zM12 16h1v1h-1zM14 16h1v1h-1zM16 16h1v1h-1zM18 16h4v1h-4zM23 16h1v1h-1zM25 16h1v1h-1zM0 17h2v1h-2zM3 17h3v1h-3zM8 17h2v1h-2zM11 17h1v1h-1zM13 17h1v1h-1zM18 17h3v1h-3zM24 17h1v1h-1zM26 17h2v1h-2zM0 18h4v1h-4zM6 18h1v1h-1zM10 18h1v1h-1zM15 18h2v1h-2zM19 18h1v1h-1zM25 18h1v1h-1zM28 18h1v1h-1zM0 19h2v1h-2zM3 19h1v1h-1zM7 19h1v1h-1zM9 19h1v1h-1zM11 19h1v1h-1zM14 19h2v1h-2zM22 19h2v1h-2zM25 19h2v1h-2zM0 20h4v1h-4zM6 20h5v1h-5zM13 20h2v1h-2zM17 20h1v1h-1zM20 20h8v1h-8zM8 21h2v1h-2zM12 21h1v1h-1zM14 21h1v1h-1zM16 21h1v1h-1zM18 21h1v1h-1zM20 21h1v1h-1zM24 21h2v1h-2zM0 22h7v1h-7zM8 22h5v1h-5zM14 22h1v1h-1zM19 22h2v1h-2zM22 22h1v1h-1zM24 22h2v1h-2zM0 23h1v1h-1zM6 23h1v1h-1zM8 23h2v1h-2zM11 23h2v1h-2zM14 23h1v1h-1zM16 23h5v1h-5zM24 23h1v1h-1zM28 23h1v1h-1zM0 24h1v1h-1zM2 24h3v1h-3zM6 24h1v1h-1zM8 24h1v1h-1zM10 24h2v1h-2zM13 24h2v1h-2zM16 24h1v1h-1zM18 24h8v1h-8zM0 25h1v1h-1zM2 25h3v1h-3zM6 25h1v1h-1zM8 25h2v1h-2zM11 25h1v1h-1zM15 25h1v1h-1zM18 25h2v1h-2zM21 25h1v1h-1zM27 25h1v1h-1zM0 26h1v1h-1zM2 26h3v1h-3zM6 26h1v1h-1zM10 26h1v1h-1zM12 26h2v1h-2zM18 26h2v1h-2zM23 26h2v1h-2zM26 26h3v1h-3zM0 27h1v1h-1zM6 27h1v1h-1zM15 27h1v1h-1zM18 27h2v1h-2zM22 27h2v1h-2zM25 27h2v1h-2zM28 27h1v1h-1zM0 28h7v1h-7zM8 28h2v1h-2zM11 28h1v1h-1zM13 28h1v1h-1zM15 28h2v1h-2zM19 28h3v1h-3zM23 28h3v1h-3z",
};

function friendsList(): Friend[] {
  return friendsBase().map((friend) => ({ ...friend, ...markOf(friend.id) }));
}

function friendsBase(): Friend[] {
  return [
    {
      id: "301",
      name: "GeceKurdu_",
      username: "gecekurdu_",
      avatar: "preset:fox",
      state: "playing",
      dnd: false,
      game: { where: "projects", host: "play.projects.gg", ver: "1.21.4" },
      discordUrl: "https://discord.com/users/301",
      since: "2026-09-02",
    },
    {
      id: "302",
      name: "KorAtesi",
      username: "koratesi",
      avatar: "discord",
      avatarUrl: discord(2),
      state: "playing",
      dnd: true,
      game: { where: "projects", host: "play.projects.gg" },
      discordUrl: "https://discord.com/users/302",
      since: "2026-08-11",
    },
    {
      id: "303",
      name: "ElmasKazma",
      username: "elmaskazma",
      avatar: "preset:knight",
      state: "playing",
      dnd: false,
      game: { where: "singleplayer", ver: "26.2" },
      discordUrl: "https://discord.com/users/303",
      since: "2026-07-19",
    },
    {
      id: "304",
      name: "PixelAvci",
      username: "pixelavci",
      avatar: "preset:wizard",
      state: "playing",
      dnd: false,
      game: { where: "other", ver: "1.20.1" },
      discordUrl: "https://discord.com/users/304",
    },
    {
      id: "305",
      name: "NetherKral",
      username: "netherkral",
      avatar: "preset:axolotl",
      state: "launcher",
      dnd: false,
      discordUrl: "https://discord.com/users/305",
    },
    {
      id: "306",
      name: "KizilEjder",
      username: "kizilejder",
      avatar: "discord",
      avatarUrl: discord(0),
      state: "offline",
      dnd: false,
      lastSeen: "today",
      discordUrl: "https://discord.com/users/306",
    },
    {
      id: "307",
      name: "ZumrutKilic",
      username: "zumrutkilic",
      avatar: "preset:wolf",
      state: "offline",
      dnd: false,
      lastSeen: "yesterday",
      discordUrl: "https://discord.com/users/307",
    },
    {
      id: "308",
      name: "EnderYolcu",
      username: "enderyolcu",
      avatar: "preset:ember-dragon",
      state: "offline",
      dnd: false,
      lastSeen: "week",
      discordUrl: "https://discord.com/users/308",
    },
    {
      id: "309",
      name: "Obsidyen77",
      username: "obsidyen77",
      avatar: "discord",
      state: "offline",
      dnd: false,
      discordUrl: "https://discord.com/users/309",
    },
  ];
}

const backup: BackupMeta = {
  version: 6,
  updatedAt: now() - 3 * 86_400,
  device: "LAPTOP-EV",
  size: 840,
};

/** The player's categories in the `groups` and `team` previews. */
function previewGroups(): FriendGroups {
  return {
    version: 4,
    groups: [
      { id: "okul", name: pick("Okul", "School") },
      { id: "pvp", name: "PvP" },
      { id: "skyblock", name: "Skyblock" },
    ],
    members: { "301": "okul", "305": "okul", "307": "okul", "302": "pvp", "304": "pvp" },
  };
}

function teamMember(
  friend: Friend | undefined,
  extra: Partial<TeamMember> & Pick<TeamMember, "id" | "name">,
): TeamMember {
  return {
    username: extra.name.toLowerCase().replace(/\s+/g, ""),
    avatar: "discord",
    role: "member",
    moderator: false,
    me: false,
    friend: Boolean(friend),
    blocked: false,
    state: "offline",
    dnd: false,
    discordUrl: `https://discord.com/users/${extra.id}`,
    joinedAt: now() - 86_400 * 20,
    togetherMonth: 0,
    ...(friend
      ? {
          username: friend.username,
          avatar: friend.avatar,
          avatarUrl: friend.avatarUrl,
          state: friend.state,
          dnd: friend.dnd,
          game: friend.game?.where === "other" ? { where: "other" as const } : friend.game,
        }
      : {}),
    ...markOf(extra.id),
    ...extra,
  };
}

/** Where the preview's crews come from: a service before looks, calls and roles (`crewOld`). */
const OLD_CREWS = scenario() === "crewOld";
/** Several crews per player (TEAM4). */
const MULTI =
  !OLD_CREWS &&
  (["crews", "crewsFull", "crewCallMulti", "crewAsk", "crewAccepted"].includes(scenario()) ||
    BADGE_SCENARIOS.includes(scenario()) ||
    searchParam("multi") !== null);
/** How many crews the player may be in (`&crewLimit=`; `crewsFull`: the three they are in). */
function previewLimit(): number {
  if (!MULTI) return 1;
  if (scenario() === "crewsFull") return 3;
  const asked = Number(searchParam("crewLimit"));
  return Number.isInteger(asked) && asked > 0 ? asked : 5;
}
const NOT_MUTED = { muted: false, muteUntil: null };
const PREVIEW_LOOK: CrewLook = OLD_CREWS
  ? { emblem: "shield", color: "gold", frame: "cut" }
  : { emblem: "controller", color: "teal", frame: "banner" };

/** "Gece Kartalları", led by the player (or by GeceKurdu_ in `teamMember` and `teamModerator`). */
function previewTeam(founder: boolean, moderator = false): SocialTeam {
  const friends = friendsList();
  const byId = (id: string) => friends.find((f) => f.id === id);
  // Founded a month ago today: the card shows the month's anniversary line.
  const founded = new Date();
  founded.setMonth(founded.getMonth() - 1);
  const day = (date: Date) =>
    `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
  return {
    id: "0123456789abcdef",
    name: "Gece Kartalları",
    tag: "GKR",
    ...PREVIEW_LOOK,
    motto: OLD_CREWS ? "" : pick("Birlikte güçlüyüz!", "Stronger together!"),
    code: "EKP-7Q2X-9KDA",
    createdOn: day(founded),
    role: founder ? "founder" : "member",
    moderator: !founder && moderator,
    permissions: {
      invite: "members",
      call: "members",
      kick: OLD_CREWS ? "leader" : "moderators",
      edit: moderator ? "moderators" : "leader",
      admit: "moderators",
    },
    members: [
      teamMember(undefined, {
        id: "100",
        name: "EjderAvcisi",
        avatar: "preset:golden-dragon",
        me: true,
        role: founder ? "founder" : "member",
        moderator: !founder && moderator,
        state: "launcher",
        joinedAt: now() - 86_400 * 21,
      }),
      teamMember(byId("301"), {
        id: "301",
        name: "GeceKurdu_",
        role: founder ? "member" : "founder",
      }),
      teamMember(byId("302"), { id: "302", name: "KorAtesi", moderator: !OLD_CREWS }),
      teamMember(undefined, {
        id: "701",
        name: "MaviElmas",
        avatar: "preset:knight",
        state: "playing",
        game: { where: "projects", host: "play.projects.gg", ver: "1.21.4" },
      }),
      teamMember(undefined, {
        id: "702",
        name: "DemirGolem_",
        avatar: "preset:axolotl",
        state: "launcher",
      }),
      teamMember(byId("306"), { id: "306", name: "KizilEjder" }),
      teamMember(undefined, { id: "703", name: "TasDevi", avatar: "preset:wolf" }),
    ],
    invited: ["305"],
    limit: 30,
    mute: NOT_MUTED,
    join: "open",
    requests: [],
    togetherWeek: 0,
  };
}

/** The `crews` previews: "Gece Kartalları" (the player leads it), "Skyblock Ustaları" and "PvP
 * Kulübü" (muted on its own until unmuted), with GeceKurdu_ in all three and two more on Projects. */
function previewCrews(): SocialTeam[] {
  const gkr = previewTeam(true);
  const me = gkr.members.find((m) => m.me);
  const ayse = gkr.members.find((m) => m.id === "301");
  const crew = (
    id: string,
    look: CrewLook & { name: string; tag: string; motto: string },
    members: TeamMember[],
    extra: Partial<SocialTeam> = {},
  ): SocialTeam => ({
    ...gkr,
    ...look,
    id,
    code: `EKP-${id.slice(0, 4).toUpperCase()}-${id.slice(4, 8).toUpperCase()}`,
    role: "member",
    moderator: false,
    members: members.map((m) => (m.me ? { ...m, role: "member", moderator: false } : m)),
    invited: [],
    ...extra,
  });
  const onProjects = {
    state: "playing" as const,
    game: { where: "projects" as const, host: "play.projects.gg" },
  };
  return [
    gkr,
    crew(
      "5555666677778888",
      {
        name: "Skyblock Ustaları",
        tag: "SKY",
        emblem: "castle",
        color: "purple",
        frame: "hex",
        motto: pick("Adadan adaya", "Island to island"),
      },
      [
        teamMember(undefined, {
          id: "801",
          name: "PixelAvci",
          avatar: "preset:wizard",
          ...onProjects,
          role: "founder",
        }),
        ...(ayse ? [ayse] : []),
        ...(me ? [me] : []),
        teamMember(undefined, {
          id: "802",
          name: "Obsidyen77",
          avatar: "preset:fox",
          ...onProjects,
        }),
        teamMember(undefined, {
          id: "803",
          name: "AlevYagmuru",
          avatar: "preset:wolf",
          state: "launcher",
        }),
      ],
    ),
    crew(
      "9999aaaabbbbcccc",
      {
        name: pick("PvP Kulübü", "PvP Club"),
        tag: "PVP",
        emblem: "sword",
        color: "crimson",
        frame: "shield",
        motto: "",
      },
      [
        teamMember(undefined, {
          id: "804",
          name: "KaranlikPvP",
          avatar: "preset:knight",
          role: "founder",
        }),
        ...(me ? [me] : []),
        ...(ayse ? [ayse] : []),
      ],
      { mute: { muted: true, muteUntil: null } },
    ),
  ];
}

const teamInvite = (): TeamInvite => ({
  teamId: "fedcba9876543210",
  name: "Skyblock Ustaları",
  tag: "SKY",
  emblem: OLD_CREWS ? "cube" : "castle",
  color: OLD_CREWS ? "gold" : "purple",
  frame: OLD_CREWS ? "cut" : "hex",
  motto: OLD_CREWS ? "" : pick("Adadan adaya", "Island to island"),
  members: 6,
  from: { id: "303", name: "ElmasKazma", username: "elmaskazma", avatar: "preset:knight" },
  sentAt: now() - 1_800,
});

/** A crew call: GeceKurdu_'s from projects.gg (`join`), KorAtesi's from another server (`other`),
 * DemirGolem_'s from the launcher (`ready`, before CALL1), or the player's own with a few answers
 * (`mine`). Twenty minutes, a few already gone. */
function previewCall(kind: "join" | "other" | "ready" | "mine", crews: string[] = []): CrewCall {
  const started = now() - 180;
  const who = {
    mine: { from: "100", name: "EjderAvcisi", avatar: "preset:golden-dragon" },
    join: { from: "301", name: "GeceKurdu_", avatar: "preset:fox" },
    other: { from: "302", name: "KorAtesi", avatar: "discord", avatarUrl: discord(2) },
    ready: { from: "702", name: "DemirGolem_", avatar: "preset:axolotl" },
  }[kind];
  return {
    id: `call-${kind}`,
    ...who,
    mine: kind === "mine",
    kind: kind === "ready" ? "ready" : "join",
    server:
      kind === "ready"
        ? undefined
        : kind === "other"
          ? { where: "other", host: "survival.example.net" }
          : { where: "projects", host: "play.projects.gg" },
    crews,
    startedAt: started,
    endsAt: started + 20 * 60,
    answers:
      kind === "mine"
        ? [
            { id: "301", answer: "yes" },
            { id: "701", answer: "yes" },
            { id: "302", answer: "soon" },
            { id: "703", answer: "no" },
          ]
        : [{ id: "302", answer: "soon" }],
  };
}

function initial(): SocialSnapshot {
  const name = scenario();
  const base: SocialSnapshot = {
    seq: 1,
    enabled: name !== "off",
    auth: "signedIn",
    connection: "online",
    loaded: true,
    me: {
      id: "100",
      name: "EjderAvcisi",
      username: "ejderavcisi",
      avatar: "preset:golden-dragon",
      discordAvatarUrl: discord(1),
      friendCode: "PRJ-7K3M-Q9TD",
      shareLink: "https://social.projects.gg/f/PRJ-7K3M-Q9TD",
      qr: QR_FIRST,
      status: "online",
      privacy: { shareServer: false, shareLastSeen: true },
      backup,
      ...markOf("100"),
    },
    friends: friendsList(),
    incoming: [
      {
        id: "401",
        name: "YildizTozu",
        username: "yildiztozu",
        avatar: "preset:baby-dragon",
        sentAt: now() - 600,
        ...markOf("401"),
      },
    ],
    outgoing: [
      {
        id: "402",
        name: "Lavci_42",
        username: "lavci_42",
        avatar: "discord",
        avatarUrl: discord(4),
        sentAt: now() - 7200,
      },
    ],
    blocked: [{ id: "501", name: "spam_hesap", username: "spam_hesap", avatar: "discord" }],
    invites: [
      {
        id: "i-quiet",
        from: "302",
        name: "KorAtesi",
        avatar: "discord",
        avatarUrl: discord(2),
        ...markOf("302"),
        at: now() - 900,
        server: { where: "projects", host: "play.projects.gg" },
        quiet: true,
      },
    ],
    activity: { state: "launcher" },
    groups: { version: 0, groups: [], members: {} },
    teams: false,
    crew: NO_CREW_FEATURES,
    crews: [],
    crewLimit: 1,
    teamInvites: [],
    calls: [],
    callPlace: "needsServer",
    chatAllowed: false,
    notices: [],
    teamRequests: [],
    plans: [],
    together: false,
  };
  // Crews on, with what the service offers of them.
  const keepsMutes = !OLD_CREWS && searchParam("noMute") === null;
  const mutedUntil = previewMute();
  const crewed = (crews: SocialTeam[], extra: Partial<SocialSnapshot> = {}) => ({
    ...base,
    groups: previewGroups(),
    teams: true,
    crew: OLD_CREWS
      ? NO_CREW_FEATURES
      : {
          look: true,
          calls: true,
          roles: true,
          mute: keepsMutes,
          multi: MULTI,
          callServer: true,
          // On with the second wave's data (mock-crew-wave2.ts).
          requests: false,
          link: false,
          plans: false,
          planNotes: false,
          // FIX2: "Arkadaş ekle" on a crewmate (`&fix2=0`: a service from before it).
          crewFriendRequests: searchParam("fix2") !== "0",
          // On with crew tags' data (mock-crew-badges.ts).
          badges: false,
          badgeRequests: false,
        },
    crewNotifications: keepsMutes
      ? {
          calls: true,
          muted: mutedUntil !== undefined,
          muteUntil: mutedUntil ?? null,
        }
      : undefined,
    crews,
    crewLimit: previewLimit(),
    ...extra,
  });
  // Chat previews (mock-chat.ts): the player leads the crew, so its chat is there too (GeceKurdu_ leads
  // it in `chatTombstones`, whose crew chat shows her deletion).
  if (CHAT_SCENARIOS.includes(name)) return crewed([previewTeam(name !== "chatTombstones")]);
  // The second wave's (mock-crew-wave2.ts): led by the player, or by GeceKurdu_ in `crewPlans`.
  if (WAVE2_SCENARIOS.includes(name)) {
    return crewed([previewTeam(!name.startsWith("crewPlan"))]);
  }
  // Crew tags' (mock-crew-badges.ts): the three crews of `crews`.
  if (BADGE_SCENARIOS.includes(name)) return crewed(previewCrews());
  switch (name) {
    case "groups":
      return { ...base, groups: previewGroups() };
    case "team":
    case "crewOld":
      return crewed([previewTeam(true)]);
    case "teamMember":
    case "crewMuted":
      return crewed([previewTeam(false)]);
    case "teamModerator":
      return crewed([previewTeam(false, true)]);
    case "crewCall":
      return crewed([previewTeam(false)]);
    case "crewCallOpen":
      return crewed([previewTeam(false)], { calls: [previewCall("ready")] });
    case "crewCallOther":
      return crewed([previewTeam(false)], { calls: [previewCall("other")] });
    case "crewCallMine":
      return crewed([previewTeam(true)], { calls: [previewCall("mine")] });
    case "crews":
    case "crewsFull":
    case "crewCallMulti":
      // An invite waits (at the limit too: the add view still opens, to decline it).
      return crewed(previewCrews(), {
        teamInvites: name === "crewCallMulti" ? [] : [teamInvite()],
      });
    case "teamNew":
      return crewed([], { teamInvites: [teamInvite()] });
    case "teamInvite":
      return crewed([]);
    case "legacy":
      // A service without categories and teams (the live one until SOC4 is deployed).
      return { ...base, groups: undefined };
    case "playing":
      // The player is on projects.gg: "Birlikte oyna" is offered.
      return { ...base, activity: { state: "playing", host: "play.projects.gg" } };
    case "playingOther":
      // On another server, its address shared with friends: "Birlikte oyna" is offered too.
      return {
        ...base,
        activity: { state: "playing", host: "survival.example.net" },
        me: base.me ? { ...base.me, privacy: { ...base.me.privacy, shareServer: true } } : base.me,
      };
    case "signedOut":
      return signedOut(base);
    case "signingIn":
      return { ...signedOut(base), auth: "signingIn" };
    case "loading":
      return { ...base, loaded: false, friends: [], incoming: [], outgoing: [], invites: [] };
    case "empty":
      return { ...base, friends: [], incoming: [], outgoing: [], invites: [], blocked: [] };
    case "down":
      return {
        ...base,
        connection: "unavailable",
        friends: base.friends.map((f) => ({ ...f, state: "offline", game: undefined })),
        invites: [],
      };
    case "replaced":
      return { ...base, connection: "replaced" };
    case "limited":
      // A new Discord account (SPAM1): no crew yet, and it may not found one.
      return crewed([], { me: base.me ? { ...base.me, limited: previewLimited() } : base.me });
    case "crewReset":
      // Moderators reset it: "Ekip XXXX", the tag XXXX, no motto, locked for a while.
      return crewed([
        {
          ...previewTeam(true),
          name: "Ekip 4F8A",
          tag: "4F8A",
          motto: "",
          textLockedUntil: now() + 3 * 86400,
        },
      ]);
    case "crewGone":
      return crewed([previewTeam(false)]);
    case "warning":
      return {
        ...base,
        notices: [{ id: "n-warning", kind: "warning", at: now() - 3600, reason: "harassment" }],
      };
    case "profileReset":
      return {
        ...base,
        notices: [{ id: "n-reset", kind: "profileReset", at: now() - 600, reason: "name" }],
      };
    case "skinReset":
      return {
        ...base,
        notices: [
          {
            id: "n-skin",
            kind: "skinReset",
            at: now() - 300,
            blocked: { until: now() + 7 * 86400 },
          },
        ],
      };
    case "restricted":
      return { ...signedOut(base), restriction: { until: now() + 7 * 86400 } };
    case "banned":
      return { ...signedOut(base), restriction: {} };
    case "signupsClosed":
      return signedOut(base);
    case "backupWaits":
      return { ...base, backupWaitsUntil: now() + 3600 };
    default:
      return base;
  }
}

/** `&at=`: where the player plays, and so whether "Ekibi çağır" may start there (CALL1). */
function placed(snapshot: SocialSnapshot): SocialSnapshot {
  const at = searchParam("at");
  const share = (on: boolean) =>
    snapshot.me
      ? { ...snapshot.me, privacy: { ...snapshot.me.privacy, shareServer: on } }
      : snapshot.me;
  switch (at) {
    case "projects":
      return { ...snapshot, activity: { state: "playing", host: "play.projects.gg" } };
    case "other":
      return {
        ...snapshot,
        activity: { state: "playing", host: "survival.example.net" },
        me: share(true),
      };
    case "hidden":
      return { ...snapshot, activity: { state: "playing" }, me: share(false) };
    default:
      return snapshot;
  }
}

/** Where a call may start from, as the backend decides it (CALL1): the preview's other server is
 * hidden while the address is not shared. */
function placeOf(snapshot: SocialSnapshot): CallPlace {
  if (snapshot.activity.state !== "playing") return "needsServer";
  if (snapshot.activity.host === "play.projects.gg") return "ok";
  return snapshot.me?.privacy.shareServer ? "ok" : "serverHidden";
}

function signedOut(base: SocialSnapshot): SocialSnapshot {
  return {
    ...base,
    auth: "signedOut",
    connection: "idle",
    me: undefined,
    loaded: false,
    friends: [],
    incoming: [],
    outgoing: [],
    blocked: [],
    invites: [],
    crews: [],
    teamInvites: [],
    teamRequests: [],
    plans: [],
    chat: undefined,
  };
}

const NO_CREW_FEATURES = {
  look: false,
  calls: false,
  roles: false,
  mute: false,
  multi: false,
  callServer: false,
  requests: false,
  link: false,
  plans: false,
  planNotes: false,
  crewFriendRequests: false,
  badges: false,
  badgeRequests: false,
};

// Chat (CHAT1) lives in mock-chat.ts, on this preview's data.
const chat = mockChat({
  scenario: scenario(),
  param: searchParam,
  view: () => view,
  commit,
  tell,
});
let view = withPlace(
  placed(
    chat.prepare(
      badgesSnapshot(
        wave2Snapshot(initial(), { scenario: scenario(), param: searchParam }),
        badgesContext(),
      ),
    ),
  ),
);

/** What mock-crew-badges.ts works on: this preview's data and its ways to change it. */
function badgesContext() {
  return {
    scenario: scenario(),
    param: searchParam,
    view: () => view,
    commit,
    fail: (code: string, tr: string, en: string) => new MockSocialError(code, pick(tr, en)),
    pick,
    send: (conv: string, text: string) => chat.api.chatSend(conv, text),
    sendSticker: (conv: string, sticker: string) => chat.api.chatSendSticker(conv, sticker),
  };
}

function withPlace(snapshot: SocialSnapshot): SocialSnapshot {
  // The hidden server's address shows once the player shares it ("Aç ve çağır").
  const activity =
    searchParam("at") === "hidden" &&
    snapshot.activity.state === "playing" &&
    snapshot.me?.privacy.shareServer
      ? { state: "playing" as const, host: "survival.example.net" }
      : snapshot.activity;
  const next = { ...snapshot, activity };
  return { ...next, callPlace: placeOf(next) };
}
const viewListeners = new Set<(view: SocialSnapshot) => void>();
const noticeListeners = new Set<(notice: SocialNotice) => void>();

function commit(change: (draft: SocialSnapshot) => SocialSnapshot) {
  view = { ...withPlace(change(view)), seq: view.seq + 1 };
  for (const listener of viewListeners) listener(view);
}

function tell(notice: SocialNotice) {
  for (const listener of noticeListeners) listener(notice);
}

if (typeof window !== "undefined") {
  const name = scenario();
  chat.start();
  startWave2(wave2Context());
  startBadges(badgesContext());
  // `&liveAt=<ms>`: when the live moderators' step comes.
  const liveAt = Number(searchParam("liveAt")) || 1600;
  if (name === "restrictedLive") {
    // Moderators suspend the account while the player is signed in.
    setTimeout(() => {
      const until = now() + 7 * 86400;
      commit((v) => ({ ...signedOut(v), restriction: { until } }));
      tell({ kind: "restricted", until });
    }, liveAt);
  }
  if (name === "crewGone") {
    setTimeout(() => {
      const crew = view.crews[0];
      if (!crew) return;
      commit((v) => ({ ...v, crews: v.crews.filter((c) => c.id !== crew.id) }));
      tell({ kind: "teamGone", reason: "disbanded", name: crew.name, staff: true });
    }, liveAt);
  }
  if (name === "signupsClosed") {
    // As after a sign-in the service turned down today.
    setTimeout(() => tell({ kind: "signInFailed", reason: "signupsClosed" }), 400);
  }
  if (name === "invite" || name === "inviteOther") {
    // `?inviteAt=<ms>`: when it arrives (a game started in the preview first, for example).
    const at = Number(new URLSearchParams(window.location.search).get("inviteAt")) || 1600;
    setTimeout(() => {
      const invite: SocialInvite =
        name === "invite"
          ? {
              id: "i-live",
              from: "301",
              name: "GeceKurdu_",
              avatar: "preset:fox",
              at: now(),
              server: { where: "projects", host: "play.projects.gg" },
              quiet: false,
            }
          : {
              id: "i-live",
              from: "304",
              name: "PixelAvci",
              avatar: "preset:wizard",
              at: now(),
              server: { where: "other", host: "survival.example.net" },
              quiet: false,
            };
      commit((v) => ({ ...v, invites: [invite, ...v.invites] }));
      tell({ kind: "invite", invite });
    }, at);
  }
  if (name === "restore") {
    setTimeout(() => tell({ kind: "signedIn", restore: backup }), 1200);
  }
  if (name === "teamInvite") {
    setTimeout(() => {
      const invite = { ...teamInvite(), sentAt: now() };
      commit((v) => ({ ...v, teamInvites: [invite, ...v.teamInvites] }));
      tell({ kind: "teamInvite", invite });
    }, 1600);
  }
  if (name === "crewCall" || name === "crewCallMulti") {
    // `?callAt=<ms>`: when it arrives.
    const at = Number(new URLSearchParams(window.location.search).get("callAt")) || 1600;
    setTimeout(() => {
      const call: CrewCall = {
        ...previewCall(
          "join",
          name === "crewCallMulti" ? view.crews.slice(0, 2).map((crew) => crew.id) : [],
        ),
        startedAt: now(),
        endsAt: now() + 20 * 60,
        answers: [],
      };
      commit((v) => ({ ...v, calls: [...v.calls, call] }));
      tell({ kind: "crewCall", call });
    }, at);
  }
}

/** One of the player's crews as it is, or an error worded like the backend when it is gone. */
function currentTeam(teamId: string): SocialTeam {
  const team = view.crews.find((crew) => crew.id === teamId);
  if (!team) {
    throw new MockSocialError(
      "SOSYAL-TAKIM-YOK",
      pick("Artık bir ekipte değilsin.", "You're not in a crew anymore."),
    );
  }
  return team;
}

function setTeam(teamId: string, change: (team: SocialTeam) => SocialTeam) {
  commit((v) => ({
    ...v,
    crews: v.crews.map((crew) => (crew.id === teamId ? change(crew) : crew)),
  }));
}

/** A crew more: after the others with several crews (at the limit, the backend's refusal), else
 * in place of the one there was. */
function addCrew(team: SocialTeam) {
  if (view.crew.multi && view.crews.length >= view.crewLimit) {
    throw new MockSocialError(
      "SOSYAL-EKIP-SINIR",
      pick("Daha fazla ekipte olamazsın.", "You can't be in any more crews."),
    );
  }
  commit((v) => ({
    ...v,
    crews: v.crew.multi ? [...v.crews.filter((crew) => crew.id !== team.id), team] : [team],
  }));
}

function dropCrew(teamId: string) {
  currentTeam(teamId);
  commit((v) => ({
    ...v,
    crews: v.crews.filter((crew) => crew.id !== teamId),
    calls: v.calls.filter((call) => !call.crews.includes(teamId) || call.crews.length > 1),
  }));
}

/** Crew codes the preview knows. */
const previewTeams: Record<
  string,
  CrewLook & { teamId: string; name: string; tag: string; motto: string }
> = {
  "EKP-4F8A-2KQX": {
    teamId: "1111222233334444",
    name: "Gece Kartalları",
    tag: "GKR",
    emblem: "controller",
    color: "teal",
    frame: "banner",
    motto: pick("Birlikte güçlüyüz!", "Stronger together!"),
  },
  "EKP-9WQ2-XM4C": {
    teamId: "fedcba9876543210",
    name: "Skyblock Ustaları",
    tag: "SKY",
    emblem: "castle",
    color: "purple",
    frame: "hex",
    motto: "",
  },
};

function joinedTeam(source: {
  teamId: string;
  name: string;
  tag: string;
  emblem: string;
  color?: string;
  frame?: string;
  motto?: string;
}): SocialTeam {
  const base = previewTeam(false);
  return {
    ...base,
    id: source.teamId,
    name: source.name,
    tag: source.tag,
    emblem: source.emblem,
    color: source.color ?? "gold",
    frame: source.frame ?? "cut",
    motto: source.motto ?? "",
    code: "EKP-WXYZ-1234",
    members: base.members.map((m) => (m.me ? { ...m, joinedAt: now() } : m)),
    invited: [],
  };
}

/** What mock-crew-wave2.ts works on: this preview's data and its ways to change it. */
function wave2Context() {
  return {
    scenario: scenario(),
    param: searchParam,
    view: () => view,
    commit,
    tell,
    team: currentTeam,
    setTeam,
    addCrew,
    joined: joinedTeam,
    fail: (code: string, tr: string, en: string) => new MockSocialError(code, pick(tr, en)),
  };
}
const wave2 = wave2Api(wave2Context());
const badges = badgesApi(badgesContext());
/** Crews whose leaders approve newcomers (with the second wave). */
const APPROVING = new Set(["fedcba9876543210"]);

const people: Record<string, CodeLookup["profile"]> = {
  "PRJ-4F8A-2KQX": {
    id: "601",
    name: "SessizOkcu",
    username: "sessizokcu",
    avatar: "preset:lantern",
  },
  "PRJ-9WQ2-XM4C": {
    id: "305",
    name: "NetherKral",
    username: "netherkral",
    avatar: "preset:axolotl",
  },
};

export const mockSocial: SocialApi = {
  lookupCrewLink: wave2.lookupCrewLink,
  joinCrewLink: wave2.joinCrewLink,
  withdrawTeamRequest: wave2.withdrawTeamRequest,
  answerTeamRequest: wave2.answerTeamRequest,
  setTeamLink: wave2.setTeamLink,
  setJoinMode: wave2.setJoinMode,
  makePlan: wave2.makePlan,
  setPlanNote: wave2.setPlanNote,
  answerPlan: wave2.answerPlan,
  cancelPlan: wave2.cancelPlan,
  setTogether: wave2.setTogether,
  clearTogether: wave2.clearTogether,
  setBadge: badges.setBadge,
  setBadgeSettings: badges.setBadgeSettings,
  requestFromBadge: badges.requestFromBadge,
  greetNewcomer: badges.greetNewcomer,
  onTagOff: badges.onTagOff,
  async state() {
    await delay(120);
    return view;
  },
  onView(listener) {
    viewListeners.add(listener);
    return () => viewListeners.delete(listener);
  },
  onNotice(listener) {
    noticeListeners.add(listener);
    return () => noticeListeners.delete(listener);
  },
  ...chat.api,
  async report(target, _reason, _note, block) {
    await delay(500);
    if (searchParam("reports") === "limit") {
      throw new MockSocialError(
        "SOSYAL-SINIR",
        pick(
          "Bugün çok rapor gönderdin. Yarın tekrar dene.",
          "You've sent a lot of reports today. Try again tomorrow.",
        ),
      );
    }
    if (block && target.kind === "player") await mockSocial.block(target.id);
  },
  async noticesSeen(ids) {
    await delay(120);
    commit((v) => ({ ...v, notices: v.notices.filter((notice) => !ids.includes(notice.id)) }));
  },
  async signIn() {
    await delay(300);
    commit((v) => ({ ...v, auth: "signingIn" }));
    // The browser step, then the deep link.
    setTimeout(() => {
      const name = scenario();
      if (name === "restricted" || name === "banned" || name === "signupsClosed") {
        // The service turns the sign-in down at the end of the browser step.
        commit((v) => ({ ...v, auth: "signedOut" }));
        tell({
          kind: "signInFailed",
          reason: name === "signupsClosed" ? "signupsClosed" : "restricted",
        });
        return;
      }
      const signed = chat.prepare(initial());
      commit(() => ({
        ...signed,
        auth: "signedIn",
        connection: "online",
        loaded: false,
        friends: [],
        incoming: [],
        outgoing: [],
        invites: [],
        enabled: true,
      }));
      tell({ kind: "signedIn", restore: backup });
      setTimeout(() => {
        const full = initial();
        commit((v) => ({
          ...v,
          me: full.me ?? v.me,
          loaded: true,
          friends: friendsList(),
          incoming: full.incoming,
          outgoing: full.outgoing,
          blocked: full.blocked,
        }));
      }, 900);
    }, 2600);
    return "https://social.projects.gg/auth/discord/start?challenge=preview";
  },
  async cancelSignIn() {
    commit((v) => ({ ...v, auth: "signedOut" }));
  },
  async signOut() {
    await delay(200);
    commit((v) => signedOut(v));
  },
  async refresh() {
    await delay(700);
    commit((v) => ({ ...v, connection: "online", friends: friendsList() }));
  },
  async lookup(code) {
    await delay(500);
    if (scenario() === "limited") {
      throw new MockSocialError(
        "SOSYAL-SINIRLI",
        pick(
          "Discord hesabın yeni olduğu için bugün daha fazla kod deneyemezsin.",
          "Your Discord account is new, so you can't try more codes today.",
        ),
      );
    }
    const normalized = code.toUpperCase();
    if (normalized === view.me?.friendCode) {
      return {
        profile: {
          id: "100",
          name: view.me.name,
          username: view.me.username,
          avatar: view.me.avatar,
        },
        relation: "self",
        code: normalized,
      };
    }
    const person = people[normalized];
    if (!person) throw codeNotFound();
    const relation = view.friends.some((f) => f.id === person.id) ? "friend" : "none";
    return { profile: person, relation, code: normalized };
  },
  async sendRequest(code) {
    await delay(450);
    const person = people[code.toUpperCase()];
    if (!person) throw codeNotFound();
    const request: FriendRequest = { ...person, sentAt: now() };
    commit((v) => ({ ...v, outgoing: [request, ...v.outgoing] }));
    return "requested";
  },
  async requestCrewmate(userId) {
    await delay(450);
    const member = view.crews.flatMap((c) => c.members).find((m) => m.id === userId && !m.me);
    if (!member || member.blocked) {
      throw new MockSocialError(
        "SOSYAL-EKIP-ARKADAS",
        pick(
          "Bu oyuncuya şu an istek gönderilemiyor.",
          "You can't send this player a request right now.",
        ),
      );
    }
    // They had asked first: friends at once, as by code.
    if (view.incoming.some((r) => r.id === userId)) {
      await mockSocial.accept(userId);
      commit((v) => ({
        ...v,
        crews: v.crews.map((c) => ({
          ...c,
          members: c.members.map((m) => (m.id === userId ? { ...m, friend: true } : m)),
        })),
      }));
      return "friends";
    }
    const request: FriendRequest = {
      id: member.id,
      name: member.name,
      username: member.username,
      avatar: member.avatar,
      avatarUrl: member.avatarUrl,
      sentAt: now(),
    };
    commit((v) => ({ ...v, outgoing: [request, ...v.outgoing.filter((r) => r.id !== userId)] }));
    return "requested";
  },
  async accept(id) {
    await delay(300);
    commit((v) => {
      const request = v.incoming.find((r) => r.id === id);
      if (!request) return v;
      const friend: Friend = {
        ...request,
        state: "launcher",
        dnd: false,
        discordUrl: `https://discord.com/users/${id}`,
      };
      return {
        ...v,
        incoming: v.incoming.filter((r) => r.id !== id),
        friends: [...v.friends, friend],
      };
    });
  },
  async decline(id) {
    await delay(250);
    commit((v) => ({ ...v, incoming: v.incoming.filter((r) => r.id !== id) }));
  },
  async cancelRequest(id) {
    await delay(250);
    commit((v) => ({ ...v, outgoing: v.outgoing.filter((r) => r.id !== id) }));
  },
  async removeFriend(id) {
    await delay(250);
    commit((v) => ({ ...v, friends: v.friends.filter((f) => f.id !== id) }));
  },
  async block(id) {
    await delay(250);
    commit((v) => {
      const who =
        v.friends.find((f) => f.id === id) ??
        v.incoming.find((r) => r.id === id) ??
        v.outgoing.find((r) => r.id === id);
      return {
        ...v,
        friends: v.friends.filter((f) => f.id !== id),
        incoming: v.incoming.filter((r) => r.id !== id),
        outgoing: v.outgoing.filter((r) => r.id !== id),
        invites: v.invites.filter((i) => i.from !== id),
        blocked: who
          ? [
              {
                id: who.id,
                name: who.name,
                username: who.username,
                avatar: who.avatar,
                avatarUrl: who.avatarUrl,
              },
              ...v.blocked,
            ]
          : v.blocked,
      };
    });
  },
  async unblock(id) {
    await delay(250);
    commit((v) => ({ ...v, blocked: v.blocked.filter((b) => b.id !== id) }));
  },
  async rotateCode() {
    await delay(500);
    const code = "PRJ-R2N8-HW6P";
    commit((v) =>
      v.me
        ? {
            ...v,
            me: {
              ...v.me,
              friendCode: code,
              shareLink: `https://social.projects.gg/f/${code}`,
              qr: QR_ROTATED,
            },
          }
        : v,
    );
    return code;
  },
  // Like the launcher (SOC-LAG): the view shows a profile change at once, the answer comes later.
  async setStatus(status: StatusMode) {
    commit((v) => (v.me ? { ...v, me: { ...v.me, status } } : v));
    await delay(200);
  },
  async setPrivacy(change) {
    commit((v) => (v.me ? { ...v, me: { ...v.me, privacy: { ...v.me.privacy, ...change } } } : v));
    await delay(200);
  },
  async setAvatar(avatar) {
    commit((v) =>
      v.me
        ? {
            ...v,
            me: {
              ...v.me,
              avatar,
              avatarUrl: avatar === "discord" ? v.me.discordAvatarUrl : undefined,
            },
          }
        : v,
    );
    await delay(200);
  },
  async invite(id) {
    await delay(400);
    const friend = view.friends.find((f) => f.id === id);
    return friend && friend.state !== "offline" && !friend.dnd ? "delivered" : "queued";
  },
  async dismissInvite(id) {
    commit((v) => ({ ...v, invites: v.invites.filter((i) => i.id !== id) }));
  },
  async openInDiscord() {
    // A browser has no Discord app to hand the profile to: the web profile opens instead.
    await delay(60);
    return false;
  },
  async sessions(): Promise<SocialSession[]> {
    await delay(500);
    return [
      {
        id: "a1b2c3d4e5f60718",
        device: "MASAÜSTÜ-OYUN",
        createdOn: "2026-10-12",
        lastUsedOn: "2026-10-16",
        current: true,
      },
      {
        id: "0f1e2d3c4b5a6978",
        device: "LAPTOP-EV",
        createdOn: "2026-09-30",
        lastUsedOn: "2026-10-13",
        current: false,
      },
      {
        id: "9a8b7c6d5e4f3021",
        device: "OKUL-PC",
        createdOn: "2026-09-02",
        lastUsedOn: "2026-09-21",
        current: false,
      },
    ];
  },
  async revokeSession() {
    await delay(300);
  },
  async revokeOtherSessions() {
    await delay(500);
    return 2;
  },
  async deleteAccount() {
    await delay(800);
    commit((v) => signedOut(v));
  },
  async uploadBackup() {
    await delay(300);
    const meta: BackupMeta = { version: 7, updatedAt: now(), device: "MASAÜSTÜ-OYUN", size: 812 };
    commit((v) => (v.me ? { ...v, me: { ...v.me, backup: meta } } : v));
    return meta;
  },
  async downloadBackup() {
    await delay(500);
    return { v: 1, prefs: { sounds: true, particles: true, closeBehavior: "tray" } };
  },
  async deleteBackup() {
    await delay(300);
    commit((v) => (v.me ? { ...v, me: { ...v.me, backup: undefined } } : v));
  },
  async saveGroups(groups, members) {
    // Shown at once, like the app; the "save" takes a moment.
    commit((v) => ({ ...v, groups: { version: v.groups?.version ?? 0, groups, members } }));
    await delay(250);
    commit((v) =>
      v.groups ? { ...v, groups: { ...v.groups, version: v.groups.version + 1 } } : v,
    );
  },
  async createTeam({ name, tag, emblem, color, frame, motto }) {
    await delay(500);
    const me = view.me;
    addCrew({
      ...previewTeam(true),
      id: `abcdef01234567${String(view.crews.length).padStart(2, "0")}`,
      name,
      tag,
      emblem,
      color: color ?? "gold",
      frame: frame ?? "cut",
      motto: motto ?? "",
      code: "EKP-3HXN-8RWE",
      createdOn: new Date().toISOString().slice(0, 10),
      members: previewTeam(true).members.filter((m) => m.me && me),
      invited: [],
    });
  },
  async editTeam(teamId, { permissions, ...change }) {
    await delay(300);
    currentTeam(teamId);
    setTeam(teamId, (team) => ({
      ...team,
      ...change,
      permissions: { ...team.permissions, ...permissions },
    }));
  },
  async setModerator(teamId, memberId, moderator) {
    await delay(250);
    setTeam(teamId, (team) => ({
      ...team,
      members: team.members.map((m) => (m.id === memberId ? { ...m, moderator } : m)),
    }));
  },
  async callCrew(crews) {
    await delay(300);
    const named = crews.length ? crews : view.crews.slice(0, 1).map((crew) => crew.id);
    for (const id of named) currentTeam(id);
    if (view.callPlace !== "ok") return { status: view.callPlace };
    if (view.calls.some((c) => c.mine && c.endsAt > now())) return { status: "active" };
    const call: CrewCall = {
      ...previewCall("mine", view.crew.multi ? named : []),
      server:
        view.activity.state === "playing" && view.activity.host !== "play.projects.gg"
          ? { where: "other", host: view.activity.host }
          : { where: "projects", host: "play.projects.gg" },
      startedAt: now(),
      endsAt: now() + 20 * 60,
      answers: [],
    };
    commit((v) => ({ ...v, calls: [...v.calls, call] }));
    return { status: "called" };
  },
  async answerCall(callId, answer) {
    await delay(250);
    if (!view.calls.some((c) => c.id === callId)) return false;
    commit((v) => ({
      ...v,
      calls: v.calls.map((c) =>
        c.id === callId
          ? {
              ...c,
              myAnswer: answer,
              answers: [...c.answers.filter((a) => a.id !== "100"), { id: "100", answer }],
            }
          : c,
      ),
    }));
    return true;
  },
  async endCall(callId) {
    await delay(250);
    commit((v) => ({ ...v, calls: v.calls.filter((c) => c.id !== callId) }));
  },
  async setCrewNotifications(change) {
    await delay(250);
    const current = view.crewNotifications;
    if (!current) {
      throw new MockSocialError(
        "SOSYAL",
        pick(
          "Bir sorun oluştu. Biraz sonra tekrar dene.",
          "Something went wrong. Try again in a bit.",
        ),
      );
    }
    const muted = change.muted ?? current.muted;
    commit((v) => ({
      ...v,
      crewNotifications: {
        calls: change.calls ?? current.calls,
        muted,
        muteUntil: !muted ? null : change.muted ? (change.muteUntil ?? null) : current.muteUntil,
      },
    }));
  },
  async setCrewMute(teamId, change) {
    await delay(250);
    currentTeam(teamId);
    setTeam(teamId, (team) => ({
      ...team,
      mute: change.muted
        ? { muted: true, muteUntil: change.muteUntil ?? null }
        : { muted: false, muteUntil: null },
    }));
  },
  async orderCrews(order) {
    await delay(250);
    commit((v) => ({
      ...v,
      crews: order
        .map((id) => v.crews.find((crew) => crew.id === id))
        .filter((crew): crew is SocialTeam => Boolean(crew)),
    }));
  },
  async disbandTeam(teamId) {
    await delay(400);
    dropCrew(teamId);
  },
  async leaveTeam(teamId) {
    await delay(400);
    dropCrew(teamId);
  },
  async rotateTeamCode(teamId) {
    await delay(400);
    currentTeam(teamId);
    const code = "EKP-R2N8-HW6P";
    setTeam(teamId, (team) => ({ ...team, code }));
    return code;
  },
  async inviteToTeam(teamId, friendId) {
    await delay(350);
    setTeam(teamId, (team) => ({ ...team, invited: [...new Set([...team.invited, friendId])] }));
  },
  async removeTeamMember(teamId, memberId) {
    await delay(350);
    setTeam(teamId, (team) => ({
      ...team,
      code: "EKP-K7MD-2QPC",
      members: team.members.filter((m) => m.id !== memberId),
    }));
  },
  async transferTeam(teamId, memberId) {
    await delay(350);
    setTeam(teamId, (team) => ({
      ...team,
      role: "member",
      moderator: true,
      members: team.members.map((m) => ({
        ...m,
        role: m.id === memberId ? "founder" : "member",
        moderator: m.me ? true : m.id === memberId ? false : m.moderator,
      })),
    }));
  },
  async lookupTeam(code) {
    await delay(500);
    const normalized = code.toUpperCase();
    const team = view.crews.find((crew) => crew.code === normalized);
    if (team) {
      return {
        teamId: team.id,
        name: team.name,
        tag: team.tag,
        emblem: team.emblem,
        color: team.color,
        frame: team.frame,
        motto: team.motto,
        members: team.members.length,
        relation: "member",
        code: normalized,
        join: team.join,
      };
    }
    const found = previewTeams[normalized];
    if (!found) {
      throw new MockSocialError(
        "SOSYAL-TAKIM-YOK",
        pick("Bu koda ait bir ekip yok.", "No crew has this code."),
      );
    }
    return {
      ...found,
      members: 6,
      relation: view.crew.multi
        ? view.crews.length >= view.crewLimit
          ? "limit"
          : "none"
        : view.crews.length
          ? "elsewhere"
          : "none",
      code: normalized,
      join: view.crew.requests && APPROVING.has(found.teamId) ? "approval" : "open",
      requestedAt: view.teamRequests.find((r) => r.teamId === found.teamId)?.sentAt,
    };
  },
  async joinTeam(code) {
    await delay(500);
    const found = previewTeams[code.toUpperCase()];
    if (!found) {
      throw new MockSocialError(
        "SOSYAL-TAKIM-YOK",
        pick("Bu koda ait bir ekip yok.", "No crew has this code."),
      );
    }
    return wave2.join(found, APPROVING.has(found.teamId));
  },
  async acceptTeamInvite(teamId) {
    await delay(450);
    const invite = view.teamInvites.find((i) => i.teamId === teamId);
    if (invite) addCrew(joinedTeam(invite));
    commit((v) => ({
      ...v,
      teamInvites: v.teamInvites.filter((i) => i.teamId !== teamId),
      teamRequests: v.teamRequests.filter((r) => r.teamId !== teamId),
    }));
  },
  async declineTeamInvite(teamId) {
    await delay(250);
    commit((v) => ({ ...v, teamInvites: v.teamInvites.filter((i) => i.teamId !== teamId) }));
  },
};
