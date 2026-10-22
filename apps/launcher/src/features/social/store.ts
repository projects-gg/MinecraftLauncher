import { create } from "zustand";
import { persist } from "zustand/middleware";
import {
  HIDDEN_SNAPSHOT,
  type ReportTarget,
  type SignInFailure,
  type SocialProfile,
  type SocialSnapshot,
  type StatusMode,
} from "@/lib/api/social-types";
import type { ChatMutes } from "./chat/conversations";
import type { HomeHint } from "./model";

export type SocialPage = "list" | "add" | "settings" | "team" | "chat";

/** The list page's two halves: friends, and the conversations ("Mesajlar"). */
export type ListTab = "friends" | "messages";

/** A crew page shows the crew, or its chat ("Sohbet"). */
export type CrewPane = "crew" | "chat";

/** What "Rapor et" is about, with what the dialog shows of it. */
export interface ReportRequest {
  target: ReportTarget;
  /** The player, the crew or the message's sender, as the lists show them. */
  name: string;
  /** Their picture (a player or a sender). */
  person?: Pick<SocialProfile, "name" | "avatar" | "avatarUrl">;
  /** A crew's look. */
  crew?: { emblem: string; color: string; frame: string };
  /** The reported message's text. */
  text?: string;
  /** Reported from a plan card (FIX2): the plan's time as shown, and its note. */
  plan?: { when: string; note?: string };
  /** Reported from a reaction's list (CHAT2): the reaction. */
  reaction?: { e: string };
  /** A crew invite: the player may report the inviter instead of the crew. */
  inviter?: {
    target: ReportTarget;
    name: string;
    person: Pick<SocialProfile, "name" | "avatar" | "avatarUrl">;
  };
}

/** Where a menu sends the player on the team page: editing, the invite list, or leaving. */
export type TeamIntent = "edit" | "invite" | "leave";

/** Pages deeper in the panel than the list slide in from the right. */
const DEPTH: Record<SocialPage, number> = { list: 0, add: 1, settings: 1, team: 1, chat: 2 };

interface SocialUi {
  /** The latest snapshot from the backend (hidden until the first one arrives). */
  view: SocialSnapshot;
  /** Keeps the newest snapshot: an older one (out-of-order event) is ignored. */
  setView: (view: SocialSnapshot) => void;

  open: boolean;
  page: SocialPage;
  /** Direction of the last page change: 1 deeper, -1 back. */
  direction: 1 | -1;
  /** A friend code to look up when the add page opens (`projects://friend/…`). */
  prefillCode: string | null;
  /** A crew invite link to look up on "Ekip kur ya da katıl" (`projects://crew/…`, TEAM5). */
  prefillLink: string | null;
  /** Focus goes back here when the panel closes. */
  returnFocus: HTMLElement | null;
  openPanel: (options?: {
    page?: SocialPage;
    code?: string;
    link?: string;
    from?: HTMLElement | null;
  }) => void;
  closePanel: () => void;
  goTo: (page: SocialPage) => void;
  takePrefill: () => string | null;
  takeLink: () => string | null;

  /** The browser page of the sign-in in progress ("Tarayıcıyı yeniden aç"). */
  signInUrl: string | null;
  signInError: SignInFailure | null;
  setSignIn: (url: string | null, error?: SignInFailure | null) => void;

  /** Friend id → when "Birlikte oyna" may be sent again (ms). */
  inviteCooldown: Record<string, number>;
  coolDown: (id: string, until: number) => void;

  /** The new-computer question was answered (or not needed): backups may be written. */
  restoreSettled: boolean;
  setRestoreSettled: (settled: boolean) => void;

  /** Grows when the friends button should light up once, wherever it is (a Home invitation was
   * closed: this is where friends live). */
  entryCue: number;
  cueEntry: () => void;

  /** A new category's name is asked under the list (`friend` goes into it once it is named). */
  categoryDraft: { friend?: string } | null;
  setCategoryDraft: (draft: { friend?: string } | null) => void;

  /** The team page opens at this (a menu's "Takımı düzenle", "Davet et", "Takımdan ayrıl"). */
  teamIntent: TeamIntent | null;
  /** Opens the crew page (on `crew`, one of several, when given) and at `intent`. */
  openTeam: (intent?: TeamIntent, crew?: string) => void;
  takeTeamIntent: () => TeamIntent | null;
  /** The crew the crew page shows (several crews): a crew's id, "new" for "Ekip kur ya da
   * katıl", or null for the first (the primary crew). This session only. */
  crewTab: string | null;
  setCrewTab: (tab: string | null) => void;

  /** "Ekiplerini çağır": the picker of crews to call is open, with these ticked. */
  callPicker: string[] | null;
  openCallPicker: (ticked: string[]) => void;
  closeCallPicker: () => void;

  /** A status just chosen, shown until the service confirms it (the old one comes back on failure). */
  statusPending: StatusMode | null;
  setStatusPending: (status: StatusMode | null) => void;

  /** Crew calls whose Home line the player closed with × (this session only; not an answer). */
  closedCalls: string[];
  closeCall: (id: string) => void;
  /** Crew plans whose Home line the player closed with × (this session only; TEAM5). */
  closedPlans: string[];
  closePlan: (id: string) => void;
  /** Home's "Ekibini yanına çağır" was closed with × on this server (`callLineKey`): it comes
   * back on another one. This session only. */
  callLineClosed: string | null;
  closeCallLine: (on: string | null) => void;
  /** "Ekibi çağır" waits until then (ms): the service refused a call that came too soon. */
  callRefusedUntil: number;
  /** When the player last called from here (ms), for the five minutes between calls. */
  lastCallAt: number | null;
  setCallWait: (change: { refusedUntil?: number; lastCallAt?: number }) => void;

  /** The list page shows friends or the conversations (this session only). */
  listTab: ListTab;
  setListTab: (tab: ListTab) => void;
  /** The crew page shows the crew or its chat (this session only). */
  crewPane: CrewPane;
  setCrewPane: (pane: CrewPane) => void;
  /** The conversation the chat page shows, and the page it came from (back goes there). */
  chatConv: string | null;
  chatFrom: SocialPage;
  /** Opens a conversation: a friend's on the chat page, a crew's on its crew page. */
  openChat: (conv: string, options?: { from?: SocialPage }) => void;
  /** "Rapor et" is open about this. */
  report: ReportRequest | null;
  openReport: (request: ReportRequest) => void;
  closeReport: () => void;
}

export const useSocialUi = create<SocialUi>()((set, get) => ({
  view: HIDDEN_SNAPSHOT,
  setView: (view) => {
    if (view.seq < get().view.seq) return;
    set({ view });
    if (!view.enabled && get().open) set({ open: false });
  },

  open: false,
  page: "list",
  direction: 1,
  prefillCode: null,
  prefillLink: null,
  returnFocus: null,
  openPanel: (options) => {
    const page = options?.page ?? "list";
    set((s) => ({
      open: true,
      page,
      direction: DEPTH[page] >= DEPTH[s.page] ? 1 : -1,
      prefillCode: options?.code ?? s.prefillCode,
      // An invite link opens "Ekip kur ya da katıl", where it is looked up.
      ...(options?.link
        ? { prefillLink: options.link, crewTab: "new", crewPane: "crew" as const }
        : {}),
      returnFocus:
        options?.from ??
        (document.activeElement instanceof HTMLElement ? document.activeElement : null),
    }));
  },
  closePanel: () => set({ open: false, categoryDraft: null, teamIntent: null }),
  goTo: (page) =>
    set((s) => ({
      page,
      direction: DEPTH[page] >= DEPTH[s.page] ? 1 : -1,
      categoryDraft: page === "list" ? s.categoryDraft : null,
    })),
  takePrefill: () => {
    const code = get().prefillCode;
    if (code) set({ prefillCode: null });
    return code;
  },
  takeLink: () => {
    const link = get().prefillLink;
    if (link) set({ prefillLink: null });
    return link;
  },

  signInUrl: null,
  signInError: null,
  setSignIn: (signInUrl, error = null) => set({ signInUrl, signInError: error }),

  inviteCooldown: {},
  coolDown: (id, until) => set((s) => ({ inviteCooldown: { ...s.inviteCooldown, [id]: until } })),

  restoreSettled: true,
  setRestoreSettled: (restoreSettled) => set({ restoreSettled }),

  entryCue: 0,
  cueEntry: () => set((s) => ({ entryCue: s.entryCue + 1 })),

  categoryDraft: null,
  setCategoryDraft: (categoryDraft) => set({ categoryDraft }),

  teamIntent: null,
  openTeam: (intent, crew) => {
    get().goTo("team");
    set((s) => ({ teamIntent: intent ?? null, crewTab: crew ?? s.crewTab }));
  },
  takeTeamIntent: () => {
    const intent = get().teamIntent;
    if (intent) set({ teamIntent: null });
    return intent;
  },
  crewTab: null,
  setCrewTab: (crewTab) => set({ crewTab }),

  callPicker: null,
  openCallPicker: (ticked) => set({ callPicker: ticked }),
  closeCallPicker: () => set({ callPicker: null }),

  statusPending: null,
  setStatusPending: (statusPending) => set({ statusPending }),

  callLineClosed: null,
  closeCallLine: (callLineClosed) => set({ callLineClosed }),
  closedCalls: [],
  closeCall: (id) =>
    set((s) =>
      s.closedCalls.includes(id) ? s : { closedCalls: [...s.closedCalls, id].slice(-32) },
    ),
  closedPlans: [],
  closePlan: (id) =>
    set((s) =>
      s.closedPlans.includes(id) ? s : { closedPlans: [...s.closedPlans, id].slice(-32) },
    ),
  callRefusedUntil: 0,
  lastCallAt: null,
  setCallWait: ({ refusedUntil, lastCallAt }) =>
    set((s) => ({
      callRefusedUntil: refusedUntil ?? s.callRefusedUntil,
      lastCallAt: lastCallAt ?? s.lastCallAt,
    })),

  listTab: "friends",
  setListTab: (listTab) => set({ listTab }),
  crewPane: "crew",
  setCrewPane: (crewPane) => set({ crewPane }),
  chatConv: null,
  chatFrom: "list",
  openChat: (conv, options) => {
    const s = get();
    if (conv.startsWith("c:")) {
      // A crew's chat lives on its crew page.
      if (!s.open) s.openPanel({ page: "team" });
      else s.goTo("team");
      set({ crewTab: conv.slice(2), crewPane: "chat" });
      return;
    }
    const from = options?.from ?? (s.open && s.page !== "chat" ? s.page : "list");
    if (!s.open) s.openPanel({ page: "chat" });
    else s.goTo("chat");
    set({ chatConv: conv, chatFrom: from });
  },
  report: null,
  openReport: (report) => set({ report }),
  closeReport: () => set({ report: null }),
}));

/** Friend preferences kept on this computer (and in the settings backup). */
interface SocialPrefs {
  /** The opt-in screen was accepted (Discord sign-in pressed there), Unix ms. */
  consentAt: number | null;
  favorites: string[];
  notifyFavoriteOnline: boolean;
  notifyFavoriteProjects: boolean;
  /**
   * "Çevrimiçi olunca bildir" chosen for one friend: on (true) or off (false). Friends without an
   * entry follow the favourites' setting ("Favorilerim çevrimiçi olunca").
   */
  onlineAlerts: Record<string, boolean>;
  /** "Ekip çağrıları": a teammate's call shows a toast and a notification (on by default). */
  notifyCrewCalls: boolean;
  /** "Ekip arkadaşların Projects'e girince" (off by default). */
  notifyCrewProjects: boolean;
  /** Back up launcher preferences to the friends service. */
  backup: boolean;
  /** Fingerprint of the last uploaded backup (skips uploads that change nothing). */
  backupFingerprint: string | null;
  /** Home's invitations the player closed (only on this computer, not in the backup). */
  closedHints: HomeHint[];
  /** Folded friend categories and the team section (a view of this computer only: the
   * categories themselves live with the account). */
  collapsed: string[];
  /** "Ekip sohbeti bildirimleri": crew chat messages are told like direct ones (off by default:
   * crew chat only counts unread). */
  notifyCrewChat: boolean;
  /** "Katılma istekleri": a request to join a crew the player answers is told (on by default). */
  notifyJoinRequests: boolean;
  /** "Plan hatırlatmaları": "Geliyorum" and "Belki" ring 10 minutes before (on by default). */
  notifyPlans: boolean;
  /** Conversations muted on this computer ("Sessize al"): until when (ms), or null until unmuted. */
  chatMutes: ChatMutes;
  setChatMute: (conv: string, until: number | null | undefined) => void;
  setConsent: () => void;
  toggleFavorite: (id: string) => void;
  setOnlineAlert: (id: string, on: boolean) => void;
  closeHint: (hint: HomeHint) => void;
  toggleCollapsed: (id: string) => void;
  set: <K extends keyof SocialPrefValues>(key: K, value: SocialPrefValues[K]) => void;
}

export type SocialPrefValues = Omit<
  SocialPrefs,
  | "setConsent"
  | "toggleFavorite"
  | "setOnlineAlert"
  | "closeHint"
  | "toggleCollapsed"
  | "set"
  | "setChatMute"
>;

export const useSocialPrefs = create<SocialPrefs>()(
  persist(
    (set) => ({
      consentAt: null,
      favorites: [],
      notifyFavoriteOnline: true,
      notifyFavoriteProjects: true,
      onlineAlerts: {},
      notifyCrewCalls: true,
      notifyCrewProjects: false,
      backup: true,
      backupFingerprint: null,
      closedHints: [],
      collapsed: [],
      notifyCrewChat: false,
      notifyJoinRequests: true,
      notifyPlans: true,
      chatMutes: {},
      setChatMute: (conv, until) =>
        set((s) => {
          const next: Record<string, number | null> = { ...s.chatMutes };
          if (until === undefined) delete next[conv];
          else next[conv] = until;
          // Timed mutes that ended go (the list stays small).
          const now = Date.now();
          for (const [key, value] of Object.entries(next)) {
            if (value !== null && value <= now) delete next[key];
          }
          return { chatMutes: next };
        }),
      setConsent: () => set({ consentAt: Date.now() }),
      toggleFavorite: (id) =>
        set((s) => ({
          favorites: s.favorites.includes(id)
            ? s.favorites.filter((f) => f !== id)
            : [...s.favorites, id],
        })),
      setOnlineAlert: (id, on) => set((s) => ({ onlineAlerts: { ...s.onlineAlerts, [id]: on } })),
      closeHint: (hint) =>
        set((s) => (s.closedHints.includes(hint) ? s : { closedHints: [...s.closedHints, hint] })),
      toggleCollapsed: (id) =>
        set((s) => ({
          collapsed: s.collapsed.includes(id)
            ? s.collapsed.filter((c) => c !== id)
            : [...s.collapsed, id].slice(-64),
        })),
      set: (key, value) => set({ [key]: value } as Partial<SocialPrefs>),
    }),
    { name: "pl.social", version: 1 },
  ),
);
