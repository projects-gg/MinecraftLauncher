// What voice chat's buttons do (VOICE-L3). Each asks the backend once; a refusal says why in one
// toast with voice.md §7.10's words (neutral: being refused is no failure), a failure says
// "Bağlanamadı." in flare. Nothing retries by itself.
import { toast } from "@pl/ui";
import { announce } from "@/features/accounts/use-accounts";
import { voiceApi } from "@/lib/api/voice-api";
import type { VoiceAnswer, VoiceModAction } from "@/lib/api/voice-types";
import i18n from "@/lib/i18n";
import type { ShareBy } from "@/share/types";
import { useCrewRoomsUi } from "./crew-rooms-store";
import { refusalText } from "./rules";
import { faceNow, useVoiceUi } from "./store";

/** How long the toast after "Odadan çıkar" offers "Geri al" (voice.md §4). */
export const KICK_UNDO_MS = 5000;

function failed() {
  toast.error(i18n.t("voice.refused.failed"));
}

/** An action that answers: true when it went through; a refusal or a failure is said. */
async function ask(work: () => Promise<VoiceAnswer>, name?: string): Promise<boolean> {
  try {
    const answer = await work();
    const text = refusalText(answer, i18n.t, name);
    if (text) {
      if (answer.refused === "failed") toast.error(text);
      else toast(text);
    }
    return answer.refused === null;
  } catch {
    failed();
    return false;
  }
}

/** An action that answers nothing. */
async function run(work: () => Promise<void>): Promise<boolean> {
  try {
    await work();
    return true;
  } catch {
    failed();
    return false;
  }
}

const room = () => useVoiceUi.getState().state.room;

/** "Ara" (and "Tekrar ara", "Geri ara"). */
export function callFriend(friend: string): Promise<boolean> {
  useVoiceUi.getState().setEnded(null);
  return ask(() => voiceApi.call(friend), faceNow(friend).name);
}

/** "Katıl" on a ring or on a missed call that can still be joined. */
export function answerRing(roomId: string): Promise<boolean> {
  return ask(() => voiceApi.answer(roomId));
}

/** "Reddet". */
export function declineRing(roomId: string): Promise<boolean> {
  return ask(() => voiceApi.decline(roomId));
}

export function dismissMissed(roomId: string): Promise<boolean> {
  return run(() => voiceApi.dismissMissed(roomId));
}

/** A crew room's "Katıl" ("Genel" without a room). */
export function joinCrewRoom(crew: string, roomId: string | null): Promise<boolean> {
  return ask(() => voiceApi.joinCrew(crew, roomId));
}

/** "Yeni oda". */
export function openCrewRoom(crew: string): Promise<boolean> {
  return ask(() => voiceApi.newCrewRoom(crew));
}

/** "Ayrıl", and "İptal" while the player's call rings (the room ends with them). */
export function leaveRoom(): Promise<boolean> {
  return run(() => voiceApi.leave());
}

/** "Tekrar dene" after "Bağlanamadı". */
export function retryRoom(): Promise<boolean> {
  useVoiceUi.getState().setEnded(null);
  return ask(() => voiceApi.retry());
}

/**
 * "Mikrofonu kapat" / "Mikrofonu aç". Opening the microphone while deafened brings the sound back
 * too (Discord's rule; the launcher does it, VOICE-L1): one command either way.
 */
export async function toggleMute(): Promise<void> {
  const current = room();
  if (!current) return;
  const { mute, deaf } = current.me;
  await run(() => voiceApi.setMute(!(mute || deaf)));
}

/** "Sağırlaştır" / "Sağırlaştırmayı kaldır". */
export async function toggleDeaf(): Promise<void> {
  const current = room();
  if (current) await run(() => voiceApi.setDeaf(!current.me.deaf));
}

/** "Davet et" / "Odaya davet et". */
export function inviteToRoom(friend: string): Promise<boolean> {
  return ask(() => voiceApi.invite(friend), faceNow(friend).name);
}

/** "İptal" beside an invite that rings. */
export function cancelInvite(friend: string): Promise<boolean> {
  return ask(() => voiceApi.cancelInvite(friend));
}

/** The volume a drag sets is sent at most this often; the last one always goes. */
const VOLUME_EVERY_MS = 60;
const volumeTimers = new Map<string, ReturnType<typeof setTimeout>>();
const volumeNext = new Map<string, number>();

/** A person's volume (percent, 0–200), as the menu's slider moves. */
export function setVolume(person: string, volume: number, final = false): void {
  volumeNext.set(person, volume);
  const send = () => {
    volumeTimers.delete(person);
    const value = volumeNext.get(person);
    volumeNext.delete(person);
    if (value !== undefined) void run(() => voiceApi.setVolume(person, value));
  };
  if (final) {
    clearTimeout(volumeTimers.get(person));
    send();
    return;
  }
  if (!volumeTimers.has(person)) volumeTimers.set(person, setTimeout(send, VOLUME_EVERY_MS));
}

/** "Sustur (yalnız sen)". */
export function setMutedForMe(person: string, muted: boolean): Promise<boolean> {
  return run(() => voiceApi.setMutedForMe(person, muted));
}

/**
 * A leader's or moderator's action. "Odadan çıkar" asks nothing first: its toast offers "Geri al"
 * for 5 seconds (voice.md §4), which lets them back and in again by invite.
 */
export async function moderate(action: VoiceModAction): Promise<boolean> {
  const name = "person" in action ? faceNow(action.person).name : undefined;
  const done = await ask(() => voiceApi.moderate(action), name);
  if (!done || !name || !("person" in action)) return done;
  if (action.act === "kick") {
    const text = i18n.t("voice.person.kicked", { name });
    announce(text);
    toast(text, {
      duration: KICK_UNDO_MS,
      action: {
        label: i18n.t("voice.person.undo"),
        onClick: () => void ask(() => voiceApi.moderate({ act: "unkick", person: action.person })),
      },
    });
  } else if (action.act === "move") {
    announce(i18n.t("voice.person.moved", { name }));
  }
  return done;
}

/** The room's new name: null when it was taken, else the words for the field. */
export async function renameRoom(name: string): Promise<string | null> {
  try {
    const answer = await voiceApi.moderate({ act: "rename", name: name.trim() });
    return refusalText(answer, i18n.t);
  } catch {
    return i18n.t("voice.refused.failed");
  }
}

/** "Odayı kapat" (after its one sentence). */
export function closeRoom(): Promise<boolean> {
  return ask(() => voiceApi.moderate({ act: "close" }));
}

/** "Ayarları aç" under "Mikrofona izin verilmedi.". */
export function openMicSettings(): Promise<boolean> {
  return run(() => voiceApi.openMicSettings());
}

// --- VOICE-ROOMS: the crew page's room changes -------------------------------------------------

/** How long a deleted empty room waits for "Geri al" before the delete goes. */
export const ROOM_UNDO_MS = 5000;

/** "Odayı düzenle" ("Kaydet"): null when it was saved, else the words for the dialog. */
export async function editCrewRoom(
  crew: string,
  room: string,
  name: string,
  max: number | null,
  shareBy?: ShareBy,
): Promise<string | null> {
  try {
    const answer = await voiceApi.crewChange({
      op: "edit",
      crew,
      room,
      name: name.trim(),
      max,
      ...(shareBy ? { shareBy } : {}),
    });
    if (answer.refused === null) return null;
    return refusalText(answer, i18n.t) ?? i18n.t("voice.refused.failed");
  } catch {
    return i18n.t("voice.refused.failed");
  }
}

/**
 * The crew's rooms ("Genel" aside) in a new order (a drag, Alt+↑/↓, "Yukarı taşı"): shown at once,
 * and back as they were when the service refuses.
 */
export async function orderCrewRooms(crew: string, rooms: readonly string[]): Promise<boolean> {
  const ui = useCrewRoomsUi.getState();
  ui.setOrder(crew, rooms);
  const done = await ask(() => voiceApi.crewChange({ op: "order", crew, rooms: [...rooms] }));
  if (done) announce(i18n.t("voice.rooms.ordered"));
  // Done: the service's list follows (`voice_rooms`); refused: the saved order comes back.
  if (!done) useCrewRoomsUi.getState().setOrder(crew, null);
  return done;
}

/**
 * "Odayı sil" of a room with people in it (after its one sentence): they go to "Genel". Done, the
 * room leaves the list at once (the service's list follows).
 */
export async function removeCrewRoom(crew: string, room: string): Promise<boolean> {
  const done = await ask(() => voiceApi.crewChange({ op: "remove", crew, room, empty: false }));
  if (done) useCrewRoomsUi.getState().hide(room);
  return done;
}

/**
 * "Odayı sil" of an empty room: it leaves the list at once and the toast offers "Geri al"; the
 * delete goes only when the toast ends (taken back, nothing was sent). Someone who came in
 * meanwhile keeps it: it comes back, with one line.
 */
export function removeEmptyCrewRoom(crew: string, room: string, shownName: string): void {
  const ui = useCrewRoomsUi.getState();
  if (ui.hidden.includes(room)) return;
  ui.hide(room);
  let settled = false;
  const commit = async () => {
    if (settled) return;
    settled = true;
    try {
      const answer = await voiceApi.crewChange({ op: "remove", crew, room, empty: true });
      if (answer.refused === null) return;
      useCrewRoomsUi.getState().show(room);
      // "nothing": someone is in it now (the service's `room_not_empty`).
      toast(
        answer.refused === "nothing"
          ? i18n.t("voice.rooms.notRemoved")
          : (refusalText(answer, i18n.t) ?? i18n.t("voice.refused.failed")),
      );
    } catch {
      useCrewRoomsUi.getState().show(room);
      failed();
    }
  };
  const text = i18n.t("voice.rooms.removed", { room: shownName });
  announce(text);
  toast(text, {
    duration: ROOM_UNDO_MS,
    action: {
      label: i18n.t("voice.rooms.undo"),
      onClick: () => {
        if (settled) return;
        settled = true;
        useCrewRoomsUi.getState().show(room);
      },
    },
    onAutoClose: () => void commit(),
    onDismiss: () => void commit(),
  });
}

/** A leader or moderator drags a crewmate to another room of the crew. */
export async function moveCrewmate(crew: string, person: string, room: string): Promise<boolean> {
  const name = faceNow(person).name;
  const done = await ask(() => voiceApi.crewChange({ op: "move", crew, person, room }), name);
  if (done) announce(i18n.t("voice.rooms.moved", { name }));
  return done;
}
