// @vitest-environment happy-dom
// CREW-UI: the list page's tab is the player's last pick (across restarts, `pl.social`), an
// explicit tab wins for one visit; the crew page has no tabs any more: its chat opens on the chat
// page from every place that opened the old "Sohbet" tab, and "Geri" goes back where it came from.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import {
  type ChatState,
  type Conversation,
  HIDDEN_SNAPSHOT,
  type SocialSnapshot,
  type SocialTeam,
  type TeamMember,
} from "@/lib/api/social-types";
import { CrewChatEntry } from "./chat/crew-chat-entry";
import { useSocNextFlag } from "./next/flag";
import { useSocNextPrefs } from "./next/prefs";
import {
  conversationShown,
  crewPageShown,
  friendListShown,
  useSocialPrefs,
  useSocialUi,
} from "./store";

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

function crew(id: string, name: string): SocialTeam {
  return {
    id,
    name,
    tag: "GKR",
    emblem: "controller",
    color: "teal",
    frame: "banner",
    motto: "",
    code: "EKP-7Q2X-9KDA",
    createdOn: "2026-09-20",
    role: "member",
    moderator: false,
    members: [member("100", "GeceKurdu_", { me: true }), member("1", "KorAtesi")],
    invited: [],
    mute: { muted: false },
  } as unknown as SocialTeam;
}

const A = crew("aaaaaaaaaaaaaaaa", "Gece Kartalları");
const B = crew("bbbbbbbbbbbbbbbb", "Skyblock Ustaları");
const convA = `c:${A.id}`;
const convB = `c:${B.id}`;
const dm = "d:1:100";

function conversation(id: string, unread: number, text = "selam"): Conversation {
  return {
    id,
    crew: id.slice(2),
    unread,
    writable: true,
    failed: false,
    last: { from: "1", mine: false, text, at: 1_792_521_000_000 },
  };
}

function chat(conversations: Conversation[]): ChatState {
  return { on: true, mailbox: true, perDay: 500, conversations };
}

const signedIn: SocialSnapshot = {
  ...HIDDEN_SNAPSHOT,
  seq: 1,
  enabled: true,
  auth: "signedIn",
  loaded: true,
  teams: true,
  crews: [A, B],
  crew: { ...HIDDEN_SNAPSHOT.crew, multi: true },
  me: { id: "100", name: "GeceKurdu_", avatar: "discord" } as SocialSnapshot["me"],
  chat: chat([conversation(convA, 0), conversation(convB, 0)]),
};

beforeEach(() => {
  localStorage.clear();
  useSocialPrefs.setState({ listTab: "friends" });
  useSocialUi.setState({
    view: signedIn,
    open: false,
    page: "list",
    direction: 1,
    listTab: "friends",
    chatConv: null,
    chatFrom: "list",
    chatFocus: null,
    crewTab: null,
    prefillLink: null,
  });
  useSocNextFlag.setState({ on: false });
});

afterEach(() => cleanup());

describe("the list page's tab", () => {
  it("opens on the tab the player last picked", () => {
    const ui = useSocialUi.getState();
    ui.openPanel();
    useSocialUi.getState().setListTab("messages");
    useSocialUi.getState().closePanel();
    expect(useSocialPrefs.getState().listTab).toBe("messages");
    useSocialUi.getState().openPanel();
    expect(useSocialUi.getState().listTab).toBe("messages");
    useSocialUi.getState().setListTab("friends");
    useSocialUi.getState().closePanel();
    useSocialUi.getState().openPanel();
    expect(useSocialUi.getState().listTab).toBe("friends");
  });

  it("shows an explicit tab for that visit only (a request's or an invite's Göster)", () => {
    useSocialUi.getState().openPanel();
    useSocialUi.getState().setListTab("messages");
    useSocialUi.getState().closePanel();
    useSocialUi.getState().openPanel({ page: "list", tab: "friends" });
    expect(useSocialUi.getState().listTab).toBe("friends");
    expect(useSocialPrefs.getState().listTab).toBe("messages");
    useSocialUi.getState().closePanel();
    useSocialUi.getState().openPanel();
    expect(useSocialUi.getState().listTab).toBe("messages");
  });

  it("stays where it is when the open panel is asked to open again", () => {
    useSocialUi.getState().openPanel();
    useSocialUi.getState().setListTab("messages");
    useSocialPrefs.setState({ listTab: "friends" });
    useSocialUi.getState().openPanel({ page: "list" });
    expect(useSocialUi.getState().listTab).toBe("messages");
  });

  it("is kept on this computer and comes back after a restart", async () => {
    useSocialUi.getState().setListTab("messages");
    expect(JSON.parse(localStorage.getItem("pl.social") ?? "{}").state.listTab).toBe("messages");
    vi.resetModules();
    const fresh = await import("./store");
    expect(fresh.useSocialPrefs.getState().listTab).toBe("messages");
    fresh.useSocialUi.getState().openPanel();
    expect(fresh.useSocialUi.getState().listTab).toBe("messages");
  });

  it("falls back to the friend list while chat is off", () => {
    useSocialPrefs.setState({ listTab: "messages" });
    useSocialUi.setState({ view: { ...signedIn, chat: undefined } });
    useSocialUi.getState().openPanel();
    const ui = useSocialUi.getState();
    expect(ui.listTab).toBe("messages");
    expect(friendListShown(ui)).toBe(true);
  });
});

describe("a crew's chat", () => {
  it("opens from the crew page's Sohbete Git and goes back there", () => {
    useSocialUi.getState().openPanel({ page: "team" });
    useSocialUi.getState().setCrewTab(A.id);
    useSocialUi.getState().openChat(convA, { from: "team" });
    let ui = useSocialUi.getState();
    expect([ui.page, ui.chatConv, ui.chatFrom, ui.crewTab, ui.direction]).toEqual([
      "chat",
      convA,
      "team",
      A.id,
      1,
    ]);
    expect(ui.takeChatFocus(convA)).toBe(true);
    expect(conversationShown(ui, convA)).toBe(true);
    expect(crewPageShown(ui, A.id)).toBe(false);
    ui.goTo(ui.chatFrom);
    ui = useSocialUi.getState();
    expect([ui.page, ui.direction]).toEqual(["team", -1]);
    expect(crewPageShown(ui, A.id)).toBe(true);
    expect(conversationShown(ui, convA)).toBe(false);
  });

  it("opens from a Mesajlar row and goes back to Mesajlar", () => {
    useSocialUi.getState().openPanel();
    useSocialUi.getState().setListTab("messages");
    useSocialUi.getState().openChat(convB, { from: "list" });
    const ui = useSocialUi.getState();
    expect([ui.page, ui.chatConv, ui.chatFrom, ui.crewTab]).toEqual(["chat", convB, "list", null]);
    ui.goTo(ui.chatFrom);
    expect(useSocialUi.getState().page).toBe("list");
    expect(useSocialUi.getState().listTab).toBe("messages");
  });

  it("opens from the friend list's crew menu (Sohbeti aç) and goes back to the list", () => {
    useSocialUi.getState().openPanel();
    useSocialUi.getState().openChat(convA);
    expect(useSocialUi.getState().chatFrom).toBe("list");
  });

  it("opens from a toast or a notification with the panel closed, back to its crew page", () => {
    useSocialUi.getState().openChat(convB);
    const ui = useSocialUi.getState();
    expect([ui.open, ui.page, ui.chatConv, ui.chatFrom, ui.crewTab]).toEqual([
      true,
      "chat",
      convB,
      "team",
      B.id,
    ]);
    expect(conversationShown(ui, convB)).toBe(true);
  });

  it("opened from another crew's page goes back to that page", () => {
    useSocialUi.getState().openPanel({ page: "team" });
    useSocialUi.getState().setCrewTab(A.id);
    useSocialUi.getState().openChat(convB);
    const ui = useSocialUi.getState();
    expect([ui.chatConv, ui.chatFrom, ui.crewTab]).toEqual([convB, "team", A.id]);
  });

  it("opened over a friend's chat goes back to its own crew page", () => {
    useSocialUi.getState().openChat(dm, { from: "list" });
    useSocialUi.getState().openChat(convA);
    const ui = useSocialUi.getState();
    expect([ui.chatConv, ui.chatFrom, ui.crewTab]).toEqual([convA, "team", A.id]);
  });

  it("opened again while on screen keeps its way back", () => {
    useSocialUi.getState().openPanel();
    useSocialUi.getState().openChat(convA, { from: "list" });
    useSocialUi.getState().openChat(convA);
    expect(useSocialUi.getState().chatFrom).toBe("list");
  });

  it("leaves a crewmate's direct chat as it was (back to the crew page)", () => {
    useSocialUi.getState().openPanel({ page: "team" });
    useSocialUi.getState().setCrewTab(B.id);
    useSocialUi.getState().openChat(dm, { from: "team" });
    const ui = useSocialUi.getState();
    expect([ui.chatConv, ui.chatFrom, ui.crewTab]).toEqual([dm, "team", B.id]);
  });
});

describe("the crew page", () => {
  it("opens on the crew from every place that shows a crew (no pane to fall into)", () => {
    // A crew's notice, a plan reminder, Home's lines: the page, then the crew.
    useSocialUi.getState().openChat(convA, { from: "list" });
    useSocialUi.getState().closePanel();
    useSocialUi.getState().openPanel({ page: "team" });
    useSocialUi.getState().setCrewTab(B.id);
    const ui = useSocialUi.getState();
    expect(ui.page).toBe("team");
    expect(crewPageShown(ui, B.id)).toBe(true);
    expect("crewPane" in ui).toBe(false);
  });

  it("opens Ekip kur ya da katıl for a projects://crew link", () => {
    useSocialUi.getState().openPanel({ page: "team", link: "abc" });
    const ui = useSocialUi.getState();
    expect([ui.page, ui.crewTab, ui.prefillLink]).toEqual(["team", "new", "abc"]);
  });
});

describe("Sohbete Git", () => {
  it("says how many are unread and opens the crew's chat from its page", () => {
    useSocialUi.setState({
      view: { ...signedIn, chat: chat([conversation(convA, 3), conversation(convB, 0)]) },
    });
    useSocialUi.getState().openPanel({ page: "team" });
    useSocialUi.getState().setCrewTab(A.id);
    render(<CrewChatEntry team={A} />);
    const button = screen.getByRole("button", { name: "Sohbete Git, 3 okunmamış mesaj" });
    expect(button.textContent).toContain("3");
    fireEvent.click(button);
    const ui = useSocialUi.getState();
    expect([ui.page, ui.chatConv, ui.chatFrom]).toEqual(["chat", convA, "team"]);
  });

  it("is just its words without unread messages", () => {
    render(<CrewChatEntry team={B} />);
    expect(screen.getByRole("button", { name: "Sohbete Git" }).textContent).toBe("Sohbete Git");
  });

  it("counts one the player marked unread, and an @ when the newest mentions them", () => {
    useSocNextFlag.setState({ on: true });
    useSocNextPrefs.setState({ unreadMarks: { [convB]: 1_792_521_000_000 } });
    useSocialUi.setState({
      view: {
        ...signedIn,
        chat: chat([conversation(convA, 2, "@GeceKurdu_ gel"), conversation(convB, 0)]),
      },
    });
    render(
      <>
        <CrewChatEntry team={A} />
        <CrewChatEntry team={B} />
      </>,
    );
    expect(
      screen.getByRole("button", { name: "Sohbete Git, 2 okunmamış mesaj, Senden bahsedildi" }),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Sohbete Git, 1 okunmamış mesaj" })).toBeTruthy();
  });

  it("takes the focus back when the player returns from that crew's chat", () => {
    useSocialUi.getState().openPanel({ page: "team" });
    useSocialUi.getState().setCrewTab(A.id);
    useSocialUi.getState().openChat(convA, { from: "team" });
    useSocialUi.getState().goTo("team");
    render(<CrewChatEntry team={A} />);
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Sohbete Git" }));
  });
});
