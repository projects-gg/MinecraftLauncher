// A message's small toolbar (CHAT2 §6, §7): it comes at the bubble's top corner while the pointer
// is on the message or the keyboard is in it, and fades away after (opacity and 2 px, nothing
// else moves): 😊+ (the quick reactions and "+", the whole picker), ↩ "Yanıtla" and "…" (the
// message's menu, the same rows as its right click). One Tab stop; the arrows move inside it.
import { ArrowBendUpLeft, DotsThree, Plus, Smiley } from "@phosphor-icons/react";
import { cn, IconButton, Menu, type MenuItem } from "@pl/ui";
import { Popover } from "radix-ui";
import { type KeyboardEvent, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { QuickReactions } from "./reaction-ui";

export function MessageToolbar({
  mine,
  reactable,
  repliable,
  quick,
  given,
  menu,
  onReact,
  onReply,
}: {
  mine: boolean;
  reactable: boolean;
  repliable: boolean;
  /** The quick row's six emoji. */
  quick: readonly string[];
  given: (emoji: string) => boolean;
  /** The message's menu rows (without its reactions, which have their own button). */
  menu: Array<MenuItem | null>;
  onReact: (emoji: string) => void;
  onReply: () => void;
}) {
  const { t } = useTranslation();
  const bar = useRef<HTMLDivElement>(null);
  const [reacting, setReacting] = useState(false);
  const [focus, setFocus] = useState(0);
  const buttons = () => [
    ...(bar.current?.querySelectorAll<HTMLButtonElement>("[data-tool]") ?? []),
  ];
  // Its buttons now (one may go while it is open: the message turned a week old, a switch).
  const count = (reactable ? 1 : 0) + (repliable ? 1 : 0) + 1;
  const current = Math.min(focus, count - 1);
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    // Keys from its own layers (the quick row, the picker, the menu: portals whose events still
    // bubble here) stay theirs.
    if (!event.currentTarget.contains(event.target as Node)) return;
    const delta = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (!delta) return;
    event.preventDefault();
    const list = buttons();
    const next = (current + delta + list.length) % list.length;
    setFocus(next);
    list[next]?.focus();
  };
  const tool = (index: number) => ({
    "data-tool": "",
    tabIndex: index === current ? 0 : -1,
    onFocus: () => setFocus(index),
  });
  let index = 0;
  return (
    <div
      ref={bar}
      role="toolbar"
      aria-label={t("chat2.toolbar.label")}
      onKeyDown={onKeyDown}
      className={cn(
        "absolute -top-7 z-10 flex items-center gap-0.5 rounded-[var(--radius-panel)] bg-obsidian-850 p-0.5 shadow-[0_10px_30px_rgb(34_6_6/0.5)] ring-1 ring-white/10",
        mine ? "left-0" : "right-0",
        // Shown while the pointer is on the message, the keyboard in it, or one of its layers open.
        "pointer-events-none translate-y-[2px] opacity-0 transition-[opacity,translate] duration-150 ease-out",
        "group-hover/msg:pointer-events-auto group-hover/msg:translate-y-0 group-hover/msg:opacity-100",
        "focus-within:pointer-events-auto focus-within:translate-y-0 focus-within:opacity-100",
        "has-[[data-state=open]]:pointer-events-auto has-[[data-state=open]]:translate-y-0 has-[[data-state=open]]:opacity-100",
      )}
    >
      {reactable ? (
        <Popover.Root open={reacting} onOpenChange={setReacting}>
          <Popover.Trigger asChild>
            <IconButton
              label={t("chat2.toolbar.react")}
              size="sm"
              tooltipSide="top"
              icon={
                <span className="relative flex">
                  <Smiley size={17} aria-hidden />
                  <Plus
                    size={8}
                    weight="bold"
                    aria-hidden
                    className="absolute -top-0.5 -right-1 rounded-full bg-obsidian-850"
                  />
                </span>
              }
              {...tool(index++)}
            />
          </Popover.Trigger>
          <Popover.Portal>
            <Popover.Content
              side="top"
              align={mine ? "end" : "start"}
              sideOffset={6}
              collisionPadding={12}
              className="pop-motion z-50 rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] outline-none"
            >
              <QuickReactions
                emoji={quick}
                given={given}
                onPick={(emoji) => {
                  setReacting(false);
                  onReact(emoji);
                }}
                onMore={(emoji) => {
                  setReacting(false);
                  onReact(emoji);
                }}
              />
            </Popover.Content>
          </Popover.Portal>
        </Popover.Root>
      ) : null}
      {repliable ? (
        <IconButton
          label={t("chat2.toolbar.reply")}
          size="sm"
          tooltipSide="top"
          icon={<ArrowBendUpLeft size={16} weight="bold" aria-hidden />}
          onClick={onReply}
          {...tool(index++)}
        />
      ) : null}
      {menu.length ? (
        <Menu
          items={menu}
          trigger={
            <IconButton
              label={t("chat2.toolbar.more")}
              size="sm"
              tooltipSide="top"
              icon={<DotsThree size={18} weight="bold" />}
              {...tool(index++)}
            />
          }
        />
      ) : null}
    </div>
  );
}
