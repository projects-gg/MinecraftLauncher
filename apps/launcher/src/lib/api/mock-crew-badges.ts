// Browser preview of crew tags (TEAM6), on mock-social.ts's data: tags before names, their cards,
// hidden crews, requests from a tag's card and the crew chat's "Hoş geldin" line. The switches (from
// the page URL, see mock-social.ts):
// - `badges`: several crews; the player wears "GKR". Friends wear tags of the player's crews (Ayşe
//   "SKY", KorAteşi "GKR": "Sen de bu ekiptesin"), of a crew that takes requests (Mert "YLD":
//   "Katılma isteği gönder"), of a full one (Deniz "RSM": no row), of one that invited the player
//   (Elif Su "GBY": "Kabul et") and of one the player asked already (Bora "ATL": "İsteğin
//   bekliyor"); a crewmate who is no friend wears "LAV" (no row). "PvP Kulübü" is hidden. Kerem asked
//   to join "Gece Kartalları" through Ayşe's tag ("Ayşe aracılığıyla"); a plan tonight has answers;
// - `badgesLive`: the same, and tags change live (Zeynep puts one on, Ayşe switches, Mert takes his
//   off; `&liveAt=<ms>`);
// - `badgesHidden`: the player wears "PVP", and its leader hides the crew live (the toast);
// - `badgesJoined`: Ece joins "Gece Kartalları" live (its chat's "Hoş geldin" line, with
//   `&chat2=1` the "Selam" sticker), and the player's own arrival shows in "Skyblock Ustaları"'s
//   chat;
// - `&badges=0`: a service from before TEAM6 (nothing of it shows); `&requests=0`: without requests
//   from tags; `&refuse=closed|today|waitingFull|full|blocked|ownWaiting|ownCrews|ownDaily`: a
//   request from a card is refused that way.
import { turkeyDayEnd } from "@/features/social/badges";
import type {
  BadgeRefusal,
  CrewBadge,
  CrewPlan,
  JoinRequest,
  SocialApi,
  SocialSnapshot,
  SocialTeam,
  TeamInvite,
  TeamRequest,
} from "./social-types";

const now = () => Math.floor(Date.now() / 1000);
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** The scenarios of this file. */
export const BADGE_SCENARIOS = ["badges", "badgesLive", "badgesHidden", "badgesJoined"];

/** What mock-social.ts lends this file. */
export interface BadgesContext {
  scenario: string;
  param: (key: string) => string | null;
  view: () => SocialSnapshot;
  commit: (change: (view: SocialSnapshot) => SocialSnapshot) => void;
  fail: (code: string, tr: string, en: string) => Error;
  /** Words in the preview's language. */
  pick: (tr: string, en: string) => string;
  /** The preview chat's sending (mock-chat.ts). */
  send: (conv: string, text: string) => Promise<unknown>;
  sendSticker: (conv: string, sticker: string) => Promise<unknown>;
}

/** Who listens for "Lider ekibi gizledi; etiketin çıkarıldı." */
const tagOffListeners = new Set<(off: { crew: string; name: string }) => void>();

/** "Selam ver" stays this long (seconds). */
const GREET_SECONDS = 600;

/** The crews' ids in mock-social.ts's `crews` preview. */
const GKR = "0123456789abcdef";
const SKY = "5555666677778888";
const PVP = "9999aaaabbbbcccc";

/** Crews outside the player's, as their tags' cards show them. */
function outside(pick: BadgesContext["pick"]): Record<string, CrewBadge> {
  return {
    yld: {
      crew: "7777000011112222",
      tag: "YLD",
      color: "navy",
      frame: "shield",
      emblem: "star",
      name: "Yıldız Avcıları",
      motto: pick("Gökyüzü bizim", "The sky is ours"),
      createdOn: "2026-09-14",
      members: 12,
      requests: true,
    },
    rsm: {
      crew: "8888000011112222",
      tag: "RSM",
      color: "copper",
      frame: "hex",
      emblem: "cube",
      name: "Redstone Ustaları",
      motto: "",
      createdOn: "2026-08-03",
      members: 30,
      requests: false,
    },
    gby: {
      crew: "2222333344445555",
      tag: "GBY",
      color: "blue",
      frame: "shield",
      emblem: "moon",
      name: "Gece Baykuşları",
      motto: pick("Geceleri daha iyiyiz", "Better at night"),
      createdOn: "2026-10-01",
      members: 6,
      requests: true,
    },
    atl: {
      crew: "aaaa000011112222",
      tag: "ATL",
      color: "teal",
      frame: "banner",
      emblem: "waves",
      name: "Atlantis",
      motto: pick("Derinlerde buluşalım", "Meet us in the deep"),
      createdOn: "2026-07-21",
      members: 18,
      requests: true,
    },
    lav: {
      crew: "bbbb000011112222",
      tag: "LAV",
      color: "crimson",
      frame: "cut",
      emblem: "flame",
      name: "Lav Yürüyüşçüleri",
      motto: pick("Sıcak seviyoruz", "We like it hot"),
      createdOn: "2026-06-30",
      members: 9,
      requests: true,
    },
  };
}

/** A crew's card, as the service makes it from the crew's row. */
export function badgeOf(team: SocialTeam): CrewBadge {
  return {
    crew: team.id,
    tag: team.tag,
    color: team.color,
    frame: team.frame,
    emblem: team.emblem,
    name: team.name,
    motto: team.motto,
    createdOn: team.createdOn,
    members: team.members.length,
    requests: (team.badgeRequests ?? true) && team.members.length < team.limit,
  };
}

/** Everyone's tag at once (the player's own too, in every crew they share). */
function withBadges(view: SocialSnapshot, wears: Record<string, CrewBadge | undefined>) {
  const of = (id: string, before: CrewBadge | undefined) => (id in wears ? wears[id] : before);
  const me = view.me?.id ?? "100";
  return {
    ...view,
    me: view.me ? { ...view.me, badge: of(me, view.me.badge) } : view.me,
    friends: view.friends.map((f) => ({ ...f, badge: of(f.id, f.badge) })),
    crews: view.crews.map((crew) => ({
      ...crew,
      members: crew.members.map((m) => ({ ...m, badge: of(m.id, m.badge) })),
    })),
  };
}

/** Who wears which tag now (the player, friends and crewmates). */
function wearersNow(view: SocialSnapshot): Map<string, CrewBadge | undefined> {
  const out = new Map<string, CrewBadge | undefined>();
  if (view.me) out.set(view.me.id, view.me.badge);
  for (const f of view.friends) out.set(f.id, f.badge);
  for (const crew of view.crews) for (const m of crew.members) out.set(m.id, m.badge);
  return out;
}

/** A crew's card changed (its setting, its texts): every wearer's card follows. */
function refreshCards(view: SocialSnapshot, crew: SocialTeam): SocialSnapshot {
  const card = badgeOf(crew);
  const wears: Record<string, CrewBadge | undefined> = {};
  for (const [id, badge] of wearersNow(view)) if (badge?.crew === crew.id) wears[id] = card;
  return withBadges(view, wears);
}

/** The scenario's starting data on top of mock-social's. */
export function badgesSnapshot(snapshot: SocialSnapshot, ctx: BadgesContext): SocialSnapshot {
  const on = BADGE_SCENARIOS.includes(ctx.scenario) && ctx.param("badges") !== "0";
  if (!on) return snapshot;
  const pick = ctx.pick;
  const other = outside(pick);
  const crews = snapshot.crews.map((crew) => ({
    ...crew,
    hidden: crew.id === PVP && ctx.scenario !== "badgesHidden",
    badgeRequests: true,
    // Kerem asked through Ayşe's tag; Zeynep by the crew's code.
    requests:
      crew.id === GKR
        ? ([
            {
              id: "601",
              name: "Kerem",
              username: "keremtr",
              avatar: "preset:lantern",
              sentAt: now() - 40 * 60,
              via: "301",
              viaName: "Ayşe",
            },
            {
              id: "604",
              name: "Zeynep",
              username: "zeynep.mc",
              avatar: "preset:fox",
              sentAt: now() - 3 * 3600,
            },
          ] satisfies JoinRequest[])
        : crew.requests,
  }));
  const card = (id: string) => {
    const crew = crews.find((c) => c.id === id);
    return crew ? badgeOf(crew) : undefined;
  };
  const invite: TeamInvite = {
    teamId: other.gby?.crew ?? "",
    name: "Gece Baykuşları",
    tag: "GBY",
    emblem: "moon",
    color: "blue",
    frame: "shield",
    motto: other.gby?.motto ?? "",
    members: 6,
    from: { id: "305", name: "Elif Su", username: "elifsu", avatar: "preset:axolotl" },
    sentAt: now() - 20 * 60,
  };
  const asked: TeamRequest = {
    teamId: other.atl?.crew ?? "",
    name: "Atlantis",
    tag: "ATL",
    emblem: "waves",
    color: "teal",
    frame: "banner",
    motto: other.atl?.motto ?? "",
    members: 18,
    sentAt: now() - 5 * 3600,
  };
  const tonight = Math.ceil((now() + 3 * 3600) / 900) * 900;
  const plan: CrewPlan = {
    id: "e9b2c3d4e5f60718",
    crew: GKR,
    by: "301",
    byName: "Ayşe",
    mine: false,
    canCancel: true,
    at: tonight,
    startsAt: tonight,
    endsAt: tonight + 7200,
    createdAt: now() - 3600,
    answers: [
      { id: "301", answer: "yes" },
      { id: "100", answer: "yes" },
      { id: "302", answer: "yes" },
      { id: "701", answer: "maybe" },
      { id: "702", answer: "no" },
    ],
    myAnswer: "yes",
    note: pick("Bedwars, sesli gelin", "Bedwars, bring voice"),
    canWriteNote: false,
    canClearNote: true,
  };
  const wearing = ctx.scenario === "badgesHidden" ? card(PVP) : card(GKR);
  const base: SocialSnapshot = {
    ...snapshot,
    crew: { ...snapshot.crew, badges: true, badgeRequests: ctx.param("requests") !== "0" },
    crews,
    teamInvites: [invite],
    teamRequests: [asked],
    plans: [plan, ...snapshot.plans],
  };
  return withBadges(base, {
    "100": wearing,
    "301": card(SKY),
    "302": card(GKR),
    "303": other.yld,
    "304": other.rsm,
    "305": other.gby,
    "306": other.atl,
    "701": card(GKR),
    "702": other.lav,
    "802": card(SKY),
  });
}

/** What the scenario does by itself once the preview runs. */
export function startBadges(ctx: BadgesContext) {
  if (typeof window === "undefined" || !BADGE_SCENARIOS.includes(ctx.scenario)) return;
  if (ctx.param("badges") === "0") return;
  const at = Number(ctx.param("liveAt")) || 1600;
  if (ctx.scenario === "badgesLive") {
    const other = outside(ctx.pick);
    const crew = (id: string) => ctx.view().crews.find((c) => c.id === id);
    setTimeout(() => ctx.commit((v) => withBadges(v, { "307": other.yld })), at);
    setTimeout(() => {
      const gkr = crew(GKR);
      if (gkr) ctx.commit((v) => withBadges(v, { "301": badgeOf(gkr) }));
    }, at + 2400);
    setTimeout(() => ctx.commit((v) => withBadges(v, { "303": undefined })), at + 4800);
  }
  if (ctx.scenario === "badgesHidden") {
    // Its leader hides "PvP Kulübü": every tag of it comes off (the player's own too).
    setTimeout(() => {
      const wore = ctx.view().me?.badge?.crew === PVP;
      ctx.commit((v) => {
        const wears: Record<string, CrewBadge | undefined> = {};
        for (const [id, badge] of wearersNow(v)) if (badge?.crew === PVP) wears[id] = undefined;
        return withBadges(
          { ...v, crews: v.crews.map((c) => (c.id === PVP ? { ...c, hidden: true } : c)) },
          wears,
        );
      });
      const crew = ctx.view().crews.find((c) => c.id === PVP);
      if (wore && crew)
        for (const listener of tagOffListeners) listener({ crew: PVP, name: crew.name });
    }, at);
  }
  if (ctx.scenario === "badgesJoined") {
    setTimeout(() => {
      const me = ctx.view().me?.id ?? "100";
      ctx.commit((v) => ({
        ...v,
        crews: v.crews.map((c) =>
          c.id === GKR
            ? {
                ...c,
                members: [
                  ...c.members,
                  {
                    id: "605",
                    name: "Ece",
                    username: "ece.tr",
                    avatar: "preset:axolotl",
                    role: "member" as const,
                    moderator: false,
                    me: false,
                    friend: false,
                    blocked: false,
                    state: "launcher" as const,
                    dnd: false,
                    discordUrl: "https://discord.com/users/605",
                    joinedAt: now(),
                    togetherMonth: 0,
                  },
                ],
              }
            : c,
        ),
        crewJoins: [
          ...(v.crewJoins ?? []),
          {
            crew: GKR,
            id: "605",
            name: "Ece",
            mine: false,
            at: now(),
            greetUntil: now() + GREET_SECONDS,
          },
          { crew: SKY, id: me, name: v.me?.name ?? "", mine: true, at: now() },
        ],
      }));
    }, at);
  }
}

type BadgesApi = Pick<
  SocialApi,
  "setBadge" | "setBadgeSettings" | "requestFromBadge" | "greetNewcomer" | "onTagOff"
>;

/** The API's TEAM6 part, on the preview's data. */
export function badgesApi(ctx: BadgesContext): BadgesApi {
  const team = (id: string) => ctx.view().crews.find((crew) => crew.id === id);
  return {
    async setBadge(teamId) {
      await delay(220);
      const me = ctx.view().me?.id ?? "100";
      if (teamId === null) {
        ctx.commit((v) => withBadges(v, { [me]: undefined }));
        return;
      }
      const crew = team(teamId);
      if (!crew) {
        throw ctx.fail(
          "SOSYAL-TAKIM-YOK",
          "Artık bu ekipte değilsin.",
          "You're no longer in this crew.",
        );
      }
      if (crew.hidden) {
        throw ctx.fail("SOSYAL-ETIKET-GIZLI", "Lider ekibi gizledi.", "The leader hid the crew.");
      }
      ctx.commit((v) => withBadges(v, { [me]: badgeOf(crew) }));
    },
    async setBadgeSettings(teamId, change) {
      await delay(260);
      const crew = team(teamId);
      if (crew?.role !== "founder") {
        throw ctx.fail(
          "SOSYAL-TAKIM-KURUCU",
          "Bunu yalnız ekibin lideri yapabilir.",
          "Only the crew's leader can do that.",
        );
      }
      ctx.commit((v) => {
        const changed = {
          ...crew,
          ...(change.hidden !== undefined ? { hidden: change.hidden } : {}),
          ...(change.badgeRequests !== undefined ? { badgeRequests: change.badgeRequests } : {}),
        };
        const next = { ...v, crews: v.crews.map((c) => (c.id === teamId ? changed : c)) };
        if (!changed.hidden) return refreshCards(next, changed);
        // Hiding takes every tag of it off (the player's own too).
        const wears: Record<string, CrewBadge | undefined> = {};
        for (const [id, badge] of wearersNow(next))
          if (badge?.crew === teamId) wears[id] = undefined;
        return withBadges(next, wears);
      });
    },
    async requestFromBadge(teamId, via) {
      await delay(520);
      const until = turkeyDayEnd(Date.now());
      // `&refuse=`: the crew's refusal (or the player's own limit) as the backend answers it.
      const reasons: readonly BadgeRefusal[] = [
        "closed",
        "today",
        "waitingFull",
        "full",
        "blocked",
        "ownWaiting",
        "ownCrews",
        "ownDaily",
        "member",
      ];
      const asked = ctx.param("refuse");
      const reason = reasons.find((r) => r === asked);
      if (reason) return { status: "refused", reason, until };
      const badge = wearersNow(ctx.view()).get(via);
      if (!badge || badge.crew !== teamId) return { status: "refused", reason: "closed", until };
      ctx.commit((v) =>
        v.teamRequests.some((r) => r.teamId === teamId)
          ? v
          : {
              ...v,
              teamRequests: [
                {
                  teamId,
                  name: badge.name,
                  tag: badge.tag,
                  emblem: badge.emblem,
                  color: badge.color,
                  frame: badge.frame,
                  motto: badge.motto,
                  members: badge.members,
                  sentAt: now(),
                },
                ...v.teamRequests,
              ],
            },
      );
      return { status: "sent" };
    },
    async greetNewcomer(crew, id) {
      const join = ctx.view().crewJoins?.find((j) => j.crew === crew && j.id === id);
      if (!join?.greetUntil || join.greetUntil <= now()) {
        throw ctx.fail(
          "SOSYAL-EKIP-SELAM",
          "Bu selam artık gönderilemiyor.",
          "This greeting can't be sent anymore.",
        );
      }
      // Once: the line keeps no button.
      ctx.commit((v) => ({
        ...v,
        crewJoins: (v.crewJoins ?? []).map((j) =>
          j.crew === crew && j.id === id ? { ...j, greetUntil: undefined } : j,
        ),
      }));
      const conv = `c:${crew}`;
      if (ctx.view().chat?.extras?.stickers) await ctx.sendSticker(conv, "pl1/hi");
      else await ctx.send(conv, ctx.pick("Hoş geldin! 👋", "Welcome! 👋"));
    },
    onTagOff(listener) {
      tagOffListeners.add(listener);
      return () => tagOffListeners.delete(listener);
    },
  };
}
