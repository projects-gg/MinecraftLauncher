import { describe, expect, it } from "vitest";
import type {
  ProjectsSkinChange,
  ProjectsSkinJob,
  ProjectsSkinView,
} from "@/lib/api/projects-skin-types";
import {
  CHECK_AFTER_SEC,
  daysLeft,
  linkedTo,
  linkRefusalOutcome,
  linkResultOutcome,
  lookupReady,
  needsCheck,
  type PanelInput,
  panelState,
  pinDigits,
  premiumNameProblem,
  refusalNote,
  skinResultNote,
  toastFor,
  untoldFailure,
  waitWords,
} from "./rules";

const NOW = 1_800_000_000;

function view(overrides: Partial<ProjectsSkinView> = {}): ProjectsSkinView {
  return {
    accountId: "a1",
    name: "EjderAvcisi",
    access: "ready",
    serverAway: false,
    link: { state: "linked", name: "ejderavcisi", at: NOW - 86_400, checkedAt: NOW - 60 },
    skin: {
      kind: "png",
      variant: "classic",
      premium: null,
      at: NOW - 3600,
      by: "launcher",
      picture: "data:image/png;base64,AAAA",
    },
    job: null,
    lastJob: null,
    linkJob: null,
    limits: { perDay: 3, left: 2, resetAt: NOW + 3600, linkTriesLeft: 5, linkRetryAt: null },
    blocked: null,
    limited: null,
    canUndo: true,
    onProjects: false,
    ...overrides,
  };
}

function job(overrides: Partial<ProjectsSkinJob> = {}): ProjectsSkinJob {
  return {
    id: "j1",
    kind: "skin",
    state: "queued",
    name: "EjderAvcisi",
    source: "png",
    result: null,
    picture: "data:image/png;base64,BBBB",
    variant: "classic",
    createdAtMs: NOW * 1000,
    updatedAtMs: NOW * 1000,
    ...overrides,
  };
}

function state(input: Partial<PanelInput> & { view?: ProjectsSkinView } = {}) {
  return panelState({
    view: input.view ?? view(),
    signedIn: input.signedIn ?? true,
    draft: input.draft ?? false,
    sending: input.sending ?? false,
    failure: input.failure ?? null,
    waitedLong: input.waitedLong ?? false,
    nowSec: input.nowSec ?? NOW,
  });
}

describe("the code box", () => {
  it("keeps the digits of what is typed or pasted, six at most", () => {
    expect(pinDigits("48 29-13")).toBe("482913");
    expect(pinDigits("Kod: 4829135")).toBe("482913");
    expect(pinDigits("abc")).toBe("");
  });
});

describe("asking ProBot about the link", () => {
  it("asks when the link is unknown, never learned or a day old", () => {
    const fresh = { state: "linked" as const, name: "A", at: NOW, checkedAt: NOW - 60 };
    expect(needsCheck(fresh, NOW)).toBe(false);
    expect(needsCheck({ ...fresh, checkedAt: NOW - CHECK_AFTER_SEC }, NOW)).toBe(true);
    expect(needsCheck({ ...fresh, checkedAt: null }, NOW)).toBe(true);
    expect(needsCheck({ ...fresh, state: "unknown" }, NOW)).toBe(true);
    expect(needsCheck(null, NOW)).toBe(false);
  });

  it("matches the linked name without regard to case", () => {
    expect(linkedTo({ state: "linked", name: "ahmet", at: 1, checkedAt: 1 }, "Ahmet")).toBe(true);
    expect(linkedTo({ state: "unknown", name: "Ahmet", at: 1, checkedAt: 1 }, "Ahmet")).toBe(false);
    expect(linkedTo({ state: "linked", name: "Mehmet", at: 1, checkedAt: 1 }, "Ahmet")).toBe(false);
  });
});

describe("the premium name field", () => {
  it("uses the account name's rule", () => {
    expect(premiumNameProblem("Gece kuşu")).toBe("turkish");
    expect(premiumNameProblem("Gece kusu")).toBe("spaces");
    expect(premiumNameProblem("ab")).toBeNull();
    expect(premiumNameProblem("ab", true)).toBe("tooShort");
    expect(premiumNameProblem("a".repeat(17))).toBe("tooLong");
  });

  it("looks up only names that could exist", () => {
    expect(lookupReady("Gecekusu")).toBe(true);
    expect(lookupReady(" Gecekusu ")).toBe(true);
    expect(lookupReady("ab")).toBe(false);
    expect(lookupReady("Gece-kusu")).toBe(false);
  });
});

describe("words for waits and dates", () => {
  it("says a wait in minutes, or hours from an hour", () => {
    expect(waitWords(20)).toEqual({ unit: "minutes", count: 1 });
    expect(waitWords(25 * 60)).toEqual({ unit: "minutes", count: 25 });
    expect(waitWords(90 * 60)).toEqual({ unit: "hours", count: 2 });
  });

  it("counts whole days, at least one", () => {
    expect(daysLeft(NOW + 5 * 86_400, NOW)).toBe(5);
    expect(daysLeft(NOW + 4.2 * 86_400, NOW)).toBe(5);
    expect(daysLeft(NOW + 60, NOW)).toBe(1);
  });
});

describe("refusals and results", () => {
  it("words every refusal of a skin change", () => {
    expect(refusalNote({ code: "quota", retryAfter: 600 }, NOW).key).toBe("quota");
    expect(refusalNote({ code: "limited", until: NOW + 3 * 86_400 }, NOW)).toMatchObject({
      key: "limited",
      count: 3,
    });
    expect(refusalNote({ code: "blocked", until: NOW + 86_400 }, NOW)).toMatchObject({
      key: "blocked",
      until: NOW + 86_400,
    });
    expect(refusalNote({ code: "blocked", until: null }, NOW).key).toBe("blockedForever");
    expect(refusalNote({ code: "otherAccount", name: "Mehmet" }, NOW)).toMatchObject({
      key: "otherAccount",
      name: "Mehmet",
    });
    expect(refusalNote({ code: "tries", retryAfter: 1500 }, NOW)).toMatchObject({
      key: "tries",
      wait: { unit: "minutes", count: 25 },
    });
    expect(refusalNote({ code: "choiceGone" }, NOW).key).toBe("premiumInvalid");
    expect(refusalNote({ code: "unreachable" }, NOW).key).toBe("unreachable");
  });

  it("words a skin job's end, and says nothing for a quiet one", () => {
    expect(skinResultNote({ code: "applied" })).toBeNull();
    expect(skinResultNote({ code: "superseded" })).toBeNull();
    expect(skinResultNote({ code: "cancelled" })).toBeNull();
    expect(skinResultNote({ code: "invalidImage" })?.key).toBe("invalidImage");
    expect(skinResultNote({ code: "mineskinBusy" })?.key).toBe("busy");
    expect(skinResultNote({ code: "premiumDisabled" })?.key).toBe("premiumDisabled");
    expect(skinResultNote({ code: "timeout" })?.key).toBe("saveFailed");
    expect(skinResultNote({ code: "error" })?.key).toBe("saveFailed");
    expect(skinResultNote({ code: "skinBlocked", until: null })?.key).toBe("blockedForever");
  });

  it("tells the code box how a link ended", () => {
    const linked = job({ kind: "link", state: "done", result: { code: "linked" } });
    expect(linkResultOutcome(linked)).toEqual({ kind: "linked", name: "EjderAvcisi" });
    const expired = job({ kind: "link", state: "failed", result: { code: "pinExpired" } });
    expect(linkResultOutcome(expired)).toMatchObject({
      kind: "error",
      note: { key: "pinExpired" },
    });
    const taken = job({ kind: "link", state: "failed", result: { code: "minecraftTaken" } });
    expect(linkResultOutcome(taken)).toMatchObject({ kind: "error", note: { key: "taken" } });
    const other = job({
      kind: "link",
      state: "failed",
      result: { code: "otherAccount", name: "Mehmet" },
    });
    expect(linkResultOutcome(other)).toEqual({ kind: "other", name: "Mehmet" });
    expect(linkRefusalOutcome({ code: "alreadyLinked", name: "Ahmet" }, NOW)).toEqual({
      kind: "linked",
      name: "Ahmet",
    });
    expect(linkRefusalOutcome({ code: "tries", retryAfter: 3600 }, NOW)).toMatchObject({
      kind: "error",
      note: { key: "tries", wait: { unit: "hours", count: 1 } },
    });
  });
});

describe("the panel (skin1.md §13.2)", () => {
  it("not signed in to friends: the Discord card, nothing to save", () => {
    const s = state({ signedIn: false });
    expect(s.body).toBe("connect");
    expect(s.preview).toBe("default");
    expect(s.chip).toBeNull();
    expect(s.saveShown).toBe(false);
    expect(s.note).toBeNull();
    expect(state({ view: view({ access: "signedOut" }) }).body).toBe("connect");
  });

  it("feature off and session ended: no editor", () => {
    expect(state({ view: view({ access: "off" }) }).body).toBe("off");
    expect(state({ view: view({ access: "session" }) }).body).toBe("session");
  });

  it("service or ProBot away: the link dims, saving waits, the picker stays open", () => {
    const s = state({ view: view({ access: "unreachable" }), draft: true });
    expect(s.body).toBe("editor");
    expect(s.link).toEqual({ kind: "linked", name: "ejderavcisi", dim: true });
    expect(s.picker).toBe("open");
    expect(s.saveEnabled).toBe(false);
    expect(s.note?.key).toBe("unreachable");
    const unknown = view({
      access: "unreachable",
      link: { state: "unknown", name: null, at: null, checkedAt: null },
    });
    expect(state({ view: unknown }).link).toEqual({ kind: "none" });
  });

  it("link unknown: a skeleton row while ProBot is asked, saving waits silently", () => {
    const s = state({
      view: view({ link: { state: "unknown", name: "EjderAvcisi", at: null, checkedAt: null } }),
      draft: true,
    });
    expect(s.link.kind).toBe("checking");
    expect(s.saveEnabled).toBe(false);
    expect(s.note).toBeNull();
  });

  it("not linked: the code box, and Kaydet says why it waits", () => {
    const s = state({
      view: view({ link: { state: "none", name: null, at: null, checkedAt: NOW }, skin: null }),
      draft: true,
    });
    expect(s.link).toEqual({ kind: "code", dim: false });
    expect(s.picker).toBe("open");
    expect(s.saveEnabled).toBe(false);
    expect(s.note?.key).toBe("linkFirst");
    expect(s.chip).toBe("preview");
  });

  it("linked to another account: its card, no line", () => {
    const s = state({
      view: view({
        link: { state: "linked", name: "Mehmet", at: NOW, checkedAt: NOW },
        skin: null,
      }),
      draft: true,
    });
    expect(s.link).toEqual({ kind: "other", name: "Mehmet", dim: false });
    expect(s.saveEnabled).toBe(false);
    expect(s.note).toBeNull();
    expect(s.resetShown).toBe(false);
  });

  it("ready: the Projects skin with its chip, nothing to save yet", () => {
    const s = state();
    expect(s.preview).toBe("projects");
    expect(s.chip).toBe("projects");
    expect(s.saveShown).toBe(true);
    expect(s.saveEnabled).toBe(false);
    expect(s.revertShown).toBe(false);
    expect(s.resetShown).toBe(true);
    expect(s.resetEnabled).toBe(true);
    expect(s.note).toBeNull();
  });

  it("changed: the pick previews and can be saved or dropped", () => {
    const s = state({ draft: true });
    expect(s.preview).toBe("draft");
    expect(s.chip).toBe("preview");
    expect(s.saveEnabled).toBe(true);
    expect(s.revertShown).toBe(true);
  });

  it("sending: the picker locks, Kaydet stays (it says Gönderiliyor after 150 ms)", () => {
    const s = state({ draft: true, sending: true });
    expect(s.picker).toBe("locked");
    expect(s.saveShown).toBe(true);
    expect(s.saveEnabled).toBe(false);
    expect(s.revertShown).toBe(false);
    expect(s.resetEnabled).toBe(false);
  });

  it("in line: the chip says so, Kaydet goes, a long wait is said after a minute", () => {
    const inLine = view({ job: job() });
    const s = state({ view: inLine, draft: true });
    expect(s.preview).toBe("job");
    expect(s.chip).toBe("inLine");
    expect(s.saveShown).toBe(false);
    expect(s.picker).toBe("locked");
    expect(s.note).toBeNull();
    expect(state({ view: inLine, waitedLong: true }).note?.key).toBe("longWait");
    expect(state({ view: view({ job: job(), serverAway: true }) }).note?.key).toBe("serverAway");
  });

  it("failed: the reason in place, the pick stays and can be saved again", () => {
    const s = state({ draft: true, failure: { key: "invalidImage", tone: "error" } });
    expect(s.note?.key).toBe("invalidImage");
    expect(s.chip).toBe("preview");
    expect(s.saveEnabled).toBe(true);
  });

  it("no changes left today: Kaydet waits and says why", () => {
    const s = state({
      view: view({ limits: { ...view().limits, left: 0 } }),
      draft: true,
    });
    expect(s.saveEnabled).toBe(false);
    expect(s.resetEnabled).toBe(false);
    expect(s.note?.key).toBe("quota");
    expect(s.picker).toBe("open");
  });

  it("says the last change of the day, and nothing for more", () => {
    expect(state({ view: view({ limits: { ...view().limits, left: 1 } }) }).note).toMatchObject({
      key: "lastChange",
      count: 1,
    });
    expect(state().note).toBeNull();
  });

  it("limited account: the picker dims, the line says when it opens", () => {
    const s = state({ view: view({ limited: { until: NOW + 5 * 86_400 } }), draft: true });
    expect(s.picker).toBe("dim");
    expect(s.saveEnabled).toBe(false);
    expect(s.note).toMatchObject({ key: "limited", count: 5 });
    expect(state({ view: view({ limited: { until: NOW - 10 } }) }).picker).toBe("open");
  });

  it("staff block: the picker dims, the line names the end or says for good", () => {
    const s = state({ view: view({ blocked: { until: NOW + 7 * 86_400 } }) });
    expect(s.picker).toBe("dim");
    expect(s.note).toMatchObject({ key: "blocked", until: NOW + 7 * 86_400 });
    expect(state({ view: view({ blocked: { until: null } }) }).note?.key).toBe("blockedForever");
  });

  it("Lobi away: saving still works, the skin waits in line", () => {
    const s = state({ view: view({ serverAway: true }), draft: true });
    expect(s.saveEnabled).toBe(true);
    expect(s.note?.key).toBe("serverAway");
  });

  it("no Projects skin yet: the default look, no chip, no way back to default", () => {
    const s = state({ view: view({ skin: null }) });
    expect(s.preview).toBe("default");
    expect(s.chip).toBeNull();
    expect(s.resetShown).toBe(false);
  });

  it("back to default (or reset by staff): the default look is on Projects", () => {
    const s = state({
      view: view({
        skin: {
          kind: "default",
          variant: "classic",
          premium: null,
          at: NOW,
          by: "staff",
          picture: null,
        },
      }),
    });
    expect(s.preview).toBe("default");
    expect(s.chip).toBe("projects");
    expect(s.resetShown).toBe(false);
  });
});

describe("toasts of closed jobs", () => {
  const closedSkin = (
    overrides: Partial<ProjectsSkinJob>,
    extra: Partial<Extract<ProjectsSkinChange, { kind: "job" }>> = {},
  ): ProjectsSkinChange => ({
    kind: "job",
    job: job({ state: "done", result: { code: "applied" }, ...overrides }),
    accountId: "a1",
    undo: true,
    onProjects: false,
    ...extra,
  });
  const away = { panelFor: null, codeBox: false };

  it("a saved skin: where it shows, and Geri al while a change is left", () => {
    expect(toastFor(closedSkin({}), away)).toEqual({
      kind: "saved",
      backToDefault: false,
      rejoin: false,
      undoFor: "a1",
    });
    expect(toastFor(closedSkin({}, { onProjects: true, undo: false }), away)).toMatchObject({
      rejoin: true,
      undoFor: null,
    });
    expect(toastFor(closedSkin({ source: "default" }), away)).toMatchObject({
      backToDefault: true,
    });
  });

  it("a failure: in the open panel in place, else a toast", () => {
    const failed = closedSkin({ state: "failed", result: { code: "invalidImage" } });
    expect(toastFor(failed, { panelFor: "a1", codeBox: false })).toBeNull();
    expect(toastFor(failed, away)).toMatchObject({
      kind: "skinFailed",
      note: { key: "invalidImage" },
      accountId: "a1",
    });
    expect(
      toastFor(closedSkin({ state: "failed", result: { code: "superseded" } }), away),
    ).toBeNull();
  });

  it("links: the box tells its own result; one made elsewhere gets a toast", () => {
    const linked: ProjectsSkinChange = {
      kind: "job",
      job: job({ kind: "link", state: "done", result: { code: "linked" } }),
      accountId: null,
      undo: false,
      onProjects: false,
    };
    expect(toastFor(linked, { panelFor: null, codeBox: true })).toBeNull();
    expect(toastFor(linked, away)).toEqual({ kind: "linked", name: "EjderAvcisi" });
    expect(toastFor({ kind: "linked", name: "Ahmet" }, away)).toEqual({
      kind: "linked",
      name: "Ahmet",
    });
    const unlinked: ProjectsSkinChange = {
      kind: "job",
      job: job({ kind: "unlink", state: "done", result: { code: "unlinked" } }),
      accountId: null,
      undo: false,
      onProjects: false,
    };
    expect(toastFor(unlinked, away)).toEqual({ kind: "unlinked" });
    const check: ProjectsSkinChange = {
      kind: "job",
      job: job({ kind: "check", state: "done", result: { code: "linked" } }),
      accountId: null,
      undo: false,
      onProjects: false,
    };
    expect(toastFor(check, away)).toBeNull();
    expect(toastFor({ kind: "state" }, away)).toBeNull();
  });

  it("tells a failure that waited while the launcher was closed once", () => {
    const failed = job({
      id: "j9",
      state: "failed",
      result: { code: "mineskinBusy" },
      updatedAtMs: NOW * 1000 - 3_600_000,
    });
    expect(untoldFailure(failed, new Set(), NOW * 1000)?.key).toBe("busy");
    expect(untoldFailure(failed, new Set(["j9"]), NOW * 1000)).toBeNull();
    expect(untoldFailure(failed, new Set(), NOW * 1000 + 2 * 86_400_000)).toBeNull();
    expect(
      untoldFailure(job({ state: "done", result: { code: "applied" } }), new Set(), NOW * 1000),
    ).toBeNull();
  });
});
