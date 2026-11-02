// The polite live region `announce` speaks through (use-accounts.ts), mounted once by the account
// host in the title bar. An announcement is a moment, not a state: its words leave the region once
// a screen reader has had them (`ANNOUNCE_CLEAR_MS`, as React Aria's announcer) and at once when the
// interface's language changes. Kept, a sentence stayed in the page in the language it was made in:
// after a switch to English the title bar still held "BuzKalesi seni oyuna çağırıyor" from an invite
// twelve minutes earlier (REL-BETA203 observation 4, BETA2-FIX), and the reading cursor found it.
// Saying it again in the new language is no answer either: a changed region is read out anew.
import { useEffect, useState } from "react";
import i18n from "@/lib/i18n";
import { onAnnounce } from "./use-accounts";

/** How long an announcement stays in the region: long enough for a screen reader to take it. */
export const ANNOUNCE_CLEAR_MS = 7_000;

export function LiveRegion() {
  const [text, setText] = useState("");
  useEffect(() => {
    let frame = 0;
    let timer: number | undefined;
    const stop = () => {
      cancelAnimationFrame(frame);
      window.clearTimeout(timer);
    };
    const clear = () => {
      stop();
      setText("");
    };
    const off = onAnnounce((next) => {
      // The same words again are read again: empty first, then the words on the next frame.
      clear();
      frame = requestAnimationFrame(() => {
        setText(next);
        timer = window.setTimeout(() => setText(""), ANNOUNCE_CLEAR_MS);
      });
    });
    i18n.on("languageChanged", clear);
    return () => {
      off();
      i18n.off("languageChanged", clear);
      stop();
    };
  }, []);
  return (
    <div aria-live="polite" aria-atomic="true" className="sr-only">
      {text}
    </div>
  );
}
