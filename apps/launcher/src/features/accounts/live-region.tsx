// The polite live region `announce` speaks through (use-accounts.ts), mounted once by the account
// host in the title bar. An announcement is a moment, not a state: its words leave the region once
// a screen reader has had them (`ANNOUNCE_CLEAR_MS`, as React Aria's announcer) and at once when the
// interface's language changes. Kept, a sentence stayed in the page in the language it was made in:
// after a switch to English the title bar still held "BuzKalesi seni oyuna çağırıyor" from an invite
// twelve minutes earlier (REL-BETA203 observation 4, BETA2-FIX), and the reading cursor found it.
// Saying it again in the new language is no answer either: a changed region is read out anew.
//
// SQOL-U3: a burst is never lost. Each announcement is a line of its own, added on the next frame
// (all of one frame's together) and taken away on its own time; the region reads additions only, so
// a screen reader reads every new line in order and nothing twice. Before, every call emptied the
// region: of "KorAtesi odaya katıldı." and "x7Ender odaya katıldı." only the last was heard. The same
// words again are a new line, so they are read again.
import { useEffect, useState } from "react";
import i18n from "@/lib/i18n";
import { onAnnounce } from "./use-accounts";

/** How long an announcement stays in the region: long enough for a screen reader to take it. */
export const ANNOUNCE_CLEAR_MS = 7_000;
/** At most this many lines at once: with more, the oldest go (a reader has had them by then). */
export const ANNOUNCE_MAX = 6;

interface Line {
  id: number;
  text: string;
}

export function LiveRegion() {
  const [lines, setLines] = useState<Line[]>([]);
  useEffect(() => {
    let last = 0;
    let frame = 0;
    let waiting: string[] = [];
    const timers = new Set<number>();
    const flush = () => {
      frame = 0;
      const batch = waiting.map((text) => ({ id: ++last, text }));
      waiting = [];
      if (!batch.length) return;
      setLines((shown) => [...shown, ...batch].slice(-ANNOUNCE_MAX));
      for (const line of batch) {
        const timer = window.setTimeout(() => {
          timers.delete(timer);
          setLines((shown) => shown.filter((l) => l.id !== line.id));
        }, ANNOUNCE_CLEAR_MS);
        timers.add(timer);
      }
    };
    const clear = () => {
      cancelAnimationFrame(frame);
      frame = 0;
      waiting = [];
      for (const timer of timers) window.clearTimeout(timer);
      timers.clear();
      setLines([]);
    };
    const off = onAnnounce((text) => {
      if (!text) return;
      waiting.push(text);
      if (!frame) frame = requestAnimationFrame(flush);
    });
    i18n.on("languageChanged", clear);
    return () => {
      off();
      i18n.off("languageChanged", clear);
      cancelAnimationFrame(frame);
      for (const timer of timers) window.clearTimeout(timer);
    };
  }, []);
  return (
    <div aria-live="polite" aria-relevant="additions" className="sr-only">
      {lines.map((line) => (
        <p key={line.id}>{line.text}</p>
      ))}
    </div>
  );
}
