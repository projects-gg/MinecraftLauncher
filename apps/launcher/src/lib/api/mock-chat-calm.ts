// CHAT-CALM's previews on top of mock-social's data: the crew chat's "Yavaş mod" and a crowded crew
// that starts muted.
//
// - `?social=chatCalm`: the player is a plain member of Gece Kartalları, whose chat has a 30 s slow
//   mode; their last message went 12 s ago, so the composer counts down.
// - `?social=chatCalmLead`: the player leads it (the crew page shows "Yavaş mod" in its settings).
// - `?social=crewCrowded`: the player just joined a crowded crew; it comes muted, with the toast.
// - `&slow=<seconds>` on any crew preview: that slow mode on the crews; `&slowmode=0`: a service
//   without it (the setting never shows).
import type { SocialApi, SocialNotice, SocialSnapshot } from "./social-types";

export const CALM_SCENARIOS: readonly string[] = ["chatCalm", "chatCalmLead", "crewCrowded"];

/** The preview crew's id (mock-social.ts `previewTeam`). */
const GKR = "0123456789abcdef";

export interface CalmContext {
  scenario: string;
  param: (key: string) => string | null;
  commit: (change: (view: SocialSnapshot) => SocialSnapshot) => void;
  tell: (notice: SocialNotice) => void;
}

/** The scenario's starting data. */
export function calmSnapshot(snapshot: SocialSnapshot, ctx: CalmContext): SocialSnapshot {
  const offered = ctx.param("slowmode") !== "0";
  const asked = Number(ctx.param("slow"));
  const slow = ctx.scenario.startsWith("chatCalm") ? 30 : Number.isFinite(asked) ? asked : 0;
  return {
    ...snapshot,
    crew: { ...snapshot.crew, slow: offered && snapshot.crew.look },
    crews: snapshot.crews.map((crew) => (slow > 0 ? { ...crew, slow } : crew)),
  };
}

/** The scenario's live part: the crowded crew's notice a moment after the page opens. */
export function startCalm(ctx: CalmContext) {
  if (ctx.scenario !== "crewCrowded") return;
  setTimeout(() => {
    ctx.commit((view) => ({
      ...view,
      crews: view.crews.map((crew) =>
        crew.id === GKR ? { ...crew, mute: { muted: true, muteUntil: null } } : crew,
      ),
    }));
    ctx.tell({ kind: "crowdedCrewMuted", crew: GKR });
  }, 900);
}

export function calmApi(ctx: CalmContext): Pick<SocialApi, "setTeamSlow"> {
  return {
    async setTeamSlow(teamId, seconds) {
      await new Promise((done) => setTimeout(done, 250));
      ctx.commit((view) => ({
        ...view,
        crews: view.crews.map((crew) => (crew.id === teamId ? { ...crew, slow: seconds } : crew)),
      }));
    },
  };
}
