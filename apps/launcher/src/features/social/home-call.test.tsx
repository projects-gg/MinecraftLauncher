// @vitest-environment happy-dom
// SMALL12: a crewmate's call that Home's line shows (with "Geliyorum") gets no toast as well; the
// toast stays for every other place and for a call the line does not show.
import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  type CrewCall,
  HIDDEN_SNAPSHOT,
  type SocialSnapshot,
  type SocialTeam,
  type TeamMember,
} from "@/lib/api/social-types";
import { useLaunch } from "@/stores/launch";
import { homeShowsCall, useHomeLineShown } from "./home-strip";
import { useSocialUi } from "./store";

const NOW = 1_792_521_000_000;

function member(id: string, name: string, extra: Partial<TeamMember> = {}): TeamMember {
  return {
    id,
    name,
    username: name.toLowerCase(),
    avatar: "discord",
    role: "member",
    moderator: false,
    me: false,
    friend: false,
    blocked: false,
    state: "launcher",
    dnd: false,
    discordUrl: `https://discord.com/users/${id}`,
    joinedAt: 0,
    ...extra,
  };
}

const crew = {
  id: "0123456789abcdef",
  name: "Gece Kartalları",
  tag: "GKR",
  emblem: "controller",
  color: "teal",
  frame: "banner",
  motto: "",
  code: "EKP-7Q2X-9KDA",
  createdOn: "2026-09-20",
  role: "member",
  moderator: false,
  permissions: { invite: "members", call: "members", kick: "moderators", edit: "leader" },
  members: [member("100", "Ben", { me: true }), member("1", "Ayşe")],
  invited: [],
} as unknown as SocialTeam;

function call(id: string, startedAgoSec: number): CrewCall {
  return {
    id,
    from: "1",
    mine: false,
    kind: "join",
    crews: [],
    startedAt: NOW / 1000 - startedAgoSec,
    endsAt: NOW / 1000 + 600,
    answers: [],
  };
}

const signedIn: SocialSnapshot = {
  ...HIDDEN_SNAPSHOT,
  seq: 1,
  enabled: true,
  auth: "signedIn",
  loaded: true,
  teams: true,
  crews: [crew],
};

/** Home is the page: its friends line counts itself while mounted. */
function OnHome() {
  useHomeLineShown();
  return null;
}

const idle = useLaunch.getState().phase;

beforeEach(() => {
  useSocialUi.setState({ view: signedIn, open: false, closedCalls: [] });
});
afterEach(() => {
  cleanup();
  useLaunch.setState({ phase: idle });
});

describe("a crew call's toast", () => {
  it("is left out while Home's line shows the call, also before the list brings it", () => {
    render(<OnHome />);
    const arriving = call("c1", 5);
    expect(homeShowsCall(arriving, NOW)).toBe(true);
    useSocialUi.setState({ view: { ...signedIn, calls: [arriving] } });
    expect(homeShowsCall(arriving, NOW)).toBe(true);
  });

  it("shows anywhere but Home", () => {
    expect(homeShowsCall(call("c1", 5), NOW)).toBe(false);
  });

  it("shows when Home's line shows another call or none", () => {
    render(<OnHome />);
    // The newest unanswered call is the one on the line.
    const older = call("old", 300);
    useSocialUi.setState({ view: { ...signedIn, calls: [older, call("new", 5)] } });
    expect(homeShowsCall(older, NOW)).toBe(false);
    // Closed on Home: the line moved on.
    useSocialUi.setState({ view: signedIn, closedCalls: ["c1"] });
    expect(homeShowsCall(call("c1", 5), NOW)).toBe(false);
  });

  it("shows while a game is open (Home's crew lines wait then)", () => {
    render(<OnHome />);
    useLaunch.setState({ phase: { kind: "running", since: NOW, shown: true, firstStart: false } });
    expect(homeShowsCall(call("c1", 5), NOW)).toBe(false);
  });
});
