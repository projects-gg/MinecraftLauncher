// SOC-NEXT §3.9: a conversation's own rows in "Mesajlar" (conv-menu.ts decides them), worded and
// done here: "Okundu say" / "Okunmadı say", "Başa sabitle" / "Sabitlemeyi kaldır", "Sessize al" with
// the crew mute's choices, "Geçmişi temizle" (asked once, in the list's own dialog). And
// "Hepsini okundu say" on the "Mesajlar" tab's right click.
import {
  BellSlash,
  Broom,
  ChatCircleDots,
  Checks,
  PushPin,
  PushPinSlash,
  SpeakerHigh,
} from "@phosphor-icons/react";
import { type MenuItem, toast } from "@pl/ui";
import type { TFunction } from "i18next";
import { create } from "zustand";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { ChatState, Conversation } from "@/lib/api/social-types";
import { ClearDialog } from "../chat/chat-page";
import { chatMutedNow } from "../chat/conversations";
import { rowsOf } from "../menus";
import { MUTE_PRESETS, muteUntilFor } from "../mute";
import { useSocialPrefs } from "../store";
import { conversationMenu, livePins, shownUnread } from "./conv-menu";
import { MAX_PINNED, useSocNextPrefs } from "./prefs";

/** "Geçmişi temizle" asked from the list (the conversation it is about). */
const useClearAsk = create<{ conv: string | null }>()(() => ({ conv: null }));

/** The list's "Geçmişi temizle" question (mounted once by the list). */
export function ListClearDialog() {
  const conv = useClearAsk((s) => s.conv);
  return (
    <ClearDialog
      conv={conv ?? ""}
      open={conv !== null}
      onOpenChange={(open) => {
        if (!open) useClearAsk.setState({ conv: null });
      }}
    />
  );
}

/** A conversation read here: the service hears it when it counted unread ones, the mark goes. */
export function markRead(conv: Pick<Conversation, "id" | "unread">) {
  if (conv.unread > 0) void api.social.chatRead(conv.id);
  useSocNextPrefs.getState().clearUnread(conv.id);
}

/** The conversation's rows, before the person's or the crew's (a separator between). */
export function conversationItems(
  conv: Conversation,
  conversations: readonly Conversation[],
  t: TFunction,
): Array<MenuItem | null> {
  const prefs = useSocNextPrefs.getState();
  const social = useSocialPrefs.getState();
  const now = Date.now();
  const pins = livePins(prefs.pinnedChats, conversations);
  const pinned = pins.includes(conv.id);
  const plan = conversationMenu({
    unread: shownUnread(conv, prefs.unreadMarks) > 0,
    hasMessages: conv.last !== undefined,
    theirsNewest: conv.last !== undefined && !conv.last.mine && conv.last.deletedBy === undefined,
    pinned,
    pinsFull: !pinned && pins.length >= MAX_PINNED,
    muted: chatMutedNow(conv.id, social.chatMutes, now),
  });
  return rowsOf(plan, (row): MenuItem => {
    switch (row.action) {
      case "markRead":
        return {
          label: t("socNext.conv.markRead"),
          icon: <Checks size={16} />,
          onSelect: () => {
            markRead(conv);
            announce(t("socNext.conv.markRead"));
          },
        };
      case "markUnread":
        return {
          label: t("socNext.conv.markUnread"),
          icon: <ChatCircleDots size={16} />,
          onSelect: () => {
            if (conv.last) useSocNextPrefs.getState().markUnread(conv.id, conv.last.at);
            announce(t("socNext.conv.markUnread"));
          },
        };
      case "pin":
        return {
          label: t("socNext.conv.pin"),
          icon: <PushPin size={16} />,
          onSelect: () => {
            const live = livePins(useSocNextPrefs.getState().pinnedChats, conversations);
            if (live.length >= MAX_PINNED) {
              toast(t("socNext.conv.pinsFull"));
              return;
            }
            useSocNextPrefs.getState().restore({ pinnedChats: [...live, conv.id] });
            announce(t("socNext.conv.pinned"));
          },
        };
      case "unpin":
        return {
          label: t("socNext.conv.unpin"),
          icon: <PushPinSlash size={16} />,
          onSelect: () => {
            useSocNextPrefs.getState().setPinned(conv.id, false);
            announce(t("socNext.conv.unpin"));
          },
        };
      case "mute":
        return {
          label: t("chat.actions.mute"),
          icon: <BellSlash size={16} />,
          onSelect: () => {},
          items: MUTE_PRESETS.map((preset) => ({
            label: t(`teams.mute.presets.${preset}`),
            onSelect: () => {
              const until = muteUntilFor(preset, Date.now());
              useSocialPrefs.getState().setChatMute(conv.id, until === null ? null : until * 1000);
            },
          })),
        };
      case "unmute":
        return {
          label: t("chat.actions.unmute"),
          icon: <SpeakerHigh size={16} />,
          onSelect: () => useSocialPrefs.getState().setChatMute(conv.id, undefined),
        };
      case "clear":
        return {
          label: t("chat.actions.clear"),
          icon: <Broom size={16} />,
          onSelect: () => {
            // After the menu has let go of the focus.
            window.setTimeout(() => useClearAsk.setState({ conv: conv.id }), 0);
          },
        };
    }
  });
}

/** "Hepsini okundu say" (the "Mesajlar" tab's right click), off while nothing is unread. */
export function allReadItems(chat: ChatState, t: TFunction): Array<MenuItem | null> {
  const marks = useSocNextPrefs.getState().unreadMarks;
  const unread = chat.conversations.filter((conv) => shownUnread(conv, marks) > 0);
  return [
    {
      label: t("socNext.conv.allRead"),
      icon: <Checks size={16} />,
      disabled: unread.length === 0,
      onSelect: () => {
        for (const conv of unread) markRead(conv);
        useSocNextPrefs.getState().clearAllUnread();
        announce(t("socNext.conv.allRead"));
      },
    },
  ];
}
