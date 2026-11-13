// The chat under the room's people (SQOL §1.3 "Hangi sohbet"): a call with a friend that never held
// a third shows the conversation with that friend (chat's own, slim: "Mesajlar" over it); otherwise
// the room's own chat while the room has one ("Oda Sohbeti", and in a crew room a quiet "Ekip
// Sohbeti" beside it that opens the crew's chat in the friends panel); else nothing and the people
// take the room. A third person turns it to the room's chat for good. The two cross-fade in place.
import { ChatCircleDots } from "@phosphor-icons/react";
import { ContentIcon, cn } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useId, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useComposerNotice, useComposerState, usePeople } from "@/features/social/chat/chat-page";
import { Conversation, useUnreadAtOpen } from "@/features/social/chat/conversation";
import { crewConversation } from "@/features/social/chat/conversations";
import { messageLine } from "@/features/social/chat/preview";
import { enter, leave } from "@/features/social/motion";
import { useSocialUi } from "@/features/social/store";
import type { ChatMessage } from "@/lib/api/social-types";
import type { VoiceRoomView } from "@/lib/api/voice-types";
import { usePanelChrome } from "../panel-chrome";
import { useVoiceUi } from "../store";
import { useFieldCue } from "./field-cue";
import { RoomChat } from "./room-chat";
import { type ChatChoiceInput, chatChoice, type PanelChat } from "./rules";
import { useRoomChat } from "./store";

const NONE: PanelChat = { kind: "none" };

/** The room has its own chat (the launcher says so; the browser preview with `rchat=`). */
export function roomChatOn(room: VoiceRoomView | null): boolean {
  return room?.chat === true;
}

/** The panel's chat now, read outside React (room chat's host). */
export function choiceNow(): PanelChat {
  const voice = useVoiceUi.getState().state;
  const room = voice.room;
  if (!room) return NONE;
  const view = useSocialUi.getState().view;
  const data = useRoomChat.getState();
  return chatChoice({
    room: { kind: room.kind, people: room.people, chat: roomChatOn(room), grew: room.grew },
    calling: voice.calling,
    me: room.me.id,
    friends: view.friends.map((f) => f.id),
    chatOn: view.chat?.on === true,
    latched: data.room === room.id && data.latched,
  });
}

/** The panel's chat for `room` (none without a room). */
export function usePanelChat(room: VoiceRoomView | null): PanelChat {
  const calling = useVoiceUi((s) => s.state.calling);
  const friendList = useSocialUi((s) => s.view.friends);
  const chatOn = useSocialUi((s) => s.view.chat?.on === true);
  const latched = useRoomChat((s) => Boolean(room) && s.room === room?.id && s.latched);
  const friends = useMemo(() => friendList.map((f) => f.id), [friendList]);
  const kind = room?.kind;
  const people = room?.people;
  const me = room?.me.id;
  const chat = roomChatOn(room);
  const grew = room?.grew;
  return useMemo(() => {
    if (!kind || !people) return NONE;
    const input: ChatChoiceInput = {
      room: { kind, people, chat, grew },
      calling,
      me,
      friends,
      chatOn,
      latched,
    };
    return chatChoice(input);
  }, [kind, people, chat, grew, calling, me, friends, chatOn, latched]);
}

/** A friend's conversation in the room panel: chat's own, read while it shows here. */
function DirectChat({ conv, friend, visible }: { conv: string; friend: string; visible: boolean }) {
  const { t, i18n } = useTranslation();
  const people = usePeople();
  const openReport = useSocialUi((s) => s.openReport);
  const writable = useSocialUi(
    (s) =>
      s.view.chat?.conversations.find((c) => c.id === conv)?.writable ??
      s.view.friends.some((f) => f.id === friend),
  );
  const composer = useComposerState(conv, writable, false);
  const notice = useComposerNotice();
  const unreadAtOpen = useUnreadAtOpen(conv);
  const focusCue = useFieldCue(visible);
  const report = (message: ChatMessage) => {
    if (message.id === undefined) return;
    const sender = people(message.from);
    openReport({
      target: { kind: "message", conv, id: message.id },
      name: sender.name,
      person: sender,
      text: messageLine(message, t, i18n.language),
    });
  };
  return (
    <Conversation
      key={conv}
      conv={conv}
      crew={false}
      people={people}
      composer={composer}
      notice={notice}
      unreadAtOpen={unreadAtOpen}
      onScreen={visible}
      shown={visible}
      focusCue={focusCue}
      onReport={report}
      empty={
        <div className="flex flex-col items-center gap-2 text-center">
          <ContentIcon kind="chat" quiet />
          <p className="text-[13px] text-ash-400">{t("chat.list.noMessages")}</p>
        </div>
      }
    />
  );
}

/** "Ekip Sohbeti": the crew's own chat, in the friends panel (the sheet over the page closes). */
function CrewChatLink({ crew }: { crew: string }) {
  const { t } = useTranslation();
  const { docked, close } = usePanelChrome();
  const known = useSocialUi(
    (s) => s.view.chat?.on === true && s.view.crews.some((c) => c.id === crew),
  );
  if (!known) return null;
  return (
    <button
      type="button"
      onClick={() => {
        if (!docked) close();
        useSocialUi.getState().openChat(crewConversation(crew));
      }}
      className="inline-flex h-7 shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-2 text-[12px] font-medium text-ash-300 outline-none transition-[background-color,color,transform,scale] duration-150 ease-out hover:bg-white/8 hover:text-bone-50 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.97]"
    >
      <ChatCircleDots size={14} aria-hidden />
      {t("roomChat.crewChat")}
    </button>
  );
}

/** The chat section: its quiet title line, then the chat itself (the panel's remaining room). */
export function PanelChatSection({
  room,
  chat,
  centered = false,
}: {
  room: VoiceRoomView;
  chat: Exclude<PanelChat, { kind: "none" }>;
  /** SQOL-U3: a very wide panel: the chat in the middle of its column, its lines at most 720 px. */
  centered?: boolean;
}) {
  const { t } = useTranslation();
  const { shown } = usePanelChrome();
  // On screen: the panel shows its room page (not "Davet Et" over it), the friends panel not over
  // it (a friend's conversation marks nothing read under it).
  const onRoomPage = useVoiceUi((s) => s.page === "room");
  const friendsOpen = useSocialUi((s) => s.open);
  const visible = onRoomPage && shown && !friendsOpen;
  const labelId = useId();
  return (
    <section
      aria-labelledby={labelId}
      className={cn("flex min-h-0 flex-1 flex-col", centered && "mx-auto w-full max-w-[720px]")}
    >
      <div className="flex h-9 shrink-0 items-center gap-2 pr-3 pl-5">
        <h3
          id={labelId}
          className="min-w-0 flex-1 truncate text-[12.5px] font-semibold text-ash-300"
        >
          {chat.kind === "dm" ? t("roomChat.dm") : t("roomChat.title")}
        </h3>
        {chat.kind === "room" && room.kind === "crew" && room.crew ? (
          <CrewChatLink crew={room.crew} />
        ) : null}
      </div>
      <div className="relative flex min-h-0 flex-1 flex-col px-3">
        <AnimatePresence initial={false} mode="popLayout">
          <motion.div
            key={chat.kind === "dm" ? chat.conv : "room"}
            className="flex min-h-0 flex-1 flex-col"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.22) }}
            exit={{ opacity: 0, transition: leave(0.14) }}
          >
            {chat.kind === "dm" ? (
              <DirectChat conv={chat.conv} friend={chat.friend} visible={visible} />
            ) : (
              <RoomChat room={room} visible={visible} />
            )}
          </motion.div>
        </AnimatePresence>
      </div>
    </section>
  );
}
