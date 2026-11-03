// SOC-NEXT §3.1 in the composer: while the player writes "@" in crew chat (or ":" and two letters
// anywhere) a small list opens over the field (mentions.ts decides it): the crew's members, those
// online first, or emoji (the picker's data, the player's skin tone). ↑ and ↓ move, Enter or Tab
// puts the choice in, Esc closes it (the panel stays), the pointer picks too; the field keeps the
// focus. It opens and moves without motion: it follows the keys as they are typed.
import { cn } from "@pl/ui";
import { type KeyboardEvent, type RefObject, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { displayName, type EmojiData, type EmojiEntry, loadEmojiData } from "@/features/emoji/data";
import { searchEmoji } from "@/features/emoji/search";
import { useEmojiPrefs } from "@/features/emoji/store";
import { withTone } from "@/features/emoji/tone";
import { inside, leaveOnEscape } from "../edit-escape";
import { SocialFace } from "../social-face";
import { useSocialUi } from "../store";
import { useSocNext } from "./flag";
import {
  type MemberChoice,
  memberChoices,
  putSuggestion,
  SUGGEST_ROWS,
  type SuggestToken,
  suggestToken,
} from "./mentions";

type Choice =
  | { kind: "member"; member: MemberChoice & { avatar?: string; avatarUrl?: string } }
  | { kind: "emoji"; entry: EmojiEntry; emoji: string };

export interface Suggestions {
  token: SuggestToken | null;
  choices: Choice[];
  active: number;
  /** Handles the keys the list takes; true when it took this one. */
  onKey: (event: KeyboardEvent<HTMLTextAreaElement>) => boolean;
  pick: (index: number) => void;
  listId: string;
}

/** The crew's members as the "@" box offers them. */
function useMembers(
  crew: string | null,
): Array<MemberChoice & { avatar?: string; avatarUrl?: string }> {
  const team = useSocialUi((s) => (crew ? s.view.crews.find((c) => c.id === crew) : undefined));
  return useMemo(
    () =>
      (team?.members ?? []).map((member) => ({
        id: member.id,
        name: member.name,
        realName: member.realName,
        online: member.state !== "offline",
        avatar: member.avatar,
        avatarUrl: member.avatarUrl,
      })),
    [team],
  );
}

/**
 * What the composer suggests at the caret. `put` writes the new text and puts the caret after the
 * choice; `crew` is the crew of a crew chat (members are offered only there).
 */
export function useSuggestions({
  text,
  caret,
  crew,
  focused,
  put,
  field,
}: {
  text: string;
  caret: number;
  crew: string | null;
  focused: boolean;
  put: (value: string, caret: number) => void;
  field: RefObject<HTMLTextAreaElement | null>;
}): Suggestions {
  const next = useSocNext();
  const { i18n } = useTranslation();
  const lang = i18n.language.startsWith("en") ? "en" : "tr";
  const me = useSocialUi((s) => s.view.me?.id);
  const members = useMembers(crew);
  const memberNames = useMemo(
    () => members.flatMap((m) => (m.realName ? [m.name, m.realName] : [m.name])),
    [members],
  );
  // A selection (caret -1) suggests nothing.
  const raw = next && focused && caret >= 0 ? suggestToken(text, caret, memberNames) : null;
  const token = raw?.kind === "member" && !crew ? null : raw;
  // Esc closes the list for the token written: it opens again for the next one.
  const [closedAt, setClosedAt] = useState<number | null>(null);
  const shownToken = token && token.start !== closedAt ? token : null;
  useEffect(() => {
    if (!token) setClosedAt(null);
  }, [token]);

  // The emoji data loads on the first ":" token (as the picker does on its first use).
  const [data, setData] = useState<EmojiData | null>(null);
  const wantsEmoji = shownToken?.kind === "emoji";
  useEffect(() => {
    if (!wantsEmoji || data?.lang === lang) return;
    let alive = true;
    void loadEmojiData(lang).then(
      (loaded) => {
        if (alive) setData(loaded);
      },
      () => {},
    );
    return () => {
      alive = false;
    };
  }, [wantsEmoji, data, lang]);
  const tone = useEmojiPrefs((s) => s.tone);

  const choices = useMemo<Choice[]>(() => {
    if (!shownToken) return [];
    if (shownToken.kind === "member") {
      return memberChoices(members, shownToken.query, me).map((member) => ({
        kind: "member" as const,
        member,
      }));
    }
    if (!data) return [];
    return searchEmoji(data, shownToken.query)
      .slice(0, SUGGEST_ROWS)
      .map((entry) => ({
        kind: "emoji" as const,
        entry,
        emoji: withTone(entry, tone, data.toneBases),
      }));
  }, [shownToken, members, me, data, tone]);

  const [active, setActive] = useState(0);
  const key = shownToken ? `${shownToken.kind}:${shownToken.start}:${shownToken.query}` : "";
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new token starts at the first row
  useEffect(() => setActive(0), [key]);

  const pick = (index: number) => {
    const choice = choices[index];
    if (!shownToken || !choice) return;
    const value =
      choice.kind === "member" ? `@${choice.member.realName ?? choice.member.name}` : choice.emoji;
    if (choice.kind === "emoji") useEmojiPrefs.getState().usedEmoji(choice.entry.emoji);
    const placed = putSuggestion(text, shownToken, value);
    put(placed.value, placed.caret);
  };

  const open = choices.length > 0;
  // Esc closes the list only (the panel and an edit stay).
  const closeNow = useRef(() => setClosedAt(shownToken?.start ?? null));
  closeNow.current = () => setClosedAt(shownToken?.start ?? null);
  useEffect(() => {
    if (!open) return;
    return leaveOnEscape(
      (target) => inside(field.current, target),
      () => closeNow.current(),
    );
  }, [open, field]);

  const onKey = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (!open || event.nativeEvent.isComposing) return false;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((i) => (i + step + choices.length) % choices.length);
      return true;
    }
    if ((event.key === "Enter" && !event.shiftKey) || event.key === "Tab") {
      event.preventDefault();
      pick(active);
      return true;
    }
    return false;
  };

  return {
    token: open ? shownToken : null,
    choices,
    active,
    onKey,
    pick,
    listId: "composer-suggest",
  };
}

/** The list over the field. */
export function SuggestBox({ suggestions, lang }: { suggestions: Suggestions; lang: "tr" | "en" }) {
  const { t } = useTranslation();
  const { token, choices, active, pick, listId } = suggestions;
  if (!token || !choices.length) return null;
  return (
    <div
      id={listId}
      role="listbox"
      aria-label={
        token.kind === "member" ? t("socNext.mention.members") : t("socNext.mention.emoji")
      }
      className="absolute inset-x-0 bottom-full z-30 mb-1.5 overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-900/97 p-1 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_18px_44px_rgb(34_6_6/0.55)] ring-1 ring-white/10 backdrop-blur-xl [html[data-perf=on]_&]:backdrop-blur-none"
    >
      {choices.map((choice, index) => (
        // The field's keys move and pick (aria-activedescendant); a pointer press picks too.
        // biome-ignore lint/a11y/useKeyWithClickEvents: the field keeps the focus and the keys
        <div
          key={choice.kind === "member" ? choice.member.id : choice.entry.emoji}
          id={`${listId}-${index}`}
          role="option"
          aria-selected={index === active}
          tabIndex={-1}
          // A press leaves the focus (and the caret) in the field.
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => pick(index)}
          className={cn(
            "flex h-8 cursor-pointer items-center gap-2 rounded-md px-2 text-[13px] text-bone-100 select-none",
            index === active ? "bg-white/9 text-bone-50" : "hover:bg-white/5",
          )}
        >
          {choice.kind === "member" ? (
            <>
              <SocialFace
                source={{
                  name: choice.member.name,
                  avatar: choice.member.avatar ?? "discord",
                  avatarUrl: choice.member.avatarUrl,
                }}
                size={20}
              />
              <span className="min-w-0 truncate font-medium">{choice.member.name}</span>
              {choice.member.realName ? (
                <span className="min-w-0 truncate text-[12px] text-ash-400">
                  {choice.member.realName}
                </span>
              ) : null}
              {choice.member.online ? null : (
                <span className="ml-auto shrink-0 text-[11.5px] text-ash-400">
                  {t("social.presence.offline")}
                </span>
              )}
            </>
          ) : (
            <>
              <span className="w-5 text-center text-[17px] leading-none">{choice.emoji}</span>
              <span className="min-w-0 truncate">{displayName(choice.entry.name, lang)}</span>
            </>
          )}
        </div>
      ))}
    </div>
  );
}
