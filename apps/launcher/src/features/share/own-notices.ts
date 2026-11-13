// What screen sharing says in toasts and to screen readers (SHARE, SQOL sqol.md §1.2; the notices
// of sqol-r.md "API"), mounted once with the launcher window's voice chat. Every `share-notice`
// comes here; the player's own shares are also watched, so whichever comes first (the launcher's
// notice or its state) says it once:
// - the window, screen or game a share showed closed: with the room panel open its card asks
//   ("Başka Ekran Seç", "Paylaşımı Durdur"); closed, a toast asks the same for the minute it waits
//   (opening the panel hands the question back to the card, closing it asks in a toast again). How
//   it ended then: "Artık … paylaşılıyor." (a switch, or the launcher's new game window), "Paylaşım
//   sürüyor." (by itself), "Paylaşım durdu." (the minute ran out). Over the open panel's card a
//   sentence the card already shows is a status message, not a toast hiding its buttons;
// - a switch that failed: "Bu pencere paylaşılamadı." (the old source goes on);
// - a minimised window and its coming back: a status message only (the card shows it; a toast at
//   each Alt+Tab out of a full screen game would be noise).
import { toast } from "@pl/ui";
import { useEffect } from "react";
import { useVoiceUi } from "@/features/voice/store";
import i18n from "@/lib/i18n";
import { shareApi } from "@/share/api";
import type { ShareAsked, ShareNotice, ShareRefusal, VoiceMyShare } from "@/share/types";
import {
  askOwnQuestion,
  changeRefusedText,
  droppedOwnQuestion,
  forgetOwnQuestions,
  lostWhyOf,
  noticeText,
  ownQuestion,
  ownToast,
  ownToastId,
  refusalText,
  sayOwnSwitchFailed,
  switchAnswered,
  switchedText,
  switchFailedText,
} from "./actions";
import { forgetOwnPreview, forgetOwnStats } from "./own-live";
import { sayShare } from "./own-say";
import {
  clearLost,
  dropRefused,
  dropRefusedChange,
  isLost,
  type LostWhy,
  lostUntil,
  markLost,
  type OwnPatch,
  type OwnStatus,
  ownStatus,
  useOwnUi,
} from "./own-store";

/** Shares whose minute ran out just now (ms), so the launcher's notice is not said twice. */
const endedLost = new Map<number, number>();
/** Each share's state as last seen, for the status messages of a change. */
const lastStatus = new Map<number, OwnStatus>();
/** Shares a `switched` notice answered: their state going live then says nothing more. */
const switchedLately = new Set<number>();

const RECENT_MS = 5_000;

/** The room's refusals a live change can get (`share_set`: the gate, the month, the room's rule). */
const CHANGE_REFUSALS: ReadonlySet<ShareRefusal> = new Set([
  "budget",
  "off",
  "notAllowed",
  "failed",
]);

/** What a refused live change asked of the room: its given fields only. */
function askedOf(change: ShareAsked): OwnPatch {
  const asked: OwnPatch = {};
  if (change.preset != null) asked.preset = change.preset;
  if (change.fps != null) asked.fps = change.fps;
  if (change.audio != null) asked.audio = change.audio;
  return asked;
}

/** The room panel is open: the player's cards say their shares' states themselves. */
const panelOpen = () => useVoiceUi.getState().panel;

/** In share `n`'s place: its toast, or (the open panel's card showing it) a status message. */
function sayInPlace(n: number, text: string) {
  if (ownQuestion(n) !== undefined || !panelOpen()) ownToast(n, text);
  else sayShare(text);
}

/** A question toast still up goes (its share went on, or the open panel's card asks now). */
function takeQuestionBack(n: number) {
  if (droppedOwnQuestion(n)) toast.dismiss(ownToastId(n));
}

function ask(share: VoiceMyShare, why: LostWhy, until: number | null) {
  if (ownQuestion(share.n) === why) return;
  // Its minute ran out just now (the notice came before the state let the share go).
  const ended = endedLost.get(share.n);
  if (ended !== undefined && Date.now() - ended < RECENT_MS) return;
  askOwnQuestion(share.n, why, until, share.kind);
}

/** "Paylaşım durdu." where the question was (its minute ran out; the card went with it). */
function stoppedInPlace(n: number) {
  clearLost(n);
  endedLost.set(n, Date.now());
  ownToast(n, i18n.t("share.notice.stopped"));
}

/** The player's shares changed (or the panel opened or closed): their toasts and messages follow. */
export function watchOwnShares(mine: readonly VoiceMyShare[]) {
  const { lost } = useOwnUi.getState();
  const t = i18n.t.bind(i18n);
  for (const share of mine) {
    const status = ownStatus(share, lost[share.n]);
    const before = lastStatus.get(share.n);
    lastStatus.set(share.n, status);
    if (isLost(status)) {
      // The open panel's card asks (a toast over it would hide its buttons); else a toast.
      if (panelOpen()) takeQuestionBack(share.n);
      else ask(share, status, lostUntil(share, lost[share.n]));
      continue;
    }
    if (isLost(before)) {
      // A switch said "Artık … paylaşılıyor." already; else it went on by itself.
      if (switchedLately.has(share.n)) takeQuestionBack(share.n);
      else if (status === "live") sayInPlace(share.n, t("share.own.resumedSaid"));
      else takeQuestionBack(share.n);
      switchedLately.delete(share.n);
      continue;
    }
    switchedLately.delete(share.n);
    if (before === undefined || before === status) continue;
    if (status === "minimized") sayShare(t("share.own.minimizedSaid"));
    else if (status === "live" && before === "minimized") sayShare(t("share.own.resumedSaid"));
  }
  for (const n of [...lastStatus.keys()]) {
    if (mine.some((share) => share.n === n)) continue;
    const before = lastStatus.get(n);
    lastStatus.delete(n);
    switchedLately.delete(n);
    forgetOwnPreview(n);
    forgetOwnStats(n);
    if (!isLost(before) && ownQuestion(n) === undefined) continue;
    // Its minute ran out while the player stayed: said in a toast; left the room meanwhile:
    // nothing more to say.
    if (useVoiceUi.getState().state.room) stoppedInPlace(n);
    else takeQuestionBack(n);
  }
}

/** A share waiting for a new source as last seen (its toast or its card asking). */
const waitingShare = (n: number) => ownQuestion(n) !== undefined || isLost(lastStatus.get(n));

/** A `share-notice`: its toast, or the own share's question and its answers. */
export function showShareNotice(notice: ShareNotice) {
  const now = Date.now();
  for (const [n, at] of endedLost) if (now - at > RECENT_MS) endedLost.delete(n);
  switch (notice.kind) {
    case "refused": {
      // The room refused a live change (`share_update` asks it first; its notice can come before
      // the launcher's answer): that change's quality, frame rate or sound goes back, said by its
      // cause. The notice names the share and the change; one that does not takes back the latest
      // change the room decides on. Else a share or a watch could not start, as before.
      if (notice.change) {
        const asked = askedOf(notice.change);
        if (notice.n == null) dropRefused();
        else dropRefusedChange(notice.n, asked);
        toast(changeRefusedText(notice.refusal, asked));
        return;
      }
      const change = CHANGE_REFUSALS.has(notice.refusal) ? dropRefused() : null;
      toast(change ? changeRefusedText(notice.refusal, change) : refusalText(notice.refusal));
      return;
    }
    case "sourceClosed":
      // The state says it too; the question comes from whichever is first.
      markLost(notice.n, lostWhyOf(notice.src), null);
      return;
    case "switched":
      switchAnswered(notice.n, true);
      clearLost(notice.n);
      switchedLately.add(notice.n);
      sayInPlace(notice.n, switchedText(notice.src, notice.label));
      return;
    case "switchFailed": {
      const kind = switchAnswered(notice.n, false) ?? "window";
      sayOwnSwitchFailed(notice.n, switchFailedText(kind));
      return;
    }
    case "closed": {
      // The minute after a source closed ran out: said already, or said now in the question's place.
      const n = "n" in notice && typeof notice.n === "number" ? notice.n : null;
      if (notice.why === "sourceGone" || notice.why === "error") {
        if (endedLost.size > 0 && (n === null || endedLost.has(n))) return;
        const waiting = n ?? [...lastStatus.keys()].find(waitingShare) ?? null;
        if (waiting !== null && waitingShare(waiting)) {
          stoppedInPlace(waiting);
          return;
        }
      }
      const text = noticeText(notice);
      if (text) toast(text);
      return;
    }
    default: {
      const text = noticeText(notice);
      if (text) toast(text);
    }
  }
}

/** Screen sharing's toasts and the own shares' watch, for the launcher window's page. */
export function useShareNotices() {
  useEffect(() => shareApi.onNotice(showShareNotice), []);
  useEffect(() => {
    const look = () => watchOwnShares(useVoiceUi.getState().state.share?.mine ?? []);
    look();
    const stopVoice = useVoiceUi.subscribe((now, before) => {
      // The panel opening hands a question to the card; closing it asks in a toast again.
      if (now.state.share !== before.state.share || now.panel !== before.panel) look();
    });
    const stopOwn = useOwnUi.subscribe((now, before) => {
      if (now.lost !== before.lost) look();
    });
    return () => {
      stopVoice();
      stopOwn();
    };
  }, []);
}

/** Tests start from nothing. */
export function resetOwnNotices() {
  forgetOwnQuestions();
  endedLost.clear();
  lastStatus.clear();
  switchedLately.clear();
}
