// Browser preview of the friends feature: every state the panel can be in, with friends in the
// launcher, on projects.gg, in singleplayer, on another server and offline, requests both ways,
// blocks and invites. Actions change the data like the app would.
//
// Preview switch (read once, from the page URL): `?social=<scenario>`
// - (none) / `friends`: signed in, a lively list, requests and a quiet invite;
// - `off`: the feature is off (nothing shows); `signedOut`: the opt-in screen;
// - `signingIn`: waiting for the browser; `loading`: skeletons; `empty`: no friends yet;
// - `down`: the service is unavailable; `replaced`: another device took over;
// - `invite`: a live invite arrives shortly after start; `restore`: the "getireyim mi?" offer;
// - `groups`: the friend list in the player's categories;
// - `team`: teams on, the player founded a team (with categories); `teamMember`: a member of one;
// - `teamNew`: teams on, no team yet, an invite to one waiting; `teamInvite`: one arrives live.
import { pick } from "./mock-lang";
import type {
  BackupMeta,
  CodeLookup,
  Friend,
  FriendGroups,
  FriendRequest,
  SocialApi,
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
  return [
    {
      id: "301",
      name: "Ayşe",
      username: "aysekaya",
      avatar: "preset:fox",
      state: "playing",
      dnd: false,
      game: { where: "projects", host: "play.projects.gg" },
      discordUrl: "https://discord.com/users/301",
      since: "2026-09-02",
    },
    {
      id: "302",
      name: "KorAteşi",
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
      name: "Mert",
      username: "mert.yildiz",
      avatar: "preset:knight",
      state: "playing",
      dnd: false,
      game: { where: "singleplayer" },
      discordUrl: "https://discord.com/users/303",
      since: "2026-07-19",
    },
    {
      id: "304",
      name: "Deniz",
      username: "denizz",
      avatar: "preset:wizard",
      state: "playing",
      dnd: false,
      game: { where: "other" },
      discordUrl: "https://discord.com/users/304",
    },
    {
      id: "305",
      name: "Elif Su",
      username: "elifsu",
      avatar: "preset:axolotl",
      state: "launcher",
      dnd: false,
      discordUrl: "https://discord.com/users/305",
    },
    {
      id: "306",
      name: "Bora",
      username: "bora_mc",
      avatar: "discord",
      avatarUrl: discord(0),
      state: "offline",
      dnd: false,
      lastSeen: "today",
      discordUrl: "https://discord.com/users/306",
    },
    {
      id: "307",
      name: "Zeynep",
      username: "zeyno",
      avatar: "preset:wolf",
      state: "offline",
      dnd: false,
      lastSeen: "yesterday",
      discordUrl: "https://discord.com/users/307",
    },
    {
      id: "308",
      name: "Can Ejder",
      username: "canejder",
      avatar: "preset:ember-dragon",
      state: "offline",
      dnd: false,
      lastSeen: "week",
      discordUrl: "https://discord.com/users/308",
    },
    {
      id: "309",
      name: "Selin",
      username: "selin.ak",
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
    me: false,
    friend: Boolean(friend),
    blocked: false,
    state: "offline",
    dnd: false,
    discordUrl: `https://discord.com/users/${extra.id}`,
    joinedAt: now() - 86_400 * 20,
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
    ...extra,
  };
}

/** "Gece Kartalları", founded by the player (or led by Ayşe in `teamMember`). */
function previewTeam(founder: boolean): SocialTeam {
  const friends = friendsList();
  const byId = (id: string) => friends.find((f) => f.id === id);
  return {
    id: "0123456789abcdef",
    name: "Gece Kartalları",
    tag: "GKR",
    emblem: "shield",
    code: "TKM-7Q2X-9KDA",
    createdOn: "2026-09-28",
    role: founder ? "founder" : "member",
    members: [
      teamMember(undefined, {
        id: "100",
        name: "EjderAvcısı",
        avatar: "preset:golden-dragon",
        me: true,
        role: founder ? "founder" : "member",
        state: "launcher",
        joinedAt: now() - 86_400 * 21,
      }),
      teamMember(byId("301"), { id: "301", name: "Ayşe", role: founder ? "member" : "founder" }),
      teamMember(byId("302"), { id: "302", name: "KorAteşi" }),
      teamMember(undefined, {
        id: "701",
        name: "Yusuf",
        avatar: "preset:knight",
        state: "playing",
        game: { where: "projects", host: "play.projects.gg" },
      }),
      teamMember(undefined, {
        id: "702",
        name: "Melis",
        avatar: "preset:axolotl",
        state: "launcher",
      }),
      teamMember(byId("306"), { id: "306", name: "Bora" }),
      teamMember(undefined, { id: "703", name: "Arda", avatar: "preset:wolf" }),
    ],
    invited: ["305"],
    limit: 30,
  };
}

const teamInvite = (): TeamInvite => ({
  teamId: "fedcba9876543210",
  name: "Skyblock Ustaları",
  tag: "SKY",
  emblem: "cube",
  members: 6,
  from: { id: "303", name: "Mert", username: "mert.yildiz", avatar: "preset:knight" },
  sentAt: now() - 1_800,
});

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
      name: "EjderAvcısı",
      username: "ejderavcisi",
      avatar: "preset:golden-dragon",
      discordAvatarUrl: discord(1),
      friendCode: "PRJ-7K3M-Q9TD",
      shareLink: "https://social.projects.gg/f/PRJ-7K3M-Q9TD",
      qr: QR_FIRST,
      status: "online",
      privacy: { shareServer: false, shareLastSeen: true },
      backup,
    },
    friends: friendsList(),
    incoming: [
      {
        id: "401",
        name: "Yiğit",
        username: "yigitmc",
        avatar: "preset:baby-dragon",
        sentAt: now() - 600,
      },
    ],
    outgoing: [
      {
        id: "402",
        name: "Ece",
        username: "ece.tr",
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
        name: "KorAteşi",
        avatar: "discord",
        avatarUrl: discord(2),
        at: now() - 900,
        server: { where: "projects", host: "play.projects.gg" },
        quiet: true,
      },
    ],
    activity: { state: "launcher" },
    groups: { version: 0, groups: [], members: {} },
    teams: false,
    teamInvites: [],
  };
  switch (name) {
    case "groups":
      return { ...base, groups: previewGroups() };
    case "team":
      return { ...base, groups: previewGroups(), teams: true, team: previewTeam(true) };
    case "teamMember":
      return { ...base, groups: previewGroups(), teams: true, team: previewTeam(false) };
    case "teamNew":
      return { ...base, teams: true, teamInvites: [teamInvite()] };
    case "teamInvite":
      return { ...base, teams: true };
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
    default:
      return base;
  }
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
    team: undefined,
    teamInvites: [],
  };
}

let view = initial();
const viewListeners = new Set<(view: SocialSnapshot) => void>();
const noticeListeners = new Set<(notice: SocialNotice) => void>();

function commit(change: (draft: SocialSnapshot) => SocialSnapshot) {
  view = { ...change(view), seq: view.seq + 1 };
  for (const listener of viewListeners) listener(view);
}

function tell(notice: SocialNotice) {
  for (const listener of noticeListeners) listener(notice);
}

if (typeof window !== "undefined") {
  const name = scenario();
  if (name === "invite") {
    setTimeout(() => {
      const invite = {
        id: "i-live",
        from: "301",
        name: "Ayşe",
        avatar: "preset:fox",
        at: now(),
        server: { where: "projects" as const, host: "play.projects.gg" },
        quiet: false,
      };
      commit((v) => ({ ...v, invites: [invite, ...v.invites] }));
      tell({ kind: "invite", invite });
    }, 1600);
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
}

/** The team as it is, or an error worded like the backend when there is none. */
function currentTeam(): SocialTeam {
  if (!view.team) {
    throw new MockSocialError("SOSYAL-TAKIM-YOK", pick("Takımın yok.", "You're not in a team."));
  }
  return view.team;
}

function setTeam(change: (team: SocialTeam) => SocialTeam | undefined) {
  commit((v) => (v.team ? { ...v, team: change(v.team) } : v));
}

/** Team codes the preview knows. */
const previewTeams: Record<string, { teamId: string; name: string; tag: string; emblem: string }> =
  {
    "TKM-4F8A-2KQX": {
      teamId: "1111222233334444",
      name: "Gece Kartalları",
      tag: "GKR",
      emblem: "shield",
    },
    "TKM-9WQ2-XM4C": {
      teamId: "fedcba9876543210",
      name: "Skyblock Ustaları",
      tag: "SKY",
      emblem: "cube",
    },
  };

function joinedTeam(source: {
  teamId: string;
  name: string;
  tag: string;
  emblem: string;
}): SocialTeam {
  const base = previewTeam(false);
  return {
    ...base,
    id: source.teamId,
    name: source.name,
    tag: source.tag,
    emblem: source.emblem,
    code: "TKM-WXYZ-1234",
    members: base.members.map((m) => (m.me ? { ...m, joinedAt: now() } : m)),
    invited: [],
  };
}

const people: Record<string, CodeLookup["profile"]> = {
  "PRJ-4F8A-2KQX": { id: "601", name: "Kerem", username: "keremtr", avatar: "preset:lantern" },
  "PRJ-9WQ2-XM4C": { id: "305", name: "Elif Su", username: "elifsu", avatar: "preset:axolotl" },
};

export const mockSocial: SocialApi = {
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
  async signIn() {
    await delay(300);
    commit((v) => ({ ...v, auth: "signingIn" }));
    // The browser step, then the deep link.
    setTimeout(() => {
      const signed = initial();
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
  async setStatus(status: StatusMode) {
    await delay(200);
    commit((v) => (v.me ? { ...v, me: { ...v.me, status } } : v));
  },
  async setPrivacy(change) {
    await delay(200);
    commit((v) => (v.me ? { ...v, me: { ...v.me, privacy: { ...v.me.privacy, ...change } } } : v));
  },
  async setAvatar(avatar) {
    await delay(200);
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
  },
  async invite(id) {
    await delay(400);
    const friend = view.friends.find((f) => f.id === id);
    return friend && friend.state !== "offline" && !friend.dnd ? "delivered" : "queued";
  },
  async dismissInvite(id) {
    commit((v) => ({ ...v, invites: v.invites.filter((i) => i.id !== id) }));
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
  async createTeam({ name, tag, emblem }) {
    await delay(500);
    const me = view.me;
    commit((v) => ({
      ...v,
      team: {
        ...previewTeam(true),
        id: "abcdef0123456789",
        name,
        tag,
        emblem,
        code: "TKM-3HXN-8RWE",
        createdOn: new Date().toISOString().slice(0, 10),
        members: previewTeam(true).members.filter((m) => m.me && me),
        invited: [],
      },
    }));
  },
  async editTeam(change) {
    await delay(300);
    currentTeam();
    setTeam((team) => ({ ...team, ...change }));
  },
  async disbandTeam() {
    await delay(400);
    currentTeam();
    commit((v) => ({ ...v, team: undefined }));
  },
  async leaveTeam() {
    await delay(400);
    currentTeam();
    commit((v) => ({ ...v, team: undefined }));
  },
  async rotateTeamCode() {
    await delay(400);
    currentTeam();
    const code = "TKM-R2N8-HW6P";
    setTeam((team) => ({ ...team, code }));
    return code;
  },
  async inviteToTeam(friendId) {
    await delay(350);
    setTeam((team) => ({ ...team, invited: [...new Set([...team.invited, friendId])] }));
  },
  async removeTeamMember(memberId) {
    await delay(350);
    setTeam((team) => ({
      ...team,
      code: "TKM-K7MD-2QPC",
      members: team.members.filter((m) => m.id !== memberId),
    }));
  },
  async transferTeam(memberId) {
    await delay(350);
    setTeam((team) => ({
      ...team,
      role: "member",
      members: team.members.map((m) => ({
        ...m,
        role: m.id === memberId ? "founder" : "member",
      })),
    }));
  },
  async lookupTeam(code) {
    await delay(500);
    const normalized = code.toUpperCase();
    if (view.team?.code === normalized) {
      const team = view.team;
      return {
        teamId: team.id,
        name: team.name,
        tag: team.tag,
        emblem: team.emblem,
        members: team.members.length,
        relation: "member",
        code: normalized,
      };
    }
    const found = previewTeams[normalized];
    if (!found) {
      throw new MockSocialError(
        "SOSYAL-TAKIM-YOK",
        pick("Bu koda ait bir takım yok.", "No team has this code."),
      );
    }
    return {
      ...found,
      members: 6,
      relation: view.team ? "elsewhere" : "none",
      code: normalized,
    };
  },
  async joinTeam(code) {
    await delay(500);
    const found = previewTeams[code.toUpperCase()];
    if (!found) {
      throw new MockSocialError(
        "SOSYAL-TAKIM-YOK",
        pick("Bu koda ait bir takım yok.", "No team has this code."),
      );
    }
    commit((v) => ({ ...v, team: joinedTeam(found) }));
  },
  async acceptTeamInvite(teamId) {
    await delay(450);
    const invite = view.teamInvites.find((i) => i.teamId === teamId);
    commit((v) => ({
      ...v,
      teamInvites: v.teamInvites.filter((i) => i.teamId !== teamId),
      team: invite ? joinedTeam(invite) : v.team,
    }));
  },
  async declineTeamInvite(teamId) {
    await delay(250);
    commit((v) => ({ ...v, teamInvites: v.teamInvites.filter((i) => i.teamId !== teamId) }));
  },
};
