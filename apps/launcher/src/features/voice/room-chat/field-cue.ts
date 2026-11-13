import { useEffect, useState } from "react";
import { useVoiceUi } from "../store";

/**
 * Grows when the room panel's chat field should take the focus: the panel was opened for its chat
 * (`openRoomPanel({ focus: "chat" })`) or changed its place while the field had it. Each ask is taken
 * once, by the field on screen; one that comes on screen later does not take an old ask.
 */
export function useFieldCue(visible: boolean): number {
  const asked = useVoiceUi((s) => s.chatFocus);
  const [cue, setCue] = useState(0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new ask, or the field coming on screen
  useEffect(() => {
    if (visible && useVoiceUi.getState().takeChatFocus()) setCue((n) => n + 1);
  }, [asked, visible]);
  return cue;
}
