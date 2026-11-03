// SOC-NOTE §3.8: a crew chat's pinned messages. Its leader and moderators pin up to three from a
// message's menu ("Sabitle"; a fourth asks to take the oldest off) and take one off there or from
// the list ("Sabitlemeyi kaldır"). Over the chat, one quiet line: the shown pin's author and text,
// "1/3" when there are more, and the list. Pressing the line goes to that message when this
// computer holds it (and the next press to the one before, Telegram's way); otherwise the list
// opens, with each pin's text in full. The line comes and goes as a fade (and a 4 px rise), a new
// shown pin swaps in place; nothing moves under it (the list keeps room at its top).
import { ListBullets, PushPin, PushPinSlash } from "@phosphor-icons/react";
import { Button, cn, Dialog, easeOutExpo, IconButton, returnFocusAfterKeys, toast } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { Popover } from "radix-ui";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { useCalm } from "@/features/accounts/calm";
import { announce, friendlyError } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { ChatMessage, ChatPin, SocialTeam } from "@/lib/api/social-types";
import i18n from "@/lib/i18n";
import { useChat } from "../chat/chat-store";
import type { ChatPerson } from "../chat/sender-name";
import { focusFirstAction } from "../crew-badge";
import { enter, leave, swap, useSocialMotion } from "../motion";
import { useSocNext } from "../next/flag";
import { SocialFace } from "../social-face";
import { useSocialUi } from "../store";

/** Pins a crew chat holds at most (the service's `PINS.max`). */
export const PINS_MAX = 3;
/** Messages older than this are not pinned (the service knows deletions for a week). */
export const PIN_WINDOW_MS = 7 * 86_400_000;

const NO_PINS: readonly ChatPin[] = [];

/** Pinned messages show: SOC-NEXT's switch (signed config) and the service's (Studio). */
export function usePinsOn(): boolean {
  const flag = useSocNext();
  const on = useSocialUi((s) => s.view.chat?.socNote?.pinsOn === true);
  return flag && on;
}

/** A crew chat's pins, the oldest pin first (none while pins are off). */
export function useCrewPins(conv: string | null): readonly ChatPin[] {
  const on = usePinsOn();
  // By their words: every snapshot brings new arrays, the rows draw again only when pins change.
  const key = useSocialUi((s) => {
    const pins = conv ? s.view.chat?.socNote?.pins[conv] : undefined;
    return pins?.length ? JSON.stringify(pins) : "";
  });
  return useMemo(() => (on && key ? (JSON.parse(key) as ChatPin[]) : NO_PINS), [on, key]);
}

/** The crew's leader and moderators pin and take pins off. */
export function canPinIn(team: Pick<SocialTeam, "role" | "moderator"> | undefined): boolean {
  return team !== undefined && (team.role === "founder" || team.moderator);
}

/** Whether a message may be pinned: a sent text message of the week (not a sticker, not gone). */
export function pinnable(message: ChatMessage, nowMs: number): boolean {
  return (
    message.id !== undefined &&
    message.state === "sent" &&
    !message.sticker &&
    !message.deleted &&
    message.text.length > 0 &&
    message.at > nowMs - PIN_WINDOW_MS
  );
}

/** The pin shown after one more press: the one before it, the newest again after the oldest. */
export function nextShown(index: number, count: number): number {
  if (count <= 0) return 0;
  return index <= 0 ? count - 1 : index - 1;
}

/** "Sabitlemeyi kaldır" / "Sabitle" in a message's menu, while this conversation takes pins. */
export function pinChoice(
  message: ChatMessage,
  pins: readonly ChatPin[],
  manage: boolean,
  nowMs: number,
): "pin" | "unpin" | null {
  if (!manage || message.id === undefined) return null;
  if (pins.some((pin) => pin.id === message.id)) return "unpin";
  return pinnable(message, nowMs) ? "pin" : null;
}

// --- actions -------------------------------------------------------------------------------------

const usePinReplace = create<{ ask: { conv: string; id: number; oldest: number } | null }>()(
  () => ({ ask: null }),
);

async function sendPin(conv: string, id: number, replace: number | undefined, done: string) {
  try {
    await api.social.chatPin(conv, id, replace);
    announce(done);
    toast.success(done);
  } catch (error) {
    toast.error(friendlyError(error, i18n.t("social.actions.failed")).message);
  }
}

/** "Sabitle": three pins there already, the player is asked to take the oldest off. */
export function pinMessage(
  conv: string,
  message: ChatMessage,
  pins: readonly ChatPin[],
  t: (key: string) => string,
) {
  if (message.id === undefined) return;
  const oldest = pins[0];
  if (pins.length >= PINS_MAX && oldest) {
    window.setTimeout(
      () => usePinReplace.setState({ ask: { conv, id: message.id as number, oldest: oldest.id } }),
      0,
    );
    return;
  }
  void sendPin(conv, message.id, undefined, t("socNote.pins.done"));
}

/** "Sabitlemeyi kaldır". */
export async function unpinMessage(conv: string, id: number, t: (key: string) => string) {
  try {
    await api.social.chatUnpin(conv, id);
    const done = t("socNote.pins.undone");
    announce(done);
    toast.success(done);
  } catch (error) {
    toast.error(friendlyError(error, t("social.actions.failed")).message);
  }
}

/** "Sabitler dolu": the oldest pin goes for this message (one sentence: the oldest goes). */
export function PinReplaceDialog() {
  const { t } = useTranslation();
  const ask = usePinReplace((s) => s.ask);
  const close = () => usePinReplace.setState({ ask: null });
  const last = useRef(ask);
  if (ask) last.current = ask;
  const shown = ask ?? last.current;
  return (
    <Dialog
      open={ask !== null}
      onOpenChange={(open) => {
        if (!open) close();
      }}
      title={t("socNote.pins.fullTitle")}
      description={t("socNote.pins.fullText")}
      closeLabel={t("common.close")}
      width={400}
      footer={
        <>
          <Button variant="ghost" onClick={close}>
            {t("social.actions.cancel")}
          </Button>
          <Button
            variant="primary"
            onClick={() => {
              if (!shown) return;
              close();
              void sendPin(shown.conv, shown.id, shown.oldest, t("socNote.pins.done"));
            }}
          >
            {t("socNote.pins.replace")}
          </Button>
        </>
      }
    />
  );
}

// --- the line over the chat ----------------------------------------------------------------------

/** Whether this computer holds the message now (loaded in the conversation on screen). */
function held(conv: string, id: number): boolean {
  return Boolean(useChat.getState().threads[conv]?.messages.some((m) => m.id === id));
}

export function PinStrip({
  conv,
  pins,
  people,
  manage,
  onJump,
}: {
  conv: string;
  /** The oldest pin first. */
  pins: readonly ChatPin[];
  people: (id: string) => ChatPerson;
  /** The player leads or moderates the crew: the list offers "Sabitlemeyi kaldır". */
  manage: boolean;
  /** Goes to a message by its key (and lights it). */
  onJump: (key: string) => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const calm = useCalm();
  const [index, setIndex] = useState(pins.length - 1);
  const [open, setOpen] = useState(false);
  // A new pin shows first; a pin taken off leaves the newest in front.
  const count = pins.length;
  const newest = pins[count - 1]?.id;
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new newest pin comes to the front
  useEffect(() => setIndex(count - 1), [newest, count]);
  const at = Math.min(Math.max(index, 0), count - 1);
  const pin = pins[at];
  const newestFirst = useMemo(() => [...pins].reverse(), [pins]);
  if (!pin) return null;
  const author = people(pin.from);
  const position = count - at;
  const press = () => {
    if (held(conv, pin.id)) {
      onJump(`i:${pin.id}`);
      if (count > 1) setIndex(nextShown(at, count));
    } else {
      setOpen(true);
    }
  };
  const go = (target: ChatPin) => {
    setOpen(false);
    if (held(conv, target.id)) {
      onJump(`i:${target.id}`);
    } else {
      toast(t("socNote.pins.gone"));
    }
  };
  return (
    <div className="absolute inset-x-1 top-0 z-10 flex h-10 items-center gap-1 rounded-[var(--radius-panel)] bg-obsidian-900/92 pr-1 pl-1 shadow-[0_10px_28px_rgb(34_6_6/0.45)] ring-1 ring-white/8 backdrop-blur-xl [html[data-perf=on]_&]:backdrop-blur-none">
      <button
        type="button"
        onClick={press}
        aria-label={`${t("socNote.pins.strip")}${count > 1 ? `, ${t("socNote.pins.position", { at: position, count })}` : ""}: ${author.name}, ${pin.text}`}
        className="flex h-8 min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-md px-2 text-left outline-none transition-[background-color,transform,scale] duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.99]"
      >
        <PushPin size={14} weight="fill" className="shrink-0 text-ash-300" aria-hidden />
        <span className="relative h-[18px] min-w-0 flex-1 overflow-hidden">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span
              key={pin.id}
              className="absolute inset-x-0 truncate text-[12.5px] leading-[18px]"
              {...swap(reduce)}
            >
              <span className="font-semibold text-bone-100">{author.name}</span>
              <span className="text-ash-300"> {pin.text}</span>
            </motion.span>
          </AnimatePresence>
        </span>
        {count > 1 ? (
          <span className="shrink-0 text-[11.5px] text-ash-400 tabular-nums">
            {t("socNote.pins.position", { at: position, count })}
          </span>
        ) : null}
      </button>
      <Popover.Root open={open} onOpenChange={setOpen}>
        <Popover.Trigger asChild>
          <IconButton
            size="sm"
            label={t("socNote.pins.list")}
            icon={<ListBullets size={16} />}
            className="shrink-0"
          />
        </Popover.Trigger>
        <AnimatePresence>
          {open ? (
            <Popover.Portal forceMount>
              <Popover.Content
                forceMount
                asChild
                align="end"
                sideOffset={6}
                collisionPadding={12}
                onOpenAutoFocus={focusFirstAction}
                onCloseAutoFocus={returnFocusAfterKeys}
                aria-label={t("socNote.pins.list")}
              >
                <motion.div
                  className="z-50 w-[300px] rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-1.5 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
                  style={{ transformOrigin: "var(--radix-popover-content-transform-origin)" }}
                  initial={calm({ opacity: 0, scale: 0.96 })}
                  animate={calm({
                    opacity: 1,
                    scale: 1,
                    transition: { duration: 0.16, ease: easeOutExpo },
                  })}
                  exit={calm({ opacity: 0, scale: 0.98, transition: { duration: 0.11 } })}
                >
                  <p className="px-2 pt-1 pb-1.5 text-[12px] font-medium text-ash-400">
                    {t("socNote.pins.list")}
                  </p>
                  <ul className="flex flex-col">
                    {newestFirst.map((entry) => {
                      const who = people(entry.from);
                      return (
                        <li key={entry.id} className="group/pin relative flex items-start">
                          <button
                            type="button"
                            onClick={() => go(entry)}
                            aria-label={`${t("socNote.pins.jump")}: ${who.name}, ${entry.text}`}
                            className={cn(
                              "flex min-w-0 flex-1 cursor-pointer items-start gap-2.5 rounded-md px-2 py-2 text-left outline-none transition-colors duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300",
                              manage && "pr-9",
                            )}
                          >
                            <SocialFace source={who} size={22} className="mt-0.5 shrink-0" />
                            <span className="min-w-0">
                              <span className="block truncate text-[12.5px] font-semibold text-bone-100">
                                {who.name}
                              </span>
                              <span className="line-clamp-3 text-[12.5px] leading-snug break-words text-ash-300">
                                {entry.text}
                              </span>
                            </span>
                          </button>
                          {manage ? (
                            <IconButton
                              size="sm"
                              label={t("socNote.pins.unpin")}
                              icon={<PushPinSlash size={14} />}
                              tooltipSide="left"
                              className="absolute top-1.5 right-1"
                              onClick={() => void unpinMessage(conv, entry.id, t)}
                            />
                          ) : null}
                        </li>
                      );
                    })}
                  </ul>
                  <p className="px-2 pt-1.5 pb-1 text-[11.5px] leading-snug text-ash-400">
                    {t("socNote.pins.listHint")}
                  </p>
                </motion.div>
              </Popover.Content>
            </Popover.Portal>
          ) : null}
        </AnimatePresence>
      </Popover.Root>
    </div>
  );
}

/** The line over the chat, coming and going as a fade (and a 4 px rise). */
export function PinStripSlot(props: Parameters<typeof PinStrip>[0]) {
  const { reduce } = useSocialMotion();
  return (
    <AnimatePresence initial={false}>
      {props.pins.length ? (
        <motion.div
          key="pins"
          className="pointer-events-none absolute inset-x-0 top-0 z-10 [&>*]:pointer-events-auto"
          initial={{ opacity: 0, transform: reduce ? "none" : "translateY(-4px)" }}
          animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.2) }}
          exit={{ opacity: 0, transition: leave(0.14) }}
        >
          <PinStrip {...props} />
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
