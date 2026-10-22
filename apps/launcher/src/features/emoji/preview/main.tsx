// Static preview of the emoji and sticker picker (CHAT2-L; how to build it: vite.config.ts here).
// A friends panel over the home art with a short conversation and the composer's picker button.
// Switches in the address: lang=en, open=1 (opens the picker), tab=stickers, tone=0…5,
// recents=1 (a few recent emoji and stickers), perf=1 (Performance mode), nostickers=1.
import "@/styles.css";
import "@/assets/fonts/emoji/emoji.css";
import "../picker.css";
import { PaperPlaneRight } from "@phosphor-icons/react";
import { findSticker, type Sticker as StickerData } from "@pl/stickers";
import { cn, guardReducedMotion, PerfProvider, TooltipProvider } from "@pl/ui";
import { StrictMode, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import hero from "@/assets/art/hero-dragon-1.webp";
import i18n from "@/lib/i18n";
import { enChat2 } from "@/locales/en-chat2";
import { trChat2 } from "@/locales/tr-chat2";
import { bigEmojiCount, EmojiPickerButton, insertText, Sticker, UnknownSticker } from "../index";
import { useEmojiPrefs } from "../store";
import { isSkinTone } from "../tone";

const params = new URLSearchParams(location.search);
const lang = params.get("lang") === "en" ? "en" : "tr";
i18n.addResourceBundle("tr", "translation", { chat2: trChat2 }, true, true);
i18n.addResourceBundle("en", "translation", { chat2: enChat2 }, true, true);
void i18n.changeLanguage(lang);
document.documentElement.lang = lang;
if (params.get("perf") === "1") document.documentElement.dataset.perf = "on";

const tone = Number(params.get("tone") ?? "0");
if (isSkinTone(tone)) useEmojiPrefs.setState({ tone });
if (params.get("recents") === "1") {
  useEmojiPrefs.setState({
    recentEmoji: ["😂", "❤️", "👍", "🔥", "😭", "🙏", "🥺", "💀", "✨", "🎉", "😎", "🇹🇷"],
    recentStickers: ["pl1/gg", "pl1/lol", "pl1/come", "pl1/hi"],
  });
}

type Message =
  | { id: number; mine: boolean; text: string }
  | { id: number; mine: boolean; sticker: string; arrive?: boolean };

const WORDS = {
  tr: {
    name: "Ayşe",
    status: "Oyunda",
    field: "Mesaj yaz",
    send: "Gönder",
    messages: [
      { id: 1, mine: false, text: "Akşam kaleye geliyor musun? 🏰" },
      { id: 2, mine: true, text: "Geliyorum 🔥🔥" },
      { id: 3, mine: false, sticker: "pl1/come" },
      { id: 4, mine: true, text: "😂😂" },
      { id: 5, mine: false, sticker: "pl2/new" },
    ] as Message[],
  },
  en: {
    name: "Ayşe",
    status: "In game",
    field: "Write a message",
    send: "Send",
    messages: [
      { id: 1, mine: false, text: "Coming to the castle tonight? 🏰" },
      { id: 2, mine: true, text: "On my way 🔥🔥" },
      { id: 3, mine: false, sticker: "pl1/come" },
      { id: 4, mine: true, text: "😂😂" },
      { id: 5, mine: false, sticker: "pl2/new" },
    ] as Message[],
  },
}[lang];

function Bubble({ mine, children }: { mine: boolean; children: string }) {
  const big = bigEmojiCount(children);
  if (big) {
    return <p className="emoji-glyph px-1 text-[32px] leading-[1.2]">{children}</p>;
  }
  return (
    <p
      className={cn(
        "max-w-[78%] rounded-[10px] px-3 py-[7px] text-[13.5px] leading-[1.45] ring-1 ring-inset",
        mine
          ? "bg-[color-mix(in_oklab,var(--color-gold-500)_17%,var(--color-obsidian-800))] text-bone-50 ring-gold-400/20"
          : "bg-obsidian-800/85 text-bone-100 ring-white/7",
      )}
      style={{ fontFamily: '"Figtree Variable", "PL Emoji", sans-serif' }}
    >
      {children}
    </p>
  );
}

function Preview() {
  const [messages, setMessages] = useState<Message[]>(WORDS.messages);
  const [text, setText] = useState("");
  const field = useRef<HTMLTextAreaElement>(null);
  const selection = useRef({ start: 0, end: 0 });
  const opener = useRef<HTMLDivElement>(null);

  // open=1: the picker opens by itself (screenshots).
  useEffect(() => {
    if (params.get("open") !== "1") return;
    const timer = window.setTimeout(() => {
      opener.current?.querySelector<HTMLButtonElement>("button")?.click();
    }, 300);
    return () => window.clearTimeout(timer);
  }, []);

  const remember = () => {
    const element = field.current;
    if (element) selection.current = { start: element.selectionStart, end: element.selectionEnd };
  };
  const onEmoji = (emoji: string) => {
    const next = insertText(text, selection.current.start, selection.current.end, emoji, 300);
    if (!next) return;
    setText(next.value);
    selection.current = { start: next.caret, end: next.caret };
  };
  const onSticker = (sticker: StickerData) => {
    setMessages((list) => [
      ...list,
      { id: Date.now(), mine: true, sticker: sticker.ref, arrive: true },
    ]);
  };

  return (
    <div className="fixed inset-0 overflow-hidden">
      <img src={hero} alt="" className="absolute inset-0 size-full object-cover brightness-75" />
      <aside className="glass-strong absolute top-0 right-0 bottom-0 flex w-[440px] flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)]">
        <header className="flex h-[58px] shrink-0 items-center gap-3 border-b border-white/6 px-4">
          <span className="size-8 rounded-[4px] bg-[linear-gradient(135deg,#8e4a12,#f0b849)]" />
          <span className="flex flex-col">
            <span className="text-[14px] font-semibold text-bone-50">{WORDS.name}</span>
            <span className="text-[12px] text-jade-400">{WORDS.status}</span>
          </span>
        </header>
        <div className="flex min-h-0 flex-1 flex-col justify-end gap-2.5 overflow-hidden px-4 pb-2">
          {messages.map((message) => (
            <div
              key={message.id}
              className={cn("flex", message.mine ? "justify-end" : "justify-start")}
            >
              {"text" in message ? (
                <Bubble mine={message.mine}>{message.text}</Bubble>
              ) : (
                <StickerMessage reference={message.sticker} arrive={message.arrive} />
              )}
            </div>
          ))}
        </div>
        <form
          className="shrink-0 px-3 pt-2 pb-3"
          onSubmit={(event) => {
            event.preventDefault();
            if (!text.trim()) return;
            setMessages((list) => [...list, { id: Date.now(), mine: true, text }]);
            setText("");
          }}
        >
          <div className="flex items-end gap-1 rounded-[var(--radius-panel)] border border-white/10 bg-obsidian-800/80 pr-[3px] focus-within:border-gold-400/60 focus-within:ring-2 focus-within:ring-gold-400/20">
            <textarea
              ref={field}
              rows={1}
              value={text}
              onChange={(event) => {
                setText(event.target.value);
                remember();
              }}
              onSelect={remember}
              onKeyUp={remember}
              aria-label={WORDS.field}
              placeholder={WORDS.field}
              className="min-w-0 flex-1 resize-none bg-transparent py-[9px] pl-3.5 text-[13.5px] leading-5 text-bone-50 outline-none placeholder:text-ash-500"
              style={{ height: 38, fontFamily: '"Figtree Variable", "PL Emoji", sans-serif' }}
            />
            <div ref={opener} className="mb-[3px] flex">
              <EmojiPickerButton
                onEmoji={onEmoji}
                onSticker={params.get("nostickers") === "1" ? undefined : onSticker}
                initialTab={params.get("tab") === "stickers" ? "stickers" : "emoji"}
                returnFocus={() => field.current}
              />
            </div>
            <button
              type="submit"
              aria-label={WORDS.send}
              className="relative mb-[3px] inline-flex size-8 shrink-0 items-center justify-center rounded-md text-ash-500"
            >
              <PaperPlaneRight size={17} aria-hidden />
            </button>
          </div>
        </form>
      </aside>
    </div>
  );
}

function StickerMessage({ reference, arrive }: { reference: string; arrive?: boolean }) {
  const sticker = findSticker(reference);
  const label = i18n.t("chat2.stickers.unknown");
  return sticker ? (
    <Sticker sticker={sticker} size={112} arrive={arrive} alt={sticker.name[lang]} />
  ) : (
    <UnknownSticker size={112} label={label} />
  );
}

guardReducedMotion();
const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <TooltipProvider>
        <PerfProvider performanceMode={params.get("perf") === "1"}>
          <Preview />
        </PerfProvider>
      </TooltipProvider>
    </StrictMode>,
  );
}
