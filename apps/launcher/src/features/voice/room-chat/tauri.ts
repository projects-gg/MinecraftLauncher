// The app's side of room chat (SQOL §3, docs/tasks/sqol-r.md "Oda paneli ve sohbet": R's
// `voice_chat_*`, `voice_typing` and the `voice-chat` event). Called by name, as the share module's
// commands are, so the page does not wait for the generated bindings; answers are read field by
// field (types.ts). A refusal is an answer: a deletion's or a report's turns into a
// `RoomChatRefused` here, which its callers word ("Bu mesajı silemezsin.", "Rapor gönderilemedi.").
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@/lib/api/listen";
import {
  codeOf,
  eventOf,
  msgOf,
  type RoomChatApi,
  type RoomChatMsg,
  RoomChatRefused,
  sendOf,
} from "./types";

const quiet = () => {
  // A launcher without the command (an older one) or one closing: nothing to do here.
};

/** A command that answers a refusal (`RoomChatError`) or null. */
async function done(command: string, args: Record<string, unknown>): Promise<void> {
  const refused = await invoke<unknown>(command, args);
  if (refused !== null && refused !== undefined) throw new RoomChatRefused(codeOf(refused));
}

export const tauriRoomChat: RoomChatApi = {
  log: async (room) => {
    const list = await invoke<unknown>("voice_chat_log", { room });
    return Array.isArray(list) ? list.map(msgOf).filter((m): m is RoomChatMsg => m !== null) : [];
  },
  send: async (room, body) =>
    sendOf(
      await invoke<unknown>("voice_chat_send", {
        room,
        ...(body.text ? { text: body.text } : {}),
        ...(body.sticker ? { sticker: body.sticker } : {}),
      }),
    ),
  remove: (room, id) => done("voice_chat_delete", { room, id }),
  report: (room, id, reason, note) =>
    done("voice_chat_report", { room, id, reason, ...(note ? { note } : {}) }),
  typing: (on) => invoke<null>("voice_typing", { on }).then(quiet, quiet),
  onEvent: (listener) => {
    let stop: (() => void) | null = null;
    let gone = false;
    void listen<unknown>("voice-chat", (event) => {
      const parsed = eventOf(event.payload);
      if (parsed) listener(parsed);
    }).then((unlisten) => {
      if (gone) unlisten();
      else stop = unlisten;
    });
    return () => {
      gone = true;
      stop?.();
    };
  },
};
