// `roomChatApi` (SQOL §3): the app's commands, or the browser preview's mock (mock.ts). Kept with
// the feature, as screen sharing's and voice chat's seams are.
import { isTauri } from "@/lib/platform";
import { mockRoomChat } from "./mock";
import { tauriRoomChat } from "./tauri";
import type { RoomChatApi } from "./types";

export const roomChatApi: RoomChatApi = isTauri() ? tauriRoomChat : mockRoomChat;
