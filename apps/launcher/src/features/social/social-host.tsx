// Everything the friends feature runs in the background, mounted once with the main window's
// title bar: the snapshot subscription, notices (toasts and, while the launcher is not in front,
// system notifications), the crew calls' clock, the settings backup, the new-computer question and
// the crew picker of "Ekiplerini çağır"; chat changes (CHAT1) and how a new message is told,
// "Rapor et" and moderators' one-time notes (MOD1); join requests, a request taken, crewmates'
// new plans and the plan reminders (TEAM5); the toast when a hidden crew took the player's tag off
// and "Etiketi tak" on the toast of a request taken (TEAM6). CHAT-CALM: a conversation's messages
// are told in one notice that changes in place (chat/notify.ts `ChatNotices`), gone once the player
// read them; a crowded crew muted on joining says so once, with "Sesi aç".
//
// SOC-NEXT: nothing someone the player ignores does is told; a friend's notice says their
// nickname; a crew chat message that mentions the player is heard as a reply to them is
// (CHAT-CALM's rule; more than five members mentioned: nobody) and marks "@" in "Mesajlar"; under
// "Yayıncı modu" notices say "Yeni mesaj" in place of the words (and a plan's note stays out).
// Who is ignored and the pinned conversations travel with the settings backup.
import { Button, Dialog, toast } from "@pl/ui";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { handleAfkNotice } from "@/features/afk/cloud/plus/notices";
import { WorldShareHost } from "@/features/host/host";
import { shownInGame } from "@/features/overlay/store";
import { useOverlaySync } from "@/features/overlay/use-overlay-sync";
import { VoiceHost } from "@/features/voice/host";
import { useVoiceUi } from "@/features/voice/store";
import { api } from "@/lib/api";
import type {
  BackupMeta,
  CrewCall,
  Friend,
  SocialNotice,
  TeamMember,
} from "@/lib/api/social-types";
import { voiceApi } from "@/lib/api/voice-api";
import { clearNotification, windowAway } from "@/lib/attention";
import { formatRelative } from "@/lib/format";
import i18n from "@/lib/i18n";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { attempt, notifyOutside } from "./actions";
import { sayJoined } from "./badge-actions";
import { useChat } from "./chat/chat-store";
import { chatMutedNow, crewOf } from "./chat/conversations";
import {
  ChatNotices,
  type ChatNoticeWay,
  chatNoticeWay,
  type GatheredChatNotice,
  preview,
} from "./chat/notify";
import { messageLine } from "./chat/preview";
import { useTypingFeed } from "./chat/typing";
import { showCallToast } from "./crew-call";
import { CrewCallDialog } from "./crew-call-picker";
import { unmuteCrew } from "./crew-notify";
import { planWhen, withPlanNote } from "./crew-plans";
import { callerIn, callSentenceOf, callWhere } from "./crews";
import { FriendsSheet } from "./friends-sheet";
import { homeShowsCall } from "./home-strip";
import { useIdentityUi, useKeepMicrosoftOffer } from "./identity-store";
import { showInviteToast, showTeamInviteToast } from "./invite-toast";
import { gameBusy, joinProjects } from "./join";
import { JoinDialog } from "./join-dialog";
import { dayMonth } from "./limits";
import {
  BACKUP_PREFS,
  type BackupPrefKey,
  backupFingerprint,
  buildBackup,
  NoticeThrottle,
  readBackup,
  wantsNotice,
} from "./model";
import { crewMutedNow, crewmateHeardIn, mutedNow } from "./mute";
import { socNextOn } from "./next/flag";
import { SocNextHost } from "./next/host";
import { fromIgnored } from "./next/ignore";
import { markMention } from "./next/mention-marks";
import { mentionOf } from "./next/mentions";
import { noticeName, realNameOf } from "./next/nickname";
import { useSocNextPrefs } from "./next/prefs";
import { shownHost, streamerOnNow } from "./next/streamer";
import { NoticeDialog } from "./notice-dialog";
import { MarkedSentence } from "./person-mark";
import { PersonAskDialog } from "./person-menu";
import { PlanReminders } from "./plan-reminders";
import { ReportDialog } from "./report-dialog";
import { friendListShown, useSocialPrefs, useSocialUi } from "./store";
import { useCallTicker } from "./use-call-clock";

/** A friend coming online or joining projects.gg notifies at most this often. */
const FRIEND_NOTICE_GAP_MS = 10 * 60_000;
/** Crewmates joining Projects within this long are told together ("Ayşe ve 2 ekip arkadaşın"). */
const CREW_GATHER_MS = 5_000;

const crewThrottle = new NoticeThrottle(FRIEND_NOTICE_GAP_MS);
/** Crewmates who joined Projects while the gathering window is open, and its timer. */
const crewArrivals: { ids: string[]; timer: number | null } = { ids: [], timer: null };

function crewmateName(id: string): string | null {
  return crewmate(id)?.name ?? null;
}

function crewmate(id: string): TeamMember | null {
  for (const crew of useSocialUi.getState().view.crews) {
    const member = crew.members.find((m) => m.id === id);
    if (member) return member;
  }
  return null;
}

/** "Ayşe Projects'e girdi" (or "Ayşe ve 2 ekip arkadaşın…"), gathered over 5 s, with "Katıl". */
function showCrewArrival(id: string) {
  if (!crewThrottle.allow(`crew:${id}`, Date.now())) return;
  if (!crewArrivals.ids.includes(id)) crewArrivals.ids.push(id);
  if (crewArrivals.timer !== null) return;
  crewArrivals.timer = window.setTimeout(() => {
    const { view } = useSocialUi.getState();
    const now = Date.now();
    // Muted meanwhile ("Sessize al" while the arrivals were gathered): nothing to say of them.
    const ids = crewArrivals.ids.filter(
      (one) => crewmateName(one) && crewmateHeardIn(one, view.crews, now),
    );
    crewArrivals.ids = [];
    crewArrivals.timer = null;
    const first = ids[0];
    if (!first || mutedNow(view.crewNotifications, now)) return;
    const name = crewmateName(first) ?? "";
    const sentence = (who: string) =>
      ids.length > 1
        ? i18n.t("teams.notify.crewJoinedMany", { name: who, count: ids.length - 1 })
        : i18n.t("teams.notify.crewJoined", { name: who });
    const text = sentence(name);
    void notifyOutside(text, { onClick: openFriends }).then((sent) => {
      if (sent) return;
      // The first crewmate's badge right after their name (BADGES-UI; the system's notice is text).
      toast(<MarkedSentence text={sentence} name={name} marks={crewmate(first)?.marks} />, {
        action: { label: i18n.t("social.actions.join"), onClick: () => void joinProjects() },
      });
    });
  }, CREW_GATHER_MS);
}
/** Preference changes are gathered for a while before one upload (60 writes a day at most). */
const BACKUP_DEBOUNCE_MS = 3 * 60_000;
const BACKUP_FIRST_CHECK_MS = 20_000;

const friendThrottle = new NoticeThrottle(FRIEND_NOTICE_GAP_MS);
/** Online and on-projects notices of one presence change arrive together: show the better one. */
const pendingFriendNotices = new Map<string, { kind: "online" | "projects"; timer: number }>();

function friendName(id: string): string | null {
  return friendOf(id)?.name ?? null;
}

function friendOf(id: string): Friend | null {
  return useSocialUi.getState().view.friends.find((f: Friend) => f.id === id) ?? null;
}

function showFriendNotice(id: string, kind: "online" | "projects") {
  const pending = pendingFriendNotices.get(id);
  if (pending) {
    window.clearTimeout(pending.timer);
    if (pending.kind === "projects") kind = "projects";
  }
  const timer = window.setTimeout(() => {
    pendingFriendNotices.delete(id);
    const name = friendName(id);
    if (!name || !friendThrottle.allow(id, Date.now())) return;
    const sentence = (who: string) =>
      kind === "projects"
        ? i18n.t("social.notices.onProjects", { name: who })
        : i18n.t("social.notices.online", { name: who });
    const text = sentence(name);
    // OVL-SOC2: a game whose overlay shows it as its card says it there (one notice, never two).
    const shown = shownInGame(kind === "projects" ? "friendOnProjects" : "friendOnline");
    const outside = shown ? Promise.resolve(false) : notifyOutside(text, { onClick: openFriends });
    void outside.then((sent) => {
      if (!sent) toast(<MarkedSentence text={sentence} name={name} marks={friendOf(id)?.marks} />);
    });
  }, 400);
  pendingFriendNotices.set(id, { kind, timer });
}

/** "Göster" on a crew's notice: the panel opens on that crew's page. */
function showCrew(id: string) {
  const ui = useSocialUi.getState();
  ui.openPanel({ page: "team" });
  ui.setCrewTab(id);
  ui.setCrewPane("crew");
}

/** A click on a friend's or an invite's system notification: the friends list. */
function openFriends() {
  useSocialUi.getState().openPanel({ page: "list", tab: "friends" });
}

/** A click on a crew call's system notification: the first crew it names (else the list). */
function showCalledCrew(crews: string[]) {
  const crew = crews[0];
  if (crew) showCrew(crew);
  else openFriends();
}

/** That crew's page is on screen (the panel open on it). */
function crewInView(id: string): boolean {
  const ui = useSocialUi.getState();
  if (!ui.open || ui.page !== "team" || ui.crewPane !== "crew") return false;
  const crews = ui.view.crews;
  const shown = ui.view.crew.multi
    ? (crews.find((crew) => crew.id === ui.crewTab) ?? crews[0])
    : crews[0];
  return shown?.id === id;
}

function handleNotice(notice: SocialNotice) {
  const ui = useSocialUi.getState();
  // Flow notices drive the UI whatever the status.
  if (notice.kind === "signedIn") {
    ui.setSignIn(null, null);
    const restore = notice.restore;
    ui.setRestoreSettled(!restore);
    if (restore) restoreOffer.current?.(restore);
    // SOC-ID: a new account with a placeholder name is asked for one, once.
    if (notice.askName) useIdentityUi.getState().setNameStep(true);
    return;
  }
  if (notice.kind === "signInFailed") {
    ui.setSignIn(null, notice.reason);
    return;
  }
  // SOC-ID: a link or a removal finished ("Microsoft bağlandı."), or what went wrong is said under
  // "Bağlı hesaplar".
  if (notice.kind === "identityChanged") {
    const provider = i18n.t(`identity.providers.${notice.provider}`);
    const text = i18n.t(notice.linked ? "identity.done.linked" : "identity.done.removed", {
      provider,
    });
    useIdentityUi.getState().setFailure(null);
    toast.success(text);
    announce(text);
    return;
  }
  if (notice.kind === "identityFailed") {
    useIdentityUi.getState().setFailure({ provider: notice.provider, reason: notice.reason });
    return;
  }
  // Cloud AFK's notices (AFK+): their own quiet hours, not the friends status ("Rahatsız etmeyin").
  if (notice.kind === "afkNotice") {
    handleAfkNotice(notice.notice);
    return;
  }
  // SOC-NEXT: someone the player ignores is never told (their rows still show it).
  if (socNextOn() && fromIgnored(notice, new Set(useSocNextPrefs.getState().ignored))) return;
  const prefs = useSocialPrefs.getState();
  if (
    !wantsNotice(
      notice,
      ui.view.me?.status,
      prefs,
      ui.view.crewNotifications,
      Date.now(),
      ui.view.crews,
    )
  ) {
    return;
  }
  switch (notice.kind) {
    case "invite": {
      showInviteToast(notice.invite);
      const name = notice.invite.name ?? i18n.t("social.notices.someone");
      // In game the overlay's card says it (one notification, never two).
      if (!shownInGame(notice.kind)) {
        void notifyOutside(i18n.t("social.notices.invite", { name }), { onClick: openFriends });
      }
      announce(i18n.t("social.notices.invite", { name }));
      break;
    }
    case "request": {
      const sentence = (who: string) => i18n.t("social.notices.request", { name: who });
      const text = sentence(notice.request.name);
      // The open list already shows it, in place.
      if (listInView()) {
        announce(text);
        break;
      }
      // Their badge right after their name (BADGES-UI): a stranger's request says who is verified.
      toast(
        <MarkedSentence text={sentence} name={notice.request.name} marks={notice.request.marks} />,
        {
          action: {
            label: i18n.t("social.notices.show"),
            onClick: openFriends,
          },
        },
      );
      break;
    }
    case "friendAdded": {
      const sentence = (who: string) => i18n.t("social.notices.friendAdded", { name: who });
      const text = sentence(notice.friend.name);
      if (listInView()) announce(text);
      else
        toast.success(
          <MarkedSentence text={sentence} name={notice.friend.name} marks={notice.friend.marks} />,
        );
      break;
    }
    case "friendOnline":
      showFriendNotice(notice.id, "online");
      break;
    case "friendOnProjects":
      showFriendNotice(notice.id, "projects");
      break;
    case "signedOutRemotely":
      toast(i18n.t("social.notices.signedOut"), {
        action: {
          label: i18n.t("social.notices.signIn"),
          onClick: () => useSocialUi.getState().openPanel(),
        },
      });
      break;
    case "teamInvite": {
      const sentence = (who: string) =>
        i18n.t("teams.notices.invite", { name: who, team: notice.invite.name });
      const text = sentence(notice.invite.from.name);
      // OVL-SOC2: in game the overlay's card says it.
      if (!shownInGame(notice.kind)) void notifyOutside(text, { onClick: openFriends });
      // The open list already shows it, in place.
      if (listInView()) {
        announce(text);
        break;
      }
      // With the inviter's face and the crew's mark, as a call's toast (SMALLS-L).
      showTeamInviteToast(notice.invite);
      break;
    }
    case "crewCall": {
      const text = callSentenceOf(notice.call, useSocialUi.getState().view.crews, i18n.t);
      if (!shownInGame(notice.kind)) {
        void notifyOutside(text, { onClick: () => showCalledCrew(notice.call.crews) });
      }
      announce(text);
      // The open panel shows it already, with its answers (the crew page and the list), or
      // Home's line does.
      if (!callInView(notice.call)) showCallToast(notice.call);
      break;
    }
    case "crewCallJoin": {
      const { name } = callerIn(notice.call, useSocialUi.getState().view.crews);
      const where = callWhere(notice.call);
      const text =
        where.kind === "server" && shownHost(where.host)
          ? i18n.t("crews.call.arrivedAt", { name, host: where.host })
          : i18n.t("teams.call.arrived", { name });
      showCallToast(notice.call, true);
      void notifyOutside(text, { onClick: () => showCalledCrew(notice.call.crews) });
      announce(text);
      break;
    }
    case "crewmateOnProjects":
      showCrewArrival(notice.id);
      break;
    case "teamGone": {
      const name = notice.name;
      const text =
        notice.staff && notice.reason === "disbanded"
          ? name
            ? i18n.t("safety.crewDisbanded", { team: name })
            : i18n.t("safety.crewDisbandedUnnamed")
          : notice.reason === "kicked"
            ? name
              ? i18n.t("teams.notices.kicked", { team: name })
              : i18n.t("teams.notices.kickedUnnamed")
            : name
              ? i18n.t("teams.notices.disbanded", { team: name })
              : i18n.t("teams.notices.disbandedUnnamed");
      toast(text);
      break;
    }
    case "chatMessage":
      showChatMessage(notice);
      break;
    case "joinRequest": {
      const crew = ui.view.crews.find((one) => one.id === notice.crew);
      if (!crew) break;
      const sentence = (who: string) =>
        i18n.t("crewWave2.requests.notice", { name: who, crew: crew.name });
      const text = sentence(notice.request.name);
      // The crew's page shows it already, in place.
      if (crewInView(crew.id)) {
        announce(text);
        break;
      }
      if (!shownInGame(notice.kind)) void notifyOutside(text, { onClick: () => showCrew(crew.id) });
      const asker = notice.request;
      toast(<MarkedSentence text={sentence} name={asker.name} marks={asker.marks} />, {
        action: { label: i18n.t("social.notices.show"), onClick: () => showCrew(crew.id) },
      });
      break;
    }
    case "crowdedCrewMuted": {
      const crew = ui.view.crews.find((one) => one.id === notice.crew);
      if (!crew) break;
      const text = i18n.t("chatCalm.crowded", { crew: crew.name });
      announce(text);
      toast(text, {
        action: { label: i18n.t("teams.mute.undo"), onClick: () => void unmuteCrew(crew.id) },
      });
      break;
    }
    case "requestAccepted": {
      const text = i18n.t("teams.joined", { name: notice.name });
      if (!shownInGame(notice.kind))
        void notifyOutside(text, { onClick: () => showCrew(notice.crew) });
      // "Göster", and "Etiketi tak" while the player wears no tag (TEAM6).
      sayJoined(text, () => notice.crew, {
        label: i18n.t("social.notices.show"),
        onClick: () => showCrew(notice.crew),
      });
      break;
    }
    case "crewPlan": {
      const { plan } = notice;
      const text = withPlanNote(
        i18n.t("crewWave2.plans.notice", {
          name: plan.byName ?? i18n.t("social.notices.someone"),
          when: planWhen(plan.at, Date.now(), i18n.t, true),
        }),
        // SOC-NEXT "Yayıncı modu": a plan's note is the player's words, kept off a stream.
        streamerOnNow() ? undefined : plan.note,
        i18n.t,
      );
      if (crewInView(plan.crew)) {
        announce(text);
        break;
      }
      if (!shownInGame(notice.kind))
        void notifyOutside(text, { onClick: () => showCrew(plan.crew) });
      toast(text, {
        action: { label: i18n.t("social.notices.show"), onClick: () => showCrew(plan.crew) },
      });
      break;
    }
    case "restricted": {
      const text =
        notice.until !== undefined
          ? i18n.t("safety.restricted.until", { date: dayMonth(notice.until) })
          : i18n.t("safety.restricted.forever");
      toast(text, {
        action: {
          label: i18n.t("social.notices.show"),
          onClick: () => useSocialUi.getState().openPanel(),
        },
      });
      break;
    }
  }
}

/** A conversation is on screen: a friend's chat page, or the crew page on that crew's "Sohbet". */
function chatOnScreen(conv: string): boolean {
  const ui = useSocialUi.getState();
  if (!ui.open) return false;
  if (ui.page === "chat") return ui.chatConv === conv;
  const team = crewOf(conv);
  if (ui.page !== "team" || ui.crewPane !== "chat" || !team) return false;
  const shown =
    ui.crewTab && ui.view.crews.some((c) => c.id === ui.crewTab)
      ? ui.crewTab
      : ui.view.crews[0]?.id;
  return shown === team;
}

/** A new chat message from someone else: a toast, a system notification, or nothing (notify.ts);
 * a conversation's messages go together, a moment later, in one notice (CHAT-CALM). */
function showChatMessage(notice: Extract<SocialNotice, { kind: "chatMessage" }>) {
  const ui = useSocialUi.getState();
  const prefs = useSocialPrefs.getState();
  const now = Date.now();
  const crew = notice.crew ? ui.view.crews.find((c) => c.id === notice.crew) : undefined;
  const me = ui.view.me?.id;
  // SOC-NEXT: a crew chat message that mentions the player (by the names the crew knows) is heard
  // as a reply to them is; more than five members mentioned tells nobody (its "@" stays).
  const mention =
    socNextOn() && crew
      ? mentionOf(
          notice.message.text,
          me,
          crew.members.map((m) => ({ id: m.id, name: realNameOf(m) })),
        )
      : "none";
  if (mention !== "none") markMention(notice.conv);
  const way = chatNoticeWay({
    kind: notice.crew ? "crew" : "direct",
    status: ui.view.me?.status,
    gameBusy: gameBusy(useLaunch.getState().phase.kind),
    // FIX-209: shown, not minimized and the page focused, as every other notice asks it: WebView2's page can keep the
    // focus while the window waits in the tray or minimized, and a direct message went to a toast nobody saw.
    inFront: !windowAway(),
    onScreen: chatOnScreen(notice.conv),
    mutedHere: chatMutedNow(notice.conv, prefs.chatMutes, now),
    crewMuted: crew ? crewMutedNow(crew, ui.view.crewNotifications, now) : false,
    crewChatOn: prefs.notifyCrewChat,
    toMe: Boolean(me && notice.message.reply?.from === me) || mention === "me",
  });
  if (way === "none") return;
  const name = socNextOn() ? noticeName(notice.message.from, notice.name, ui.view) : notice.name;
  chatNotices.add(notice.conv, {
    name: name ?? i18n.t("chat.list.someone"),
    // A sticker reads "Çıkartma: GG" (CHAT2); under "Yayıncı modu" the words stay off a stream.
    text: streamerOnNow()
      ? i18n.t("socNext.streamer.newMessage")
      : preview(messageLine(notice.message, i18n.t, i18n.language)),
    crew: crew?.name,
  });
}

/** How a gathered notice goes now (each message passed the crew's rules when it came). */
function chatWayNow(conv: string): ChatNoticeWay {
  const ui = useSocialUi.getState();
  const prefs = useSocialPrefs.getState();
  if (
    ui.view.me?.status === "dnd" ||
    gameBusy(useLaunch.getState().phase.kind) ||
    chatMutedNow(conv, prefs.chatMutes, Date.now())
  ) {
    return "none";
  }
  if (!windowAway()) return chatOnScreen(conv) ? "none" : "toast";
  return "system";
}

/** "Kaan: selam", or "Kaan: 5 yeni mesaj" (in a crew's chat the crew's name says where). */
function gatheredWords(notice: GatheredChatNotice) {
  const { last, count } = notice;
  const many = i18n.t("chatCalm.newMessages", { count });
  const title = last.crew ?? last.name;
  const body = count > 1 ? many : last.crew ? `${last.name}: ${last.text}` : last.text;
  const line =
    count > 1
      ? i18n.t("chat.notices.direct", { name: title, text: many })
      : last.crew
        ? i18n.t("chat.notices.crew", { name: last.name, crew: last.crew, text: last.text })
        : i18n.t("chat.notices.direct", { name: last.name, text: last.text });
  return { title, body, line };
}

/** One in-app toast per conversation: a new burst changes the one shown. */
function chatToast(conv: string, line: string) {
  toast(line, {
    id: `chat:${conv}`,
    action: {
      label: i18n.t("chat.notices.reply"),
      onClick: () => useSocialUi.getState().openChat(conv),
    },
  });
}

const chatNotices = new ChatNotices({
  way: chatWayNow,
  show(notice, way) {
    const { title, body, line } = gatheredWords(notice);
    if (way === "toast") {
      chatToast(notice.conv, line);
      return;
    }
    void notifyOutside(title, {
      body,
      // Opens at "Yeni mesajlar": the first message the player has not read.
      onClick: () => useSocialUi.getState().openChat(notice.conv),
      place: { tag: notice.conv, popup: notice.popup },
    }).then((sent) => {
      if (!sent) chatToast(notice.conv, line);
    });
  },
  clear(conv) {
    toast.dismiss(`chat:${conv}`);
    clearNotification(conv);
  },
});

/** Unread counts of the conversations with a notice, as last seen. */
const noticeUnread = new Map<string, number>();

/** A conversation's notice goes once the player read it (here or on another device) or it is on
 * screen in front of them. */
function settleChatNotices() {
  const ui = useSocialUi.getState();
  for (const conv of chatNotices.open()) {
    const unread = ui.view.chat?.conversations.find((c) => c.id === conv)?.unread ?? 0;
    const before = noticeUnread.get(conv);
    noticeUnread.set(conv, unread);
    const read = before !== undefined && before > 0 && unread === 0;
    if (read || (chatOnScreen(conv) && !windowAway())) {
      noticeUnread.delete(conv);
      chatNotices.seen(conv);
    }
  }
}

/** The friends list is on screen (the panel is open on it, on "Arkadaşlar" when chat is on). */
function listInView(): boolean {
  return friendListShown(useSocialUi.getState());
}

/** A crew call is on screen with its answers: the panel is open on the list or the crew page, or
 * Home's line shows it (SMALL12: the line and a toast said the same call twice). */
function callInView(call: CrewCall): boolean {
  const ui = useSocialUi.getState();
  if (ui.open) return friendListShown(ui) || ui.page === "team";
  return homeShowsCall(call);
}

/** Set by the mounted restore dialog: opens it with a backup's details. */
const restoreOffer: { current: ((backup: BackupMeta) => void) | null } = { current: null };

function useSocialSync() {
  useEffect(() => {
    let alive = true;
    // Another account (or none): the conversations shown here were the old one's.
    let account: string | undefined;
    const stopView = api.social.onView((view) => {
      if (view.me?.id !== account) {
        if (account !== undefined) {
          useChat.getState().forget();
          noticeUnread.clear();
          chatNotices.forget();
        }
        account = view.me?.id;
      }
      useSocialUi.getState().setView(view);
    });
    // CHAT-CALM: a conversation read, or opened in front of the player, takes its notice away.
    const stopSettle = useSocialUi.subscribe(settleChatNotices);
    window.addEventListener("focus", settleChatNotices);
    const stopNotice = api.social.onNotice(handleNotice);
    // The leader hid the crew whose tag the player wore (TEAM6 §5.3): one toast.
    const stopTagOff = api.social.onTagOff(() => {
      const text = i18n.t("team6.hiddenToast");
      announce(text);
      toast(text);
    });
    const stopChat = api.social.onChat((change) => useChat.getState().apply(change));
    void api.social
      .state()
      .then((view) => {
        if (alive) useSocialUi.getState().setView(view);
      })
      .catch(() => {
        // The feature stays hidden; nothing else depends on it.
      });
    return () => {
      alive = false;
      stopView();
      stopSettle();
      window.removeEventListener("focus", settleChatNotices);
      stopNotice();
      stopTagOff();
      stopChat();
    };
  }, []);
}

async function currentBackup() {
  const prefs = usePrefs.getState() as unknown as Record<BackupPrefKey, unknown>;
  const social = useSocialPrefs.getState();
  // "Sesli sohbet" (VOICE): the launcher keeps it; without an answer the rest still goes.
  const voice = await voiceApi.backup().catch(() => null);
  return buildBackup(
    prefs,
    {
      favorites: social.favorites,
      notifyFavoriteOnline: social.notifyFavoriteOnline,
      notifyFavoriteProjects: social.notifyFavoriteProjects,
      onlineAlerts: social.onlineAlerts,
      notifyCrewCalls: social.notifyCrewCalls,
      notifyCrewProjects: social.notifyCrewProjects,
      notifyCrewChat: social.notifyCrewChat,
      notifyJoinRequests: social.notifyJoinRequests,
      notifyPlans: social.notifyPlans,
      // SOC-NEXT: who the player ignores and the pinned conversations (ids only).
      ignored: useSocNextPrefs.getState().ignored,
      pinnedChats: useSocNextPrefs.getState().pinnedChats,
    },
    voice,
  );
}

/** Uploads launcher preferences a while after they change (signed in, backup on, and only once
 * the new-computer question is answered). */
function useBackupSync() {
  const ready = useSocialUi(
    (s) =>
      s.view.enabled &&
      s.view.auth === "signedIn" &&
      Boolean(s.view.me) &&
      s.restoreSettled &&
      // A new Discord account keeps no backup (SPAM1); storage protection makes uploads wait.
      !s.view.me?.limited?.items.some((item) => item.key === "settingsBackup") &&
      !(s.view.backupWaitsUntil && s.view.backupWaitsUntil * 1000 > Date.now()),
  );
  const enabled = useSocialPrefs((s) => s.backup);
  useEffect(() => {
    if (!ready || !enabled) return;
    let timer: number | null = null;
    const upload = async () => {
      timer = null;
      const document = await currentBackup();
      const fingerprint = backupFingerprint(document);
      const stored = useSocialUi.getState().view.me?.backup;
      if (stored && fingerprint === useSocialPrefs.getState().backupFingerprint) return;
      try {
        await api.social.uploadBackup(document as unknown as Record<string, unknown>);
        useSocialPrefs.getState().set("backupFingerprint", fingerprint);
      } catch {
        // Tried again after the next change (or the next start).
      }
    };
    const schedule = (delay: number) => {
      if (timer !== null) window.clearTimeout(timer);
      timer = window.setTimeout(() => void upload(), delay);
    };
    schedule(BACKUP_FIRST_CHECK_MS);
    const keys = Object.keys(BACKUP_PREFS) as BackupPrefKey[];
    const stopPrefs = usePrefs.subscribe((now, before) => {
      const a = now as unknown as Record<string, unknown>;
      const b = before as unknown as Record<string, unknown>;
      if (keys.some((key) => a[key] !== b[key])) schedule(BACKUP_DEBOUNCE_MS);
    });
    const stopSocial = useSocialPrefs.subscribe((now, before) => {
      if (
        now.favorites !== before.favorites ||
        now.notifyFavoriteOnline !== before.notifyFavoriteOnline ||
        now.notifyFavoriteProjects !== before.notifyFavoriteProjects ||
        now.onlineAlerts !== before.onlineAlerts ||
        now.notifyCrewCalls !== before.notifyCrewCalls ||
        now.notifyCrewProjects !== before.notifyCrewProjects ||
        now.notifyCrewChat !== before.notifyCrewChat
      ) {
        schedule(BACKUP_DEBOUNCE_MS);
      }
    });
    const stopNext = useSocNextPrefs.subscribe((now, before) => {
      if (now.ignored !== before.ignored || now.pinnedChats !== before.pinnedChats) {
        schedule(BACKUP_DEBOUNCE_MS);
      }
    });
    // Voice chat's settings and per-person choices, changed here or in the game's overlay.
    const stopVoice = useVoiceUi.subscribe((now, before) => {
      if (now.state.backupRev !== before.state.backupRev) schedule(BACKUP_DEBOUNCE_MS);
    });
    return () => {
      if (timer !== null) window.clearTimeout(timer);
      stopPrefs();
      stopSocial();
      stopNext();
      stopVoice();
    };
  }, [ready, enabled]);
}

/** "Bu bilgisayara ayarlarını getireyim mi?" right after signing in on another computer. */
function RestoreDialog() {
  const { t } = useTranslation();
  const [offer, setOffer] = useState<BackupMeta | null>(null);
  const [busy, setBusy] = useState(false);
  const settle = useSocialUi((s) => s.setRestoreSettled);
  const offerRef = useRef(setOffer);
  offerRef.current = setOffer;
  useEffect(() => {
    restoreOffer.current = (backup) => offerRef.current(backup);
    return () => {
      restoreOffer.current = null;
    };
  }, []);

  const finish = () => {
    setOffer(null);
    settle(true);
  };

  const apply = async () => {
    setBusy(true);
    const data = await attempt(() => api.social.downloadBackup());
    setBusy(false);
    if (data === undefined) return;
    if (data) {
      const read = readBackup(data);
      const prefs = usePrefs.getState();
      for (const [key, value] of Object.entries(read.prefs)) {
        prefs.set(key as BackupPrefKey, value as never);
      }
      const social = useSocialPrefs.getState();
      if (read.friends.favorites) social.set("favorites", read.friends.favorites);
      if (read.friends.notifyFavoriteOnline !== undefined) {
        social.set("notifyFavoriteOnline", read.friends.notifyFavoriteOnline);
      }
      if (read.friends.notifyFavoriteProjects !== undefined) {
        social.set("notifyFavoriteProjects", read.friends.notifyFavoriteProjects);
      }
      if (read.friends.onlineAlerts) social.set("onlineAlerts", read.friends.onlineAlerts);
      if (read.friends.notifyCrewCalls !== undefined) {
        social.set("notifyCrewCalls", read.friends.notifyCrewCalls);
      }
      if (read.friends.notifyCrewProjects !== undefined) {
        social.set("notifyCrewProjects", read.friends.notifyCrewProjects);
      }
      if (read.friends.notifyCrewChat !== undefined) {
        social.set("notifyCrewChat", read.friends.notifyCrewChat);
      }
      if (read.friends.notifyJoinRequests !== undefined) {
        social.set("notifyJoinRequests", read.friends.notifyJoinRequests);
      }
      if (read.friends.notifyPlans !== undefined) {
        social.set("notifyPlans", read.friends.notifyPlans);
      }
      // SOC-NEXT: who the player ignores and the pinned conversations.
      useSocNextPrefs.getState().restore({
        ignored: read.friends.ignored,
        pinnedChats: read.friends.pinnedChats,
      });
      // "Sesli sohbet": the launcher reads and bounds it (this computer's devices stay); a part it
      // cannot read leaves the rest applied.
      if (read.voice) await voiceApi.restore(read.voice).catch(() => undefined);
      // What came down is what is stored: no upload needed until something changes.
      useSocialPrefs.getState().set("backupFingerprint", backupFingerprint(await currentBackup()));
      toast.success(t("social.restore.applied"));
    }
    finish();
  };

  return (
    <Dialog
      open={offer !== null}
      onOpenChange={(open) => {
        if (!open && !busy) finish();
      }}
      title={t("social.restore.title")}
      description={
        offer
          ? t("social.restore.body", {
              device: offer.device,
              when: formatRelative(new Date(offer.updatedAt * 1000).toISOString()),
            })
          : undefined
      }
      closeLabel={t("common.close")}
      width={540}
      dismissible={!busy}
      // Asked once, right after signing in: a stray click beside it must not answer "not now".
      dismissOnOutsideClick={false}
      footer={
        <>
          <Button variant="ghost" disabled={busy} onClick={finish}>
            {t("social.restore.skip")}
          </Button>
          <Button variant="primary" disabled={busy} onClick={() => void apply()}>
            {t("social.restore.apply")}
          </Button>
        </>
      }
    />
  );
}

export function SocialHost() {
  useSocialSync();
  useBackupSync();
  useCallTicker();
  // The in-game overlay follows the friends notification choices (features/overlay).
  useOverlaySync();
  // "Yazıyor…" frames are kept from the start, so a list or a conversation that opens shows who
  // is writing at once (CHAT2; a pushed event, nothing is asked for).
  useTypingFeed();
  // SOC-ID: how the service offers Microsoft is kept for the next start (Home's invitation).
  useKeepMicrosoftOffer();
  return (
    <>
      <FriendsSheet />
      <RestoreDialog />
      <JoinDialog />
      <CrewCallDialog />
      <ReportDialog />
      <PersonAskDialog />
      <NoticeDialog />
      <PlanReminders />
      {/* "Dünyanı arkadaşlarına aç" (HOST1): its view, notices and windows. */}
      <WorldShareHost />
      {/* "Sesli sohbet" (VOICE): its state, rings, notices and the room panel. */}
      <VoiceHost />
      {/* SOC-NEXT: its switch, nicknames, the timed status, "Yayıncı modu". */}
      <SocNextHost />
    </>
  );
}
