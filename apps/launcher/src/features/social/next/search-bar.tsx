// SOC-NEXT §3.2: "Sohbette ara". Ctrl+F in an open conversation (or the header's magnifier) opens a
// quiet bar over the messages: what is written searches this computer's history of the
// conversation 150 ms after the last key, two characters at least; "3/12" counts the results, the
// newest first; ↑ and Enter go to an older one, ↓ and Shift+Enter to a newer one, the message
// centred and its words marked (the gold one is the result shown). Esc (or ✕) closes it and the
// field takes the focus back. The history is read page by page from this computer while the bar is
// open (5,000 messages at most), the newest page first; nothing is asked of the service. It opens
// and steps without motion (a keyboard tool), as the command palette does.
import { CaretDown, CaretUp, MagnifyingGlass, X } from "@phosphor-icons/react";
import { cn, IconButton } from "@pl/ui";
import { type RefObject, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { api } from "@/lib/api";
import type { ChatMessage } from "@/lib/api/social-types";
import { chatKeyHere } from "../chat/focus";
import { inside, leaveOnEscape } from "../edit-escape";
import {
  mergeHistory,
  SEARCH_WAIT_MS,
  searchMessages,
  searchQuery,
  stepResult,
} from "./chat-search";

/** At most this many messages are searched (the history this computer keeps). */
const HISTORY_MAX = 5_000;

interface ChatSearchUi {
  /** The conversation whose search is open. */
  conv: string | null;
  /** Grows when the open bar should take the focus again (Ctrl+F once more, the magnifier). */
  focus: number;
  /** The result shown (a message key), for the words' marks. */
  current: string | null;
  /** The folded query searched now. */
  query: string | null;
  open: (conv: string) => void;
  close: () => void;
  show: (query: string | null, current: string | null) => void;
}

export const useChatSearch = create<ChatSearchUi>()((set, get) => ({
  conv: null,
  focus: 0,
  current: null,
  query: null,
  open: (conv) =>
    set((s) =>
      s.conv === conv
        ? { focus: s.focus + 1 }
        : { conv, focus: s.focus + 1, current: null, query: null },
    ),
  close: () => {
    if (get().conv !== null) set({ conv: null, current: null, query: null });
  },
  show: (query, current) =>
    set((s) => (s.query === query && s.current === current ? s : { query, current })),
}));

/** Opens a conversation's search (the header's magnifier). */
export function openChatSearch(conv: string) {
  useChatSearch.getState().open(conv);
}

/** Ctrl+F (⌘F) while the focus is in the conversation's panel opens its search. */
export function useSearchShortcut(
  root: RefObject<HTMLElement | null>,
  conv: string,
  live: boolean,
) {
  useEffect(() => {
    if (!live) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.altKey || event.shiftKey) return;
      if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== "f") return;
      const box = root.current;
      // SQOL-U3: only from inside the panel that holds the conversation (chat/focus.ts).
      if (!box || !chatKeyHere(box, event.target)) return;
      event.preventDefault();
      openChatSearch(conv);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [root, conv, live]);
}

/** The conversation's history from this computer, read page by page while the search is open. */
function useHistory(conv: string, open: boolean): { messages: ChatMessage[]; loading: boolean } {
  const [state, setState] = useState<{ messages: ChatMessage[]; loading: boolean }>({
    messages: [],
    loading: false,
  });
  useEffect(() => {
    if (!open) {
      setState({ messages: [], loading: false });
      return;
    }
    let alive = true;
    setState({ messages: [], loading: true });
    void (async () => {
      let held: ChatMessage[] = [];
      let before: number | undefined;
      try {
        for (;;) {
          const page = await api.social.chatPage(conv, before);
          if (!alive) return;
          held = [...page.messages, ...held];
          const oldest = page.messages.find((m) => m.id !== undefined)?.id;
          setState({ messages: held, loading: page.more && held.length < HISTORY_MAX });
          if (!page.more || oldest === undefined || held.length >= HISTORY_MAX) break;
          before = oldest;
        }
      } catch {
        if (alive) setState({ messages: held, loading: false });
      }
    })();
    return () => {
      alive = false;
    };
  }, [conv, open]);
  return state;
}

export function ChatSearchBar({
  conv,
  shown,
  textOf,
  onJump,
  onClosed,
}: {
  conv: string;
  /** The messages the conversation holds now (their newest words). */
  shown: readonly ChatMessage[];
  /** A message's words as shown (an edit on its way shows its new text). */
  textOf: (message: ChatMessage) => string;
  /** Shows a result: the message centred (an older page loads first when needed). */
  onJump: (key: string) => void;
  /** The bar closed: the composer takes the focus. */
  onClosed: () => void;
}) {
  const { t } = useTranslation();
  const open = useChatSearch((s) => s.conv === conv);
  const focus = useChatSearch((s) => s.focus);
  const field = useRef<HTMLInputElement>(null);
  const bar = useRef<HTMLElement>(null);
  const [text, setText] = useState("");
  const [query, setQuery] = useState<string | null>(null);
  const [index, setIndex] = useState(-1);
  const history = useHistory(conv, open);

  // The field takes the focus when the bar opens and at every Ctrl+F after.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `focus` asks for it again
  useEffect(() => {
    if (!open) return;
    field.current?.focus();
    field.current?.select();
  }, [open, focus]);

  // A fresh bar starts empty.
  useEffect(() => {
    if (open) return;
    setText("");
    setQuery(null);
    setIndex(-1);
  }, [open]);

  // The search runs 150 ms after the last key.
  useEffect(() => {
    const timer = window.setTimeout(() => setQuery(searchQuery(text)), SEARCH_WAIT_MS);
    return () => window.clearTimeout(timer);
  }, [text]);

  const all = useMemo(() => mergeHistory(history.messages, shown), [history.messages, shown]);
  const results = useMemo(
    () => (query ? searchMessages(all, query, textOf) : []),
    [all, query, textOf],
  );

  // A new query shows its newest result; more history coming in keeps the one shown.
  const shownKey = index >= 0 ? (results[index] ?? null) : null;
  const lastQuery = useRef<string | null>(null);
  useEffect(() => {
    if (query !== lastQuery.current) {
      lastQuery.current = query;
      setIndex(results.length ? 0 : -1);
      return;
    }
    if (index < 0 && results.length) setIndex(0);
    else if (index >= results.length) setIndex(results.length - 1);
  }, [query, results, index]);

  useEffect(() => {
    useChatSearch.getState().show(open ? query : null, open ? shownKey : null);
  }, [open, query, shownKey]);
  useEffect(() => {
    if (open && shownKey) onJump(shownKey);
  }, [open, shownKey, onJump]);

  const close = () => {
    useChatSearch.getState().close();
    onClosed();
  };
  // Esc in the bar closes it and nothing else (the panel stays open).
  const closeNow = useRef(close);
  closeNow.current = close;
  useEffect(() => {
    if (!open) return;
    return leaveOnEscape(
      (target) => inside(bar.current, target),
      () => closeNow.current(),
    );
  }, [open]);

  if (!open) return null;
  const step = (direction: 1 | -1) => setIndex((i) => stepResult(i, results.length, direction));
  const total = results.length;
  const status =
    query === null
      ? null
      : total > 0
        ? `${index + 1}/${total}`
        : history.loading
          ? t("socNext.search.loading")
          : t("socNext.search.none");
  return (
    <search
      ref={bar}
      className="absolute inset-x-1 top-0 z-20 flex h-10 items-center gap-1 rounded-[var(--radius-panel)] bg-obsidian-900/96 pr-1 pl-2.5 shadow-[0_12px_32px_rgb(34_6_6/0.55)] ring-1 ring-white/10 backdrop-blur-xl [html[data-perf=on]_&]:backdrop-blur-none"
    >
      <MagnifyingGlass size={15} className="shrink-0 text-ash-400" aria-hidden />
      <input
        ref={field}
        value={text}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            step(event.shiftKey ? -1 : 1);
          } else if (event.key === "ArrowUp") {
            event.preventDefault();
            step(1);
          } else if (event.key === "ArrowDown") {
            event.preventDefault();
            step(-1);
          }
        }}
        aria-label={t("socNext.search.placeholder")}
        placeholder={t("socNext.search.placeholder")}
        maxLength={100}
        className="min-w-0 flex-1 bg-transparent text-[13.5px] text-bone-50 outline-none placeholder:text-ash-500"
      />
      {/* As wide as "100/100": the buttons beside it never move. */}
      <span
        role="status"
        className={cn(
          "min-w-[5.5rem] shrink-0 text-right text-[12px] tabular-nums transition-opacity duration-150",
          status ? "opacity-100" : "opacity-0",
          total > 0 ? "text-bone-100" : "text-ash-400",
        )}
      >
        {status}
      </span>
      <IconButton
        label={t("socNext.search.older")}
        icon={<CaretUp size={15} weight="bold" />}
        size="sm"
        disabled={total < 2}
        onClick={() => step(1)}
      />
      <IconButton
        label={t("socNext.search.newer")}
        icon={<CaretDown size={15} weight="bold" />}
        size="sm"
        disabled={total < 2}
        onClick={() => step(-1)}
      />
      <IconButton
        label={t("socNext.search.close")}
        icon={<X size={15} />}
        size="sm"
        onClick={close}
      />
    </search>
  );
}
