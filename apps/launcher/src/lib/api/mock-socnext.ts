// SOC-NEXT's previews on top of mock-chat's data (`&socNext=demo` with a chat preview, e.g.
// `?social=chat2&socNext=demo`, with `&chatDays=1` for more conversations): KorAtesi under the
// nickname "Kor", TasDevi (a crewmate: their two crew messages fold) and KizilEjder (a friend: their
// conversation fades) ignored, NetherKral's conversation pinned to the top, ElmasKazma's marked
// unread, and DemirGolem_ mentioning the player in the crew's newest message. `&streamer=on` turns
// "Yayıncı modu" on; `&streamer=live` acts as if OBS were running (its default mode).
import { useSocNextPrefs } from "@/features/social/next/prefs";
import type { ChatMessage } from "./social-types";

/** The preview's ids (mock-social.ts, mock-chat.ts). */
const ME = "100";
const KOR = "302";
const NETHER = "305";
const ELMAS = "303";
const TAS = "703";
const KIZIL = "306";
const DEMIR = "702";
const direct = (peer: string) => `d:${ME}:${peer}`;

export function socNextDemo(param: (key: string) => string | null): boolean {
  return param("socNext") === "demo";
}

/** The player's SOC-NEXT choices of the preview (this computer's, as if set before). */
export function seedSocNextPrefs(param: (key: string) => string | null) {
  const streamer = param("streamer");
  if (!socNextDemo(param) && !streamer) return;
  const later = Date.now() + 86_400_000;
  useSocNextPrefs.setState({
    ...(socNextDemo(param)
      ? {
          nicknames: { [KOR]: "Kor" },
          ignored: [TAS, KIZIL],
          pinnedChats: [direct(NETHER)],
          unreadMarks: { [direct(ELMAS)]: later },
        }
      : {}),
    // Every preview says its own mode and timer (the choices stay in this browser between them).
    streamer: streamer === "on" ? ("on" as const) : "auto",
    statusTimer: null,
  });
}

/** The crew's newest messages in the preview: TasDevi twice (folded) and the mention. */
export function socNextCrewLines(
  t0: number,
  message: (from: string, text: string, at: number) => ChatMessage,
  pick: (tr: string, en: string) => string,
): ChatMessage[] {
  return [
    message(TAS, pick("Ben de geliyorum", "I'm coming too"), t0 - 5 * 60_000),
    message(TAS, pick("Bekleyin beni", "Wait for me"), t0 - 4 * 60_000 - 20_000),
    message(
      DEMIR,
      pick("@EjderAvcisi sen de gel, takımı kuralım", "@EjderAvcisi you come too, let's team up"),
      t0 - 60_000,
    ),
  ];
}
