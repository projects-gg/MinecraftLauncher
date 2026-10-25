// Everything the friends feature runs in the background, mounted once with the main window's
// title bar: the snapshot subscription, notices (toasts and, while the launcher is not in front,
// system notifications), the crew calls' clock, the settings backup, the new-computer question and
// the crew picker of "Ekiplerini çağır"; chat changes (CHAT1) and how a new message is told,
// "Rapor et" and moderators' one-time notes (MOD1); join requests, a request taken, crewmates'
// new plans and the plan reminders (TEAM5); the toast when a hidden crew took the player's tag off
// and "Etiketi tak" on the toast of a request taken (TEAM6).
import { Button, Dialog, toast } from "@pl/ui";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { handleAfkNotice } from "@/features/afk/cloud/plus/notices";
import { WorldShareHost } from "@/features/host/host";
import { shownInGame } from "@/features/overlay/store";
import { useOverlaySync } from "@/features/overlay/use-overlay-sync";
import { api } from "@/lib/api";
import type { BackupMeta, Friend, SocialNotice } from "@/lib/api/social-types";
import { formatRelative } from "@/lib/format";
import i18n from "@/lib/i18n";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { attempt, notifyOutside } from "./actions";
import { sayJoined } from "./badge-actions";
import { useChat } from "./chat/chat-store";
import { chatMutedNow, crewOf } from "./chat/conversations";
import { chatNoticeWay, preview } from "./chat/notify";
import { messageLine } from "./chat/preview";
import { useTypingFeed } from "./chat/typing";
import { showCallToast } from "./crew-call";
import { CrewCallDialog } from "./crew-call-picker";
import { planWhen, withPlanNote } from "./crew-plans";
import { callerIn, callSentenceOf, callWhere } from "./crews";
import { FriendsSheet } from "./friends-sheet";
import { showInviteToast } from "./invite-toast";
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
import { NoticeDialog } from "./notice-dialog";
import { PersonAskDialog } from "./person-menu";
import { PlanReminders } from "./plan-reminders";
import { ReportDialog } from "./report-dialog";
import { useSocialPrefs, useSocialUi } from "./store";
import { useCallTicker } from "./use-call-clock";

/** A friend coming online or joining projects.gg notifies at most this often. */
const FRIEND_NOTICE_GAP_MS = 10 * 60_000;
/** Crewmates joining Projects within this long are told together ("Ayşe ve 2 ekip arkadaşın"). */
const CREW_GATHER_MS = 5_000;

const crewThrottle = new NoticeThrottle(FRIEND_NOTICE_GAP_MS);
/** Crewmates who joined Projects while the gathering window is open, and its timer. */
const crewArrivals: { ids: string[]; timer: number | null } = { ids: [], timer: null };

function crewmateName(id: string): string | null {
  for (const crew of useSocialUi.getState().view.crews) {
    const member = crew.members.find((m) => m.id === id);
    if (member) return member.name;
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
    const text =
      ids.length > 1
        ? i18n.t("teams.notify.crewJoinedMany", { name, count: ids.length - 1 })
        : i18n.t("teams.notify.crewJoined", { name });
    void notifyOutside(text, { onClick: openFriends }).then((sent) => {
      if (sent) return;
      toast(text, {
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
  return useSocialUi.getState().view.friends.find((f: Friend) => f.id === id)?.name ?? null;
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
    const text =
      kind === "projects"
        ? i18n.t("social.notices.onProjects", { name })
        : i18n.t("social.notices.online", { name });
    void notifyOutside(text, { onClick: openFriends }).then((sent) => {
      if (!sent) toast(text);
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
  useSocialUi.getState().openPanel({ page: "list" });
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
    return;
  }
  if (notice.kind === "signInFailed") {
    ui.setSignIn(null, notice.reason);
    return;
  }
  // Cloud AFK's notices (AFK+): their own quiet hours, not the friends status ("Rahatsız etmeyin").
  if (notice.kind === "afkNotice") {
    handleAfkNotice(notice.notice);
    return;
  }
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
      const text = i18n.t("social.notices.request", { name: notice.request.name });
      // The open list already shows it, in place.
      if (listInView()) {
        announce(text);
        break;
      }
      toast(text, {
        action: {
          label: i18n.t("social.notices.show"),
          onClick: () => useSocialUi.getState().openPanel({ page: "list" }),
        },
      });
      break;
    }
    case "friendAdded": {
      const text = i18n.t("social.notices.friendAdded", { name: notice.friend.name });
      if (listInView()) announce(text);
      else toast.success(text);
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
      const text = i18n.t("teams.notices.invite", {
        name: notice.invite.from.name,
        team: notice.invite.name,
      });
      void notifyOutside(text, { onClick: openFriends });
      // The open list already shows it, in place.
      if (listInView()) {
        announce(text);
        break;
      }
      toast(text, {
        action: {
          label: i18n.t("social.notices.show"),
          onClick: () => useSocialUi.getState().openPanel({ page: "list" }),
        },
      });
      break;
    }
    case "crewCall": {
      const text = callSentenceOf(notice.call, useSocialUi.getState().view.crews, i18n.t);
      if (!shownInGame(notice.kind)) {
        void notifyOutside(text, { onClick: () => showCalledCrew(notice.call.crews) });
      }
      announce(text);
      // The open panel shows it already, with its answers (the crew page and the list).
      if (!callInView()) showCallToast(notice.call);
      break;
    }
    case "crewCallJoin": {
      const { name } = callerIn(notice.call, useSocialUi.getState().view.crews);
      const where = callWhere(notice.call);
      const text =
        where.kind === "server"
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
      const text = i18n.t("crewWave2.requests.notice", {
        name: notice.request.name,
        crew: crew.name,
      });
      // The crew's page shows it already, in place.
      if (crewInView(crew.id)) {
        announce(text);
        break;
      }
      void notifyOutside(text, { onClick: () => showCrew(crew.id) });
      toast(text, {
        action: { label: i18n.t("social.notices.show"), onClick: () => showCrew(crew.id) },
      });
      break;
    }
    case "requestAccepted": {
      const text = i18n.t("teams.joined", { name: notice.name });
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
        plan.note,
        i18n.t,
      );
      if (crewInView(plan.crew)) {
        announce(text);
        break;
      }
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

/** A new chat message from someone else: a toast, a system notification, or nothing (notify.ts). */
function showChatMessage(notice: Extract<SocialNotice, { kind: "chatMessage" }>) {
  const ui = useSocialUi.getState();
  const prefs = useSocialPrefs.getState();
  const now = Date.now();
  const crew = notice.crew ? ui.view.crews.find((c) => c.id === notice.crew) : undefined;
  const way = chatNoticeWay({
    kind: notice.crew ? "crew" : "direct",
    status: ui.view.me?.status,
    gameBusy: gameBusy(useLaunch.getState().phase.kind),
    inFront: document.hasFocus(),
    onScreen: chatOnScreen(notice.conv),
    mutedHere: chatMutedNow(notice.conv, prefs.chatMutes, now),
    crewMuted: crew ? crewMutedNow(crew, ui.view.crewNotifications, now) : false,
    crewChatOn: prefs.notifyCrewChat,
  });
  if (way === "none") return;
  const name = notice.name ?? i18n.t("chat.list.someone");
  // A sticker reads "Çıkartma: GG" (CHAT2).
  const text = preview(messageLine(notice.message, i18n.t, i18n.language));
  const line = crew
    ? i18n.t("chat.notices.crew", { name, crew: crew.name, text })
    : i18n.t("chat.notices.direct", { name, text });
  const toastIt = () =>
    toast(line, {
      action: {
        label: i18n.t("chat.notices.reply"),
        onClick: () => useSocialUi.getState().openChat(notice.conv),
      },
    });
  if (way === "toast") {
    toastIt();
    return;
  }
  void notifyOutside(crew ? crew.name : name, {
    body: crew ? `${name}: ${text}` : text,
    onClick: () => useSocialUi.getState().openChat(notice.conv),
  }).then((sent) => {
    if (!sent) toastIt();
  });
}

/** The friends list is on screen (the panel is open on it). */
function listInView(): boolean {
  const ui = useSocialUi.getState();
  return ui.open && ui.page === "list";
}

/** A crew call is on screen: the panel is open on the list or the crew page. */
function callInView(): boolean {
  const ui = useSocialUi.getState();
  return ui.open && (ui.page === "list" || ui.page === "team");
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
        if (account !== undefined) useChat.getState().forget();
        account = view.me?.id;
      }
      useSocialUi.getState().setView(view);
    });
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
      stopNotice();
      stopTagOff();
      stopChat();
    };
  }, []);
}

function currentBackup() {
  const prefs = usePrefs.getState() as unknown as Record<BackupPrefKey, unknown>;
  const social = useSocialPrefs.getState();
  return buildBackup(prefs, {
    favorites: social.favorites,
    notifyFavoriteOnline: social.notifyFavoriteOnline,
    notifyFavoriteProjects: social.notifyFavoriteProjects,
    onlineAlerts: social.onlineAlerts,
    notifyCrewCalls: social.notifyCrewCalls,
    notifyCrewProjects: social.notifyCrewProjects,
    notifyCrewChat: social.notifyCrewChat,
    notifyJoinRequests: social.notifyJoinRequests,
    notifyPlans: social.notifyPlans,
  });
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
      const document = currentBackup();
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
    return () => {
      if (timer !== null) window.clearTimeout(timer);
      stopPrefs();
      stopSocial();
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
      // What came down is what is stored: no upload needed until something changes.
      useSocialPrefs.getState().set("backupFingerprint", backupFingerprint(currentBackup()));
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
    </>
  );
}
