// What screen readers hear of the room's streams (SQOL-U1; WCAG 4.1.3, sqol.md §0): someone in the
// room went live or ended, a watched stream opened, paused, had its window minimized or came back.
// Polite, one short line each, the name last (a player's name never takes a suffix). A room's
// streams that were there when the player came are not news.
import { useEffect } from "react";
import { faceNow, useVoiceUi } from "@/features/voice/store";
import i18n from "@/lib/i18n";
import { shareKey, type VoiceShareView } from "@/share/types";

export type StreamSayingKind = "live" | "open" | "hold" | "min" | "resume" | "end";

export interface StreamSaying {
  kind: StreamSayingKind;
  person: string;
  n: number;
}

/**
 * What changed between two views that is worth saying. `me`: the player (their own shares are
 * theirs to know); `here`: the window that watches (states only for the streams it shows).
 */
export function streamSayings(
  before: VoiceShareView | null,
  after: VoiceShareView | null,
  me: string | null,
  here: boolean,
): StreamSaying[] {
  if (!before?.on || !after?.on) return [];
  const out: StreamSaying[] = [];
  const was = new Map(before.live.map((live) => [shareKey(live.person, live.n), live]));
  const is = new Map(after.live.map((live) => [shareKey(live.person, live.n), live]));
  const watchedBefore = new Map(before.watching.map((w) => [shareKey(w.person, w.n), w]));
  const watchedNow = new Map(after.watching.map((w) => [shareKey(w.person, w.n), w]));
  for (const [key, live] of is) {
    if (live.person === me) continue;
    const old = was.get(key);
    if (!old) {
      out.push({ kind: "live", person: live.person, n: live.n });
      continue;
    }
    if (!here || !watchedNow.has(key)) continue;
    const from = old.st ?? "live";
    const to = live.st ?? "live";
    if (from === to) continue;
    out.push({
      kind: to === "hold" ? "hold" : to === "min" ? "min" : "resume",
      person: live.person,
      n: live.n,
    });
  }
  for (const [key, live] of was) {
    if (live.person !== me && !is.has(key)) {
      out.push({ kind: "end", person: live.person, n: live.n });
    }
  }
  if (here) {
    for (const [key, watch] of watchedNow) {
      const old = watchedBefore.get(key);
      if (watch.phase === "live" && old?.phase === "connecting") {
        out.push({ kind: "open", person: watch.person, n: watch.n });
      }
    }
  }
  return out;
}

/** A saying in the player's words (the "Yayın" window names people as the launcher told it). */
export function sayingText(saying: StreamSaying, names: Record<string, string> = {}): string {
  const name = names[saying.person] ?? faceNow(saying.person).name;
  return i18n.t(`shareView.say.${saying.kind}`, { name });
}

/** Where this window says things: the launcher's polite live region, or the "Yayın" window's own. */
let sayer: (text: string) => void = () => {};

/** This window's polite live region. Returns its release. */
export function setStreamSayer(say: (text: string) => void): () => void {
  sayer = say;
  return () => {
    if (sayer === say) sayer = () => {};
  };
}

/** Says one short line about the streams (a key's volume change too). */
export function sayStream(text: string): void {
  sayer(text);
}

/** Says what changes while this window lives (`surface`: which window this is). */
export function useStreamSayings(surface: "main" | "popout") {
  useEffect(
    () =>
      useVoiceUi.subscribe((now, before) => {
        if (now.state === before.state) return;
        const me = now.state.room?.me.id ?? null;
        const after = now.state.share ?? null;
        const here = after?.surface === surface;
        const names = surface === "popout" ? (after?.names ?? {}) : {};
        for (const saying of streamSayings(before.state.share ?? null, after, me, here)) {
          sayStream(sayingText(saying, names));
        }
      }),
    [surface],
  );
}
