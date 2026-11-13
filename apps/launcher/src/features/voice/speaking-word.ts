// "konuşuyor" in a person's name for screen readers (SQOL §1.3 "Erişilebilirlik"): the room's rows
// and the call's faces say who speaks. The word comes after a moment of speech and goes after a
// moment of quiet, so a focused row's name does not change with every breath (who speaks comes
// up to ten times a second).
import { useEffect, useState } from "react";
import { useSpeaking } from "./store";

/** Speech this long before the word comes. */
export const SAY_SPEAKING_AFTER_MS = 300;
/** Quiet this long before it goes. */
export const SAY_QUIET_AFTER_MS = 1_200;

export function useSpeakingWord(id: string): boolean {
  const speaking = useSpeaking(id);
  const [said, setSaid] = useState(speaking);
  useEffect(() => {
    if (speaking === said) return;
    const timer = window.setTimeout(
      () => setSaid(speaking),
      speaking ? SAY_SPEAKING_AFTER_MS : SAY_QUIET_AFTER_MS,
    );
    return () => window.clearTimeout(timer);
  }, [speaking, said]);
  return said;
}
