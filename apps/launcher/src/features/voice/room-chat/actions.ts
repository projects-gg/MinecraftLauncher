// What the player does in the room chat: write (shown at once, settled by the room's answer: the
// room's refusal stays under the message in chat's words, with "Düzenle" or "Tekrar Dene"), take
// their message away (its tombstone at once; it comes back if the room refuses), and report one
// (chat's report dialog, sent the room's way: the room proves what was said).
import { toast } from "@pl/ui";
import { announce, friendlyError } from "@/features/accounts/use-accounts";
import { messageLine } from "@/features/social/chat/preview";
import type { ChatPerson } from "@/features/social/chat/sender-name";
import { useSocialUi } from "@/features/social/store";
import { api } from "@/lib/api";
import i18n from "@/lib/i18n";
import { useVoiceUi } from "../store";
import { roomChatApi } from "./api";
import type { ShownMsg } from "./rules";
import {
  discard,
  markDeleted,
  retrySend,
  settleSend,
  startSend,
  undelete,
  useRoomChat,
} from "./store";
import type { RoomChatSend } from "./types";

let made = 0;
const nonceOf = () => `${Date.now().toString(36)}${(made++).toString(36)}`;

async function ask(room: string, body: { text?: string; sticker?: string }): Promise<RoomChatSend> {
  try {
    return await roomChatApi.send(room, body);
  } catch {
    return { refused: { code: "failed" } };
  }
}

async function send(body: { text: string; sticker?: string }): Promise<void> {
  const room = useRoomChat.getState().room;
  // The player as the room knows them (the room's messages carry this id).
  const me = useVoiceUi.getState().state.room?.me.id;
  if (!room || !me) return;
  const nonce = nonceOf();
  useRoomChat.getState().apply((d) => startSend(d, nonce, body, me, Date.now()));
  const result = await ask(room, body.sticker ? { sticker: body.sticker } : { text: body.text });
  useRoomChat.getState().apply((d) => (d.room === room ? settleSend(d, nonce, result) : d));
}

/** Words to the player's room (checked by the composer first, as chat's are). */
export function sendRoomText(text: string): Promise<void> {
  return send({ text });
}

/** A "Küp" sticker to the player's room. */
export function sendRoomSticker(ref: string): Promise<void> {
  return send({ text: "", sticker: ref });
}

/** "Tekrar Dene" under a message the room did not take. */
export async function retryRoomMessage(key: string): Promise<void> {
  const { room, msgs } = useRoomChat.getState();
  const msg = msgs.find((m) => m.key === key);
  if (!room || !msg || !key.startsWith("n:")) return;
  useRoomChat.getState().apply((d) => retrySend(d, key));
  const result = await ask(room, msg.sticker ? { sticker: msg.sticker } : { text: msg.text });
  useRoomChat.getState().apply((d) => (d.room === room ? settleSend(d, key.slice(2), result) : d));
}

/** "Sil" under a message the room did not take; "Düzenle" takes its words back to the field. */
export function dropRoomMessage(key: string, toField: boolean): void {
  const msg = useRoomChat.getState().msgs.find((m) => m.key === key);
  useRoomChat.getState().apply((d) => discard(d, key));
  if (toField && msg && !msg.sticker) useRoomChat.getState().setDraft(msg.text);
}

/** "Herkesten Sil": the tombstone at once; the message comes back if the room refuses. */
export async function deleteRoomMessage(id: string): Promise<boolean> {
  const room = useRoomChat.getState().room;
  if (!room) return false;
  useRoomChat.getState().apply((d) => markDeleted(d, id, "me"));
  try {
    await roomChatApi.remove(room, id);
    announce(i18n.t("chat.actions.deleted"));
    return true;
  } catch (error) {
    useRoomChat.getState().apply((d) => (d.room === room ? undelete(d, id) : d));
    toast.error(friendlyError(error, i18n.t("chat.failure.connection")).message);
    return false;
  }
}

/** "Rapor Et": chat's dialog; it goes the room's way (the room seals what was said). */
export function reportRoomMessage(msg: ShownMsg, sender: ChatPerson): void {
  const room = useRoomChat.getState().room;
  const id = msg.id;
  if (!room || id === undefined) return;
  useSocialUi.getState().openReport({
    target: { kind: "message", conv: `v:${room}`, id: msg.seq },
    name: sender.name,
    person: sender,
    text: messageLine(msg, i18n.t.bind(i18n), i18n.language),
    send: async (reason, note, block) => {
      await roomChatApi.report(room, id, reason, note);
      if (block) await api.social.block(msg.from);
    },
  });
}
