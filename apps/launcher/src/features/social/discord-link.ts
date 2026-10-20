// "Discord'dan mesaj at" (friend card, teammate card, right-click menus): the person's profile in
// the Discord app when it is installed, where Message is one click away; otherwise their profile
// on discord.com in the browser. The backend decides whether the app is there and only ever hands
// it `discord://-/users/<id>`; the web profile goes through the `https://`-only opener.
import { api } from "@/lib/api";
import { openWebLink } from "@/lib/links";

/** A second press on the same person while the first one opens (a double click) is ignored. */
export const REPEAT_MS = 1500;

let last: { id: string; at: number } | null = null;

export async function openInDiscord(person: { id: string; discordUrl: string }): Promise<void> {
  const now = Date.now();
  if (last && last.id === person.id && now - last.at < REPEAT_MS) return;
  last = { id: person.id, at: now };
  const inApp = await api.social.openInDiscord(person.id).catch(() => false);
  if (!inApp) openWebLink(person.discordUrl);
}
