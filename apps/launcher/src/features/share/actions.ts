// What the share buttons do (SHARE): each asks the launcher and says a refusal in one toast; what
// worked shows in the state (the panel's "Yayında", the stage). SQOL (sqol-r.md "API"): a live
// share switches, changes and pauses in place: the change shows at once and goes back if the
// launcher refuses, and a status message says what changed (the card and the menu show it too).
// Live changes stay with the share: the next share starts with the picker's choices.
import { toast } from "@pl/ui";
import { useSocialUi } from "@/features/social/store";
import { joinCrewRoom } from "@/features/voice/actions";
import { keptVoicePrefs, useVoicePrefs } from "@/features/voice/prefs";
import { faceNow, useVoiceUi } from "@/features/voice/store";
import { voiceApi } from "@/lib/api/voice-api";
import i18n from "@/lib/i18n";
import { shareApi } from "@/share/api";
import { grabShareFrame } from "@/share/frame";
import {
  DEFAULT_SHARE_SETTINGS,
  type ShareChange,
  type ShareClosedWhy,
  type ShareKind,
  type ShareNotice,
  type SharePreset,
  type ShareRefusal,
  type ShareSource,
  type VoiceShareSettings,
} from "@/share/types";
import { sayShare } from "./own-say";
import {
  applyPatch,
  clearLost,
  dropPatch,
  isLost,
  LOST_WAIT_S,
  type LostWhy,
  lostUntil,
  type OwnPatch,
  openSwitch,
  ownStatus,
  settlePatch,
  useOwnUi,
} from "./own-store";

/** A refusal in the player's words. */
export function refusalText(refusal: ShareRefusal, max = 2): string {
  const t = i18n.t.bind(i18n);
  switch (refusal) {
    case "off":
    case "budget":
      return t("share.notice.off");
    case "unsupported":
      return t("share.notice.unsupported");
    case "notAllowed":
      return t("share.notice.notAllowed");
    case "limited":
      return t("share.notice.limited");
    case "full":
      return t("share.notice.full", { max });
    case "roomFull":
      return t("share.notice.roomFull");
    case "watchFull":
      return t("share.notice.watchFull");
    case "watchMax":
      return t("share.notice.watchMax");
    case "gone":
      return t("share.notice.gone");
    case "inUse":
      return t("share.notice.inUse");
    default:
      return t("share.notice.failed");
  }
}

/** Why one of the player's shares ended, in their words (none when they left the room). */
export function closedText(why: ShareClosedWhy, src?: ShareKind): string | null {
  const t = i18n.t.bind(i18n);
  switch (why) {
    case "mod":
      return t("share.notice.mod");
    case "budget":
    case "limit":
      return t("share.notice.off");
    case "idle":
      return t("share.notice.idle");
    case "sourceGone":
      return src === "game" ? t("share.notice.gameGone") : t("share.notice.sourceGone");
    case "network":
      return t("share.notice.network");
    case "left":
      return null;
    case "error":
      return t("share.notice.stopped");
    default:
      return t("share.notice.stopped");
  }
}

/** "Bu pencere paylaşılamadı." / "Bu ekran …" / "Oyun …". */
export function switchFailedText(kind: ShareKind): string {
  return i18n.t(`share.own.switchFailed.${kind}`);
}

/** "Artık oyun paylaşılıyor." / 'Artık "Kale planı" paylaşılıyor.' / "Artık Ekran 2 paylaşılıyor." */
export function switchedText(kind: ShareKind, label: string): string {
  return i18n.t(`share.own.switched.${kind}`, { name: label });
}

/** "Paylaştığın pencere kapandı." / "Paylaştığın ekran kapandı." / "Oyun kapandı.". */
export function lostText(kind: ShareKind): string {
  return i18n.t(`share.own.lost.${kind}`);
}

/** Which kind of source went away for a hold's reason. */
export function lostWhyOf(kind: ShareKind): LostWhy {
  return kind === "game" ? "gameClosed" : "closed";
}

/** A `share-notice` as a toast. */
export function noticeText(notice: ShareNotice): string | null {
  const t = i18n.t.bind(i18n);
  switch (notice.kind) {
    case "closed":
      return closedText(notice.why, notice.src);
    case "refused":
      return refusalText(notice.refusal);
    case "audioOff":
      return t("share.notice.audioOff");
    case "watchStopped":
      return t("share.notice.watchStopped");
    case "watchFailed":
      return t("share.notice.watchFailed");
    case "switched":
      return switchedText(notice.src, notice.label);
    case "switchFailed":
      return switchFailedText("window");
    case "sourceClosed":
      return lostText(notice.src);
    default:
      return null;
  }
}

/** "Paylaş" in the picker. */
export async function startShare(source: ShareSource, label: string, max: number) {
  try {
    const refused = await shareApi.start(source, label);
    if (refused) toast(refusalText(refused, max));
  } catch {
    toast(i18n.t("share.notice.failed"));
  }
}

export async function stopShare(n: number) {
  clearLost(n);
  if (droppedOwnQuestion(n)) toast.dismiss(ownToastId(n));
  try {
    await shareApi.stop(n);
    sayShare(i18n.t("share.notice.stopped"));
  } catch {
    // The share ends with the room anyway.
  }
}

// --- SQOL: the player's own live share ------------------------------------------------------------

/** The one toast a share keeps (its source closed, then its answer): replaced in place. */
export function ownToastId(n: number): string {
  return `share-own-${n}`;
}

/** Sonner's own lifetime for a plain toast. */
const PLAIN_TOAST_MS = 4_000;

/** The shares whose toast is the question now (their source went away). */
const questions = new Map<number, LostWhy>();

/** What share `n`'s toast asks about now (nothing: it asks nothing, or there is none). */
export function ownQuestion(n: number): LostWhy | undefined {
  return questions.get(n);
}

/** Share `n`'s toast is the question now. */
export function askedOwnQuestion(n: number, why: LostWhy) {
  questions.set(n, why);
}

/** Share `n`'s question is answered (or let go of); true when there was one. */
export function droppedOwnQuestion(n: number): boolean {
  return questions.delete(n);
}

/** Tests start from nothing. */
export function forgetOwnQuestions() {
  questions.clear();
}

/**
 * One sentence in share `n`'s toast: a question there before (its source closed) loses its
 * buttons and its minute (Sonner merges an update into the toast it replaces).
 */
export function ownToast(n: number, text: string) {
  questions.delete(n);
  toast(text, {
    id: ownToastId(n),
    cancel: undefined,
    action: undefined,
    duration: PLAIN_TOAST_MS,
  });
}

/**
 * Share `n`'s question while its source is gone: "Paylaştığın pencere kapandı." (or `text`: a
 * switch that failed meanwhile) with "Başka Ekran Seç" and "Paylaşımı Durdur", as long as it waits.
 */
export function askOwnQuestion(
  n: number,
  why: LostWhy,
  until: number | null,
  kind: ShareKind,
  text?: string,
) {
  questions.set(n, why);
  const t = i18n.t.bind(i18n);
  const left = until === null ? LOST_WAIT_S * 1000 : until * 1000 - Date.now();
  toast(text ?? lostText(kind), {
    id: ownToastId(n),
    // As long as the share waits (a toast never says something no longer true).
    duration: Math.max(PLAIN_TOAST_MS, left),
    cancel: { label: t("share.own.pickOther"), onClick: () => openSwitch(n) },
    action: { label: t("share.own.menu.stop"), onClick: () => void stopShare(n) },
  });
}

/** Share `n` waits for a new source now (its window, screen or game closed): why, until when. */
function ownLostNow(n: number): { why: LostWhy; until: number | null; kind: ShareKind } | null {
  const mine = useVoiceUi.getState().state.share?.mine.find((m) => m.n === n);
  if (!mine) return null;
  const mark = useOwnUi.getState().lost[n];
  const status = ownStatus(mine, mark);
  return isLost(status) ? { why: status, until: lostUntil(mine, mark), kind: mine.kind } : null;
}

/** A switch that failed: said in the share's toast, still asking while its source is gone. */
export function sayOwnSwitchFailed(n: number, text: string) {
  const lost = ownLostNow(n);
  if (lost) askOwnQuestion(n, lost.why, lost.until, lost.kind, text);
  else ownToast(n, text);
}

const RESOLUTION: Record<SharePreset, string> = { low: "480p", normal: "720p", high: "1080p" };

/** "Normal · 720p" ("Yüksek · 1080p60" at 60 frames): a share's quality as the card says it. */
export function qualityText(preset: SharePreset, fps?: number): string {
  const t = i18n.t.bind(i18n);
  const rate = fps !== undefined && fps !== 30 ? String(Math.round(fps)) : "";
  return t("share.own.quality", {
    quality: t(`share.quality.${preset}`),
    res: `${RESOLUTION[preset]}${rate}`,
  });
}

/** A quality row's words ("Yüksek · 1080p"). */
export function presetText(preset: SharePreset): string {
  return qualityText(preset);
}

/** What changed, said once for a screen reader (the card and the menu show it already). */
export function changeSaid(change: ShareChange): string | null {
  const t = i18n.t.bind(i18n);
  if (change.preset !== undefined) {
    return t("share.own.said.quality", {
      quality: t(`share.quality.${change.preset}`),
      res: RESOLUTION[change.preset],
    });
  }
  if (change.fps !== undefined) return t("share.own.said.fps", { fps: change.fps });
  if (change.motion !== undefined) {
    const word =
      change.motion === "auto"
        ? t("share.advanced.motionAuto")
        : change.motion === "clarity"
          ? t("share.advanced.clarity")
          : t("share.advanced.smooth");
    return t("share.own.said.motion", { motion: word });
  }
  if (change.audio !== undefined) {
    return change.audio ? t("share.own.said.audioOn") : t("share.own.said.audioOff");
  }
  if (change.cursor !== undefined) {
    return change.cursor ? t("share.own.said.cursorOn") : t("share.own.said.cursorOff");
  }
  if (change.thumbs !== undefined) {
    return change.thumbs ? t("share.own.said.thumbsOn") : t("share.own.said.thumbsOff");
  }
  return null;
}

/** A refused change, in one sentence. */
export function changeRefusedText(
  refusal: ShareRefusal,
  change: Pick<ShareChange, "preset" | "fps">,
): string {
  const t = i18n.t.bind(i18n);
  if (refusal === "budget" && (change.preset !== undefined || change.fps !== undefined)) {
    return t("share.quality.capped");
  }
  return t("share.own.changeOff");
}

/**
 * A live share's quality, frame rate, picture, cursor, sound or small picture: shown at once,
 * back as it was if the launcher refuses (one toast says why); true when taken. The room may
 * allow less: the state then says what it took.
 */
export async function changeShare(n: number, change: ShareChange): Promise<boolean> {
  const patch: OwnPatch = { ...change };
  const pending = applyPatch(n, patch);
  try {
    const refused = await shareApi.update(n, change);
    if (refused) {
      dropPatch(pending);
      toast(changeRefusedText(refused, change));
      return false;
    }
    settlePatch(pending);
    const said = changeSaid(change);
    if (said) sayShare(said);
    return true;
  } catch {
    dropPatch(pending);
    toast(i18n.t("share.own.changeFailed"));
    return false;
  }
}

/** "Duraklat" / "Sürdür": the watchers see an empty frame meanwhile; true when done. */
export async function holdShare(n: number, on: boolean): Promise<boolean> {
  const t = i18n.t.bind(i18n);
  const pending = applyPatch(n, on ? { st: "hold", hold: "paused" } : { st: "live", hold: null });
  try {
    const refused = await shareApi.hold(n, on);
    if (refused) {
      dropPatch(pending);
      toast(on ? t("share.own.pauseFailed") : t("share.own.resumeFailed"));
      return false;
    }
    settlePatch(pending);
    sayShare(on ? t("share.own.pausedSaid") : t("share.own.resumedSaid"));
    return true;
  } catch {
    dropPatch(pending);
    toast(on ? t("share.own.pauseFailed") : t("share.own.resumeFailed"));
    return false;
  }
}

/** The switches on their way (the launcher answers with a notice): their change, the kind asked. */
const switches = new Map<number, { seq: number; kind: ShareKind }>();

/** How long a switch waits for its answer before the state alone shows. */
const SWITCH_WAIT_MS = 10_000;

/**
 * The launcher said how share `n`'s switch went ("switched" / "switchFailed"): the card keeps the
 * new source or shows the old one again. Returns the kind that was asked for (null: none was).
 */
export function switchAnswered(n: number, ok: boolean): ShareKind | null {
  const wait = switches.get(n);
  if (!wait) return null;
  switches.delete(n);
  if (ok) settlePatch(wait.seq);
  else dropPatch(wait.seq);
  return wait.kind;
}

/**
 * "Ekranı Değiştir": share `n` goes on with `source` (the watchers stay). The card shows it at
 * once; the launcher answers with a notice ("Artık … paylaşılıyor." or, the old source going on,
 * "Bu pencere paylaşılamadı."). True when the launcher took the request.
 */
export async function switchShare(n: number, source: ShareSource, label: string): Promise<boolean> {
  clearLost(n);
  const earlier = switches.get(n);
  if (earlier) dropPatch(earlier.seq);
  const pending = applyPatch(n, {
    kind: source.kind,
    label: source.kind === "game" ? "" : label,
    source,
    st: "live",
    hold: null,
    holdUntil: null,
  });
  switches.set(n, { seq: pending, kind: source.kind });
  const failed = (text: string) => {
    if (switches.get(n)?.seq === pending) switches.delete(n);
    dropPatch(pending);
    sayOwnSwitchFailed(n, text);
  };
  try {
    const refused = await shareApi.switchSource(n, source, label);
    if (refused) {
      failed(refused === "inUse" ? refusalText(refused) : switchFailedText(source.kind));
      return false;
    }
    setTimeout(() => {
      if (switches.get(n)?.seq === pending) switchAnswered(n, true);
    }, SWITCH_WAIT_MS);
    return true;
  } catch {
    failed(switchFailedText(source.kind));
    return false;
  }
}

/** "İzle". */
export async function watchShare(person: string, n: number) {
  try {
    const refused = await shareApi.watch(person, n);
    if (refused) toast(refusalText(refused));
  } catch {
    toast(i18n.t("share.notice.failed"));
  }
}

export async function unwatchShare(person: string, n: number) {
  try {
    await shareApi.unwatch(person, n);
  } catch {
    // Watching ends with the room anyway.
  }
}

/** How long "Katıl ve izle" waits in the room for the share to show before it lets go. */
export const JOIN_WATCH_MS = 15_000;

/** The "Katıl ve izle" under way (its room and person), so a second press starts nothing. */
let joinWatching: string | null = null;

/**
 * SHARE-LIVE: "Katıl ve izle" on a preview card: joins the crew room, then watches the person's
 * share once the room lists it (once; not if the player ends up elsewhere, nor after 15 s).
 */
export async function joinAndWatch(
  crew: string,
  room: { id: string; general: boolean },
  person: string,
): Promise<void> {
  const asked = `${room.id}|${person}`;
  if (joinWatching === asked) return;
  joinWatching = asked;
  const joined = await joinCrewRoom(crew, room.general ? null : room.id);
  if (!joined) {
    joinWatching = null;
    return;
  }
  let done = false;
  let stop: () => void = () => {};
  const timer = setTimeout(() => finish(), JOIN_WATCH_MS);
  function finish() {
    done = true;
    stop();
    clearTimeout(timer);
    if (joinWatching === asked) joinWatching = null;
  }
  const check = () => {
    if (done) return;
    const { room: now, share } = useVoiceUi.getState().state;
    if (now && now.id !== room.id) {
      finish();
      return;
    }
    if (now?.phase !== "live" || !share?.on) return;
    const next = share.live.find(
      (l) => l.person === person && !share.watching.some((w) => w.person === person && w.n === l.n),
    );
    if (!next) return;
    finish();
    void watchShare(person, next.n);
  };
  stop = useVoiceUi.subscribe(check);
  check();
}

/** The player's share choices as voice chat keeps them. */
export function shareSettings(): VoiceShareSettings {
  return { ...DEFAULT_SHARE_SETTINGS, ...useVoicePrefs.getState().settings?.share };
}

/** Saves a share choice (the picker's and the settings page's). */
export function setShareSetting<K extends keyof VoiceShareSettings>(
  key: K,
  value: VoiceShareSettings[K],
) {
  const settings = useVoicePrefs.getState().settings;
  if (!settings) return;
  const next = { ...settings, share: { ...shareSettings(), [key]: value } };
  // Shown at once; the launcher's kept copy follows.
  keptVoicePrefs(next);
  void voiceApi
    .saveSettings(next)
    .then(keptVoicePrefs)
    .catch(() => {
      // The next change tries again.
    });
}

/**
 * "Yayını rapor et": the report dialog about one share in the player's room, with what the player
 * sees of it now (taken before the dialog covers it; none when it cannot be taken).
 */
export async function reportShare(person: string, n: number) {
  const room = useVoiceUi.getState().state.room?.id;
  if (!room) return;
  const frame = await grabShareFrame(person, n);
  const face = faceNow(person);
  useSocialUi.getState().openReport({
    target: { kind: "player", id: person, voice: room, share: { n, ...(frame ? { frame } : {}) } },
    name: face.name,
    person: { name: face.name, avatar: face.avatar ?? "discord", avatarUrl: face.avatarUrl },
  });
}
