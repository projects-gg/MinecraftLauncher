// Browser preview of the crews' second wave (TEAM5), on mock-social.ts's data: the join setting
// and join requests, the invite link, plans and the time played together. The switches (from the
// page URL, see mock-social.ts):
// - `crewApproval`: the player leads "Gece Kartalları", which approves newcomers: two requests
//   wait, its invite link is on, two plans ahead (the first with answers), 6 h together this week;
//   `crewRequestLive`: the same, and a request arrives live (`&liveAt=<ms>`);
// - `crewPlans`: a member of it (Ayşe leads): a plan is under way, the player said "Geliyorum",
//   another is tomorrow; `crewPlanLive`: a crewmate makes one live;
// - `crewAsk`: several crews; an invite link to "Skyblock Ustaları" (approving) opens as
//   `projects://crew/…` would; `&asked=1`: the player's request to it waits already;
// - `crewAccepted`: the player's request waits and is taken shortly after start;
// - `&wave2=0` on any crew scenario: a service from before TEAM5 (none of it shows);
// - `&limited=1` on any crew scenario: a new Discord account (no invite links, no plans);
// - FIX2: plans carry notes ("Bedwars, sesli gelin"), written with the plan or later by its maker,
//   cleared by the crew's leaders (`&fix2=0`: a service from before notes); a note with "küfür"
//   in it is turned away as the service's word list would; `&crowd=1` puts 22 more crewmates in
//   the crew, most of them answering the first plan (its answers list pages).

import { cleanPlanNote, planNoteProblem } from "@/features/social/plan-note";
import type {
  CrewLook,
  CrewPlan,
  JoinMode,
  JoinRequest,
  JoinResult,
  PlanAnswer,
  SocialApi,
  SocialNotice,
  SocialSnapshot,
  SocialTeam,
  TeamLookup,
  TeamRequest,
} from "./social-types";

const now = () => Math.floor(Date.now() / 1000);
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const QUARTER = 900;

/** The scenarios of this file. */
export const WAVE2_SCENARIOS = [
  "crewApproval",
  "crewRequestLive",
  "crewPlans",
  "crewPlanLive",
  "crewAsk",
  "crewAccepted",
];

/** What mock-social.ts lends this file: its data and its ways to change it. */
export interface Wave2Context {
  scenario: string;
  param: (key: string) => string | null;
  view: () => SocialSnapshot;
  commit: (change: (view: SocialSnapshot) => SocialSnapshot) => void;
  tell: (notice: SocialNotice) => void;
  /** The crew as it is, or the backend's "not in a crew" error. */
  team: (teamId: string) => SocialTeam;
  setTeam: (teamId: string, change: (team: SocialTeam) => SocialTeam) => void;
  /** Adds a crew the player joined (at the limit, the backend's refusal). */
  addCrew: (team: SocialTeam) => void;
  /** A crew the player joined, as the service sends it. */
  joined: (
    source: CrewLook & { teamId: string; name: string; tag: string; motto: string },
  ) => SocialTeam;
  fail: (code: string, tr: string, en: string) => Error;
}

/** The service offers TEAM5 (`&wave2=0`: one from before it). */
export function wave2On(param: Wave2Context["param"]): boolean {
  return param("wave2") !== "0";
}

const at = (seconds: number) => Math.ceil(seconds / QUARTER) * QUARTER;

/** People who asked to join "Gece Kartalları". */
function previewRequests(): JoinRequest[] {
  return [
    {
      id: "601",
      name: "Kerem",
      username: "keremtr",
      avatar: "preset:lantern",
      sentAt: now() - 3 * 3600,
    },
    {
      id: "604",
      name: "Zeynep",
      username: "zeynep.mc",
      avatar: "preset:fox",
      sentAt: now() - 25 * 60,
    },
  ];
}

function plan(
  team: SocialTeam,
  id: string,
  start: number,
  by: { id: string; name: string },
  answers: Array<{ id: string; answer: PlanAnswer }>,
  note?: string,
  notes = true,
): CrewPlan {
  const me = team.members.find((m) => m.me)?.id ?? "100";
  const mine = by.id === me;
  const leads = team.role === "founder" || team.moderator;
  const myAnswer = answers.find((a) => a.id === me)?.answer;
  return {
    id,
    crew: team.id,
    by: by.id,
    byName: by.name,
    mine,
    canCancel: mine || leads,
    at: start,
    startsAt: start,
    endsAt: start + 7200,
    createdAt: Math.min(now() - 600, start - 3600),
    answers,
    myAnswer,
    note: notes ? note : undefined,
    canWriteNote: notes && mine,
    canClearNote: notes && (mine || leads),
  };
}

/** `&crowd=1`: 22 more crewmates, for a plan whose answers list pages (FIX2). */
const CROWD = [
  "Deniz",
  "Ece",
  "Mert",
  "Zeynep",
  "Kerem",
  "Elif",
  "Emir",
  "Defne",
  "Berk",
  "Ada",
  "Yiğit",
  "Duru",
  "Kaan",
  "Lina",
  "Arda",
  "Nehir",
  "Çınar",
  "Asya",
  "Ozan",
  "İpek",
  "Selin",
  "Toprak",
];
const CROWD_FACES = ["fox", "wolf", "knight", "axolotl", "lantern", "wizard", "golden-dragon"];

function crowded(team: SocialTeam): SocialTeam {
  const extra = CROWD.map((name, index) => ({
    id: String(900 + index),
    name,
    username: name.toLocaleLowerCase("tr"),
    avatar: `preset:${CROWD_FACES[index % CROWD_FACES.length] ?? "fox"}`,
    role: "member" as const,
    moderator: false,
    me: false,
    // A few are friends; the others are crewmates only ("Arkadaş ekle").
    friend: index % 5 === 0,
    blocked: false,
    state:
      index % 3 === 0
        ? ("playing" as const)
        : index % 3 === 1
          ? ("launcher" as const)
          : ("offline" as const),
    dnd: false,
    game: index % 3 === 0 ? { where: "projects" as const } : undefined,
    discordUrl: `https://discord.com/users/${900 + index}`,
    joinedAt: now() - (index + 2) * 86_400,
    togetherMonth: 0,
  }));
  return { ...team, members: [...team.members, ...extra], limit: 30 };
}

/** The crowd's answers to a plan: most come, some may, a few cannot. */
function crowdAnswers(): Array<{ id: string; answer: PlanAnswer }> {
  return CROWD.map((_, index) => ({
    id: String(900 + index),
    answer: index % 7 === 3 ? "no" : index % 4 === 1 ? "maybe" : "yes",
  }));
}

/** The plans of a scenario: tonight's and Saturday's (`crewApproval`), one under way and one
 * tomorrow (`crewPlans`); notes on some (FIX2), and the crowd's answers with `&crowd=1`. */
function previewPlans(
  scenario: string,
  crews: readonly SocialTeam[],
  notes: boolean,
  crowd: boolean,
): CrewPlan[] {
  const team = crews[0];
  if (!team) return [];
  const ayse = { id: "301", name: "Ayşe" };
  const me = { id: "100", name: "EjderAvcısı" };
  const more = crowd ? crowdAnswers() : [];
  if (scenario === "crewApproval" || scenario === "crewRequestLive") {
    const tonight = at(now() + 3 * 3600);
    return [
      plan(
        team,
        "a1b2c3d4e5f60718",
        tonight,
        me,
        [
          { id: "100", answer: "yes" },
          { id: "301", answer: "yes" },
          { id: "701", answer: "yes" },
          { id: "302", answer: "maybe" },
          { id: "703", answer: "no" },
          ...more,
        ],
        "Bedwars turnuvası, sesli gelin",
        notes,
      ),
      plan(
        team,
        "b1b2c3d4e5f60718",
        at(now() + 3 * 86_400),
        ayse,
        [{ id: "301", answer: "yes" }],
        undefined,
        notes,
      ),
    ];
  }
  if (scenario === "crewPlans" || scenario === "crewPlanLive") {
    const started = Math.floor((now() - 20 * 60) / QUARTER) * QUARTER;
    return [
      plan(
        team,
        "c1b2c3d4e5f60718",
        started,
        ayse,
        [
          { id: "301", answer: "yes" },
          { id: "100", answer: "yes" },
          { id: "701", answer: "yes" },
          { id: "702", answer: "maybe" },
          ...more,
        ],
        "Lobide buluşuyoruz, Skyblock adasına geçeceğiz",
        notes,
      ),
      plan(
        team,
        "d1b2c3d4e5f60718",
        at(now() + 26 * 3600),
        { id: "302", name: "KorAteşi" },
        [{ id: "302", answer: "yes" }],
        undefined,
        notes,
      ),
    ];
  }
  return [];
}

/** A crew as the second wave shows it: how newcomers get in, its link, who asked, time together. */
export function wave2Team(team: SocialTeam, scenario: string, on: boolean): SocialTeam {
  const base: SocialTeam = {
    ...team,
    permissions: { ...team.permissions, admit: team.permissions.admit ?? "moderators" },
    join: "open",
    link: on ? `${team.id.slice(0, 4).toUpperCase()}-HJKM-NPQR` : undefined,
    linkUrl: on
      ? `https://social.projects.gg/e/${team.id.slice(0, 4).toUpperCase()}-HJKM-NPQR`
      : undefined,
    requests: [],
    togetherWeek: 0,
  };
  if (!on) return base;
  const leads = team.role === "founder" || team.moderator;
  if (scenario === "crewApproval" || scenario === "crewRequestLive") {
    return {
      ...base,
      join: "approval",
      requests: leads ? previewRequests() : [],
      togetherWeek: 6 * 3600 + 20 * 60,
      members: base.members.map((m) =>
        m.id === "301"
          ? { ...m, togetherMonth: 11 * 3600 + 40 * 60 }
          : m.id === "701"
            ? { ...m, togetherMonth: 3 * 3600 + 5 * 60 }
            : m,
      ),
    };
  }
  if (scenario === "crewPlans" || scenario === "crewPlanLive") {
    return { ...base, togetherWeek: 2 * 3600 + 45 * 60 };
  }
  return base;
}

/** The waiting request of `crewAsk&asked=1` and `crewAccepted`. */
const SKYBLOCK: CrewLook & { teamId: string; name: string; tag: string; motto: string } = {
  teamId: "fedcba9876543210",
  name: "Skyblock Ustaları",
  tag: "SKY",
  emblem: "castle",
  color: "purple",
  frame: "hex",
  motto: "",
};

/** Invite links the preview knows: Skyblock's (approving) and a crew that lets anyone in. */
const LINKS: Record<
  string,
  CrewLook & { teamId: string; name: string; tag: string; motto: string; join: JoinMode }
> = {
  "ABCD-EFGH-JKMN": { ...SKYBLOCK, join: "approval" },
  "KMNP-QRST-VWXY": {
    teamId: "2222333344445555",
    name: "Gece Baykuşları",
    tag: "GBY",
    emblem: "moon",
    color: "blue",
    frame: "shield",
    motto: "",
    join: "open",
  },
};

/** The scenario's starting data on top of mock-social's. */
export function wave2Snapshot(
  snapshot: SocialSnapshot,
  ctx: Pick<Wave2Context, "scenario" | "param">,
) {
  const on = wave2On(ctx.param) && snapshot.crew.look;
  const notes = on && ctx.param("fix2") !== "0";
  const crowd = ctx.param("crowd") !== null;
  const crews = snapshot.crews.map((team, index) => {
    const second = wave2Team(team, ctx.scenario, on);
    return crowd && index === 0 ? crowded(second) : second;
  });
  const waits =
    (ctx.scenario === "crewAsk" && ctx.param("asked") !== null) || ctx.scenario === "crewAccepted";
  const teamRequests: TeamRequest[] =
    on && waits ? [{ ...SKYBLOCK, members: 6, sentAt: now() - 2 * 3600 }] : [];
  const limited =
    on && ctx.param("limited") !== null && snapshot.me
      ? {
          ...snapshot.me,
          limited: {
            until: now() + 5 * 86_400,
            items: [{ key: "crewLink" as const }, { key: "crewPlans" as const }],
          },
        }
      : snapshot.me;
  return {
    ...snapshot,
    me: limited,
    crew: { ...snapshot.crew, requests: on, link: on, plans: on, planNotes: notes },
    crews,
    teamRequests,
    plans: on ? previewPlans(ctx.scenario, crews, notes, crowd) : [],
    together: on,
  } satisfies SocialSnapshot;
}

/** What the scenario does by itself once the preview runs. */
export function startWave2(ctx: Wave2Context) {
  if (typeof window === "undefined") return;
  const liveAt = Number(ctx.param("liveAt")) || 1600;
  const team = () => ctx.view().crews[0];
  if (ctx.scenario === "crewRequestLive") {
    setTimeout(() => {
      const crew = team();
      if (!crew) return;
      const request: JoinRequest = {
        id: "605",
        name: "Ece",
        username: "ece.tr",
        avatar: "preset:axolotl",
        sentAt: now(),
      };
      ctx.setTeam(crew.id, (t) => ({ ...t, requests: [...(t.requests ?? []), request] }));
      ctx.tell({ kind: "joinRequest", crew: crew.id, request });
    }, liveAt);
  }
  if (ctx.scenario === "crewPlanLive") {
    setTimeout(() => {
      const crew = team();
      if (!crew) return;
      const made = plan(
        crew,
        "e1b2c3d4e5f60718",
        at(now() + 2 * 86_400),
        { id: "701", name: "Yusuf" },
        [{ id: "701", answer: "yes" }],
        "Skyblock, adada buluşalım",
        ctx.view().crew.planNotes,
      );
      ctx.commit((v) => ({ ...v, plans: [...v.plans, { ...made, createdAt: now() }] }));
      ctx.tell({ kind: "crewPlan", plan: made });
    }, liveAt);
  }
  if (ctx.scenario === "crewAsk") {
    // As `projects://crew/ABCD-EFGH-JKMN` from the link's page would.
    setTimeout(() => {
      void import("@/lib/links").then(({ openLink }) => openLink("projects://crew/ABCD-EFGH-JKMN"));
    }, liveAt);
  }
  if (ctx.scenario === "crewAccepted") {
    setTimeout(() => {
      ctx.commit((v) => ({ ...v, teamRequests: [] }));
      ctx.addCrew(ctx.joined(SKYBLOCK));
      ctx.tell({ kind: "requestAccepted", crew: SKYBLOCK.teamId, name: SKYBLOCK.name });
    }, liveAt + 1400);
  }
}

/** A crew the preview lets the player join (by code or by link). */
type Joinable = CrewLook & { teamId: string; name: string; tag: string; motto: string };

/** The API's TEAM5 part, and the join both "Ekibe katıl"s share. */
type Wave2Api = Pick<
  SocialApi,
  | "lookupCrewLink"
  | "joinCrewLink"
  | "withdrawTeamRequest"
  | "answerTeamRequest"
  | "setTeamLink"
  | "setJoinMode"
  | "makePlan"
  | "setPlanNote"
  | "answerPlan"
  | "cancelPlan"
  | "setTogether"
  | "clearTogether"
> & { join: (source: Joinable, approval: boolean) => JoinResult };

/** The API's TEAM5 part, on the preview's data. */
export function wave2Api(ctx: Wave2Context): Wave2Api {
  const linkGone = () =>
    ctx.fail(
      "SOSYAL-EKIP-BAGLANTI-YOK",
      "Bu davet bağlantısı artık geçerli değil.",
      "This invite link no longer works.",
    );
  const relation = (teamId: string): TeamLookup["relation"] => {
    const v = ctx.view();
    if (v.crews.some((crew) => crew.id === teamId)) return "member";
    if (v.crew.multi) return v.crews.length >= v.crewLimit ? "limit" : "none";
    return v.crews.length ? "elsewhere" : "none";
  };
  const join = (source: Joinable, approval: boolean): JoinResult => {
    const v = ctx.view();
    const invited = v.teamInvites.some((invite) => invite.teamId === source.teamId);
    if (approval && v.crew.requests && !invited) {
      if (!v.teamRequests.some((r) => r.teamId === source.teamId)) {
        if (v.teamRequests.length >= 3) {
          throw ctx.fail(
            "SOSYAL-EKIP-ISTEK-SINIR",
            "Bekleyen 3 isteğin var. Önce birini geri çek.",
            "You have 3 requests waiting. Withdraw one first.",
          );
        }
        ctx.commit((view) => ({
          ...view,
          teamRequests: [{ ...source, members: 6, sentAt: now() }, ...view.teamRequests],
        }));
      }
      return "requested";
    }
    ctx.addCrew(ctx.joined(source));
    return "joined";
  };
  const limited = (key: "crewLink" | "crewPlans") =>
    ctx.view().me?.limited?.items.some((item) => item.key === key) ?? false;
  const me = () => ctx.view().me?.id ?? "100";
  /** A note as the service takes it (FIX2): its refusals worded as the launcher's backend says
   * them; "küfür" stands for its word list. */
  const checkedNote = (note: string): string | undefined => {
    const clean = cleanPlanNote(note);
    if (!clean) return undefined;
    const problem = planNoteProblem(clean);
    if (problem === "long") {
      throw ctx.fail(
        "SOSYAL-PLAN-NOT",
        "Not tek satır ve en çok 80 karakter olabilir.",
        "A note is one line of up to 80 characters.",
      );
    }
    if (problem === "link") {
      throw ctx.fail("SOSYAL-PLAN-NOT", "Notta bağlantı olamaz.", "A note can't have a link.");
    }
    if (problem === "phone") {
      throw ctx.fail(
        "SOSYAL-PLAN-NOT",
        "Notta telefon numarası olamaz.",
        "A note can't have a phone number.",
      );
    }
    if (/küfür/i.test(clean)) {
      throw ctx.fail("SOSYAL-PLAN-NOT", "Bu not yazılamaz.", "This note can't be used.");
    }
    return clean;
  };

  return {
    join,
    async lookupCrewLink(link) {
      await delay(500);
      const own = ctx.view().crews.find((crew) => crew.link === link);
      if (own) {
        return {
          teamId: own.id,
          name: own.name,
          tag: own.tag,
          emblem: own.emblem,
          color: own.color,
          frame: own.frame,
          motto: own.motto,
          members: own.members.length,
          relation: "member",
          code: "",
          link,
          join: own.join,
        };
      }
      const found = LINKS[link];
      if (!found) throw linkGone();
      const waiting = ctx.view().teamRequests.find((r) => r.teamId === found.teamId);
      const { join: mode, ...look } = found;
      return {
        ...look,
        members: 6,
        relation: relation(found.teamId),
        code: "",
        link,
        join: mode,
        requestedAt: waiting?.sentAt,
      };
    },
    async joinCrewLink(link) {
      await delay(500);
      const found = LINKS[link];
      if (!found) throw linkGone();
      const { join: mode, ...look } = found;
      return join(look, mode === "approval");
    },
    async withdrawTeamRequest(teamId) {
      await delay(300);
      ctx.commit((v) => ({
        ...v,
        teamRequests: v.teamRequests.filter((r) => r.teamId !== teamId),
      }));
    },
    async answerTeamRequest(teamId, userId, accept) {
      await delay(350);
      ctx.setTeam(teamId, (team) => {
        const asked = team.requests?.find((r) => r.id === userId);
        const requests = (team.requests ?? []).filter((r) => r.id !== userId);
        if (!accept || !asked) return { ...team, requests };
        return {
          ...team,
          requests,
          members: [
            ...team.members,
            {
              id: asked.id,
              name: asked.name,
              username: asked.username,
              avatar: asked.avatar,
              avatarUrl: asked.avatarUrl,
              role: "member",
              moderator: false,
              me: false,
              friend: false,
              blocked: false,
              state: "launcher",
              dnd: false,
              discordUrl: `https://discord.com/users/${asked.id}`,
              joinedAt: now(),
              togetherMonth: 0,
            },
          ],
        };
      });
    },
    async setTeamLink(teamId, on) {
      await delay(400);
      ctx.team(teamId);
      if (on && limited("crewLink")) {
        throw ctx.fail(
          "SOSYAL-SINIRLI",
          "Discord hesabın yeni olduğu için davet bağlantısı oluşturamazsın.",
          "Your Discord account is new, so you can't create invite links yet.",
        );
      }
      const link = on ? `R${String(Date.now() % 1000).padStart(3, "0")}-2N8H-W6PK` : undefined;
      ctx.setTeam(teamId, (team) => ({
        ...team,
        link,
        linkUrl: link ? `https://social.projects.gg/e/${link}` : undefined,
      }));
      return link ?? null;
    },
    async setJoinMode(teamId, join) {
      await delay(300);
      ctx.setTeam(teamId, (team) => {
        // Back to open renews the link: one posted for strangers to ask must not let them in.
        const renew = team.join === "approval" && join === "open" && team.link;
        const link = renew ? "W6PK-R2N8-HJKM" : team.link;
        return {
          ...team,
          join,
          link,
          linkUrl: link ? `https://social.projects.gg/e/${link}` : undefined,
        };
      });
    },
    async makePlan(crew, start, note) {
      await delay(350);
      const team = ctx.team(crew);
      const notes = ctx.view().crew.planNotes;
      const written = notes && note !== undefined ? checkedNote(note) : undefined;
      if (limited("crewPlans")) {
        throw ctx.fail(
          "SOSYAL-SINIRLI",
          "Discord hesabın yeni olduğu için plan yapamazsın.",
          "Your Discord account is new, so you can't make plans yet.",
        );
      }
      const v = ctx.view();
      if (start % QUARTER || start <= now() || start > now() + 7 * 86_400) {
        throw ctx.fail("SOSYAL", "Bu saat seçilemez.", "You can't pick this time.");
      }
      if (v.plans.filter((p) => p.crew === crew && p.startsAt > now()).length >= 3) {
        throw ctx.fail(
          "SOSYAL",
          "Ekipte en çok 3 plan olabilir.",
          "A crew can have 3 plans at most.",
        );
      }
      const id = `f${Date.now().toString(16).slice(-15)}`;
      const made = plan(
        team,
        id,
        start,
        { id: me(), name: v.me?.name ?? "" },
        [{ id: me(), answer: "yes" }],
        written,
        notes,
      );
      ctx.commit((view) => ({ ...view, plans: [...view.plans, { ...made, createdAt: now() }] }));
      return id;
    },
    async setPlanNote(planId, note) {
      await delay(300);
      const found = ctx.view().plans.find((p) => p.id === planId);
      if (!found) {
        throw ctx.fail("SOSYAL-PLAN-YOK", "Bu plan artık yok.", "This plan is gone.");
      }
      const written = checkedNote(note);
      // Its maker writes; the crew's leaders may only clear it.
      if (written !== undefined ? !found.canWriteNote : !found.canClearNote) {
        throw ctx.fail(
          "SOSYAL-TAKIM-IZIN",
          "Bunu yapma iznin yok.",
          "You don't have permission to do that.",
        );
      }
      ctx.commit((v) => ({
        ...v,
        plans: v.plans.map((p) => (p.id === planId ? { ...p, note: written } : p)),
      }));
    },
    async answerPlan(planId, answer) {
      await delay(250);
      if (!ctx.view().plans.some((p) => p.id === planId)) return false;
      ctx.commit((v) => ({
        ...v,
        plans: v.plans.map((p) =>
          p.id === planId
            ? {
                ...p,
                myAnswer: answer,
                answers: [...p.answers.filter((a) => a.id !== me()), { id: me(), answer }],
              }
            : p,
        ),
      }));
      return true;
    },
    async cancelPlan(planId) {
      await delay(300);
      ctx.commit((v) => ({ ...v, plans: v.plans.filter((p) => p.id !== planId) }));
    },
    async setTogether(on) {
      await delay(200);
      ctx.commit((v) => ({
        ...v,
        together: on,
        crews: on ? v.crews : v.crews.map(withoutTogether),
      }));
    },
    async clearTogether() {
      await delay(200);
      ctx.commit((v) => ({ ...v, crews: v.crews.map(withoutTogether) }));
    },
  };
}

function withoutTogether(team: SocialTeam): SocialTeam {
  return {
    ...team,
    togetherWeek: 0,
    members: team.members.map((m) => ({ ...m, togetherMonth: 0 })),
  };
}
