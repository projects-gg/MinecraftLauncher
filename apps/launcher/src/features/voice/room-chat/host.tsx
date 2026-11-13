// The room chat while the player is in a room (SQOL §1.3), mounted with voice chat's host: the
// room's last messages when the player comes in (and again after a reconnect: the `log` event),
// the room's messages and deletions as they come (events only; nothing asks on a timer), what is
// unread while the chat is out of sight, the previews over the stage, and what a screen reader
// hears: in sight, the new messages gathered a moment ("KorAtesi: selam", "3 yeni mesaj"); out of
// sight, with the previews on, gathered longer and saying where ("Oda sohbetinde 3 yeni mesaj").
// Nothing while the window is away. People the player blocks are never heard or counted. A room
// that ends takes its messages with it.
import { useEffect, useRef, useState } from "react";
import { announce } from "@/features/accounts/use-accounts";
import { messageLine } from "@/features/social/chat/preview";
import { useSocialUi } from "@/features/social/store";
import i18n from "@/lib/i18n";
import { isWindowAway, useWindowAway } from "@/lib/window-away";
import { useStageMode } from "../stage-mode";
import { useVoiceUi } from "../store";
import { roomChatApi } from "./api";
import { burstWords, createGather, type Heard, HIDDEN_GATHER_MS, SHOWN_GATHER_MS } from "./calm";
import { mockRefused } from "./mock";
import { choiceNow, roomChatOn, usePanelChat } from "./panel-chat";
import { personNow } from "./people";
import { shortLine } from "./rules";
import { usePeekSetting } from "./settings";
import {
  forRoom,
  markDeleted,
  type RoomChatData,
  receive,
  receiveLog,
  setShown,
  settleSend,
  startSend,
  useRoomChat,
} from "./store";

const meNow = () => useVoiceUi.getState().state.room?.me.id;

/** The browser preview's refused messages (`rchat=refusals`), as if the player had sent them. */
function withMockRefusals(data: RoomChatData): RoomChatData {
  const me = meNow();
  if (!me) return data;
  let next = data;
  mockRefused().forEach((one, index) => {
    const nonce = `mock${index}`;
    next = startSend(next, nonce, { text: one.text }, me, Date.now() - (3 - index) * 20_000);
    next = settleSend(next, nonce, { refused: one.refusal });
  });
  return next;
}

export function RoomChatSync() {
  const room = useVoiceUi((s) => s.state.room);
  const roomId = room?.id ?? null;
  const chatOn = roomChatOn(room);
  const panelOnRoom = useVoiceUi((s) => s.panel && s.page === "room");
  const stage = useStageMode();
  const choice = usePanelChat(room);
  const away = useWindowAway();
  const peekOn = usePeekSetting();
  const apply = useRoomChat((s) => s.apply);

  // A room of its own: the last one's messages go.
  useEffect(() => apply((d) => forRoom(d, roomId)), [apply, roomId]);

  // A third person: the panel keeps the room's chat for this stay (SQOL §1.3).
  const many = (room?.people.length ?? 0) > 1;
  useEffect(() => {
    if (many) apply((d) => (d.room === roomId && !d.latched ? { ...d, latched: true } : d));
  }, [apply, many, roomId]);

  // The room's last messages when the player comes in.
  useEffect(() => {
    if (!roomId || !chatOn) return;
    let alive = true;
    void roomChatApi.log(roomId).then(
      (msgs) => {
        if (!alive) return;
        apply((d) => (d.room === roomId ? withMockRefusals(receiveLog(d, msgs, meNow())) : d));
      },
      () => {
        if (alive) apply((d) => (d.room === roomId ? { ...d, loaded: true } : d));
      },
    );
    return () => {
      alive = false;
    };
  }, [apply, roomId, chatOn]);

  // On screen: the panel open on the room, the stage not over everything, the room chat its chat.
  const shown = panelOnRoom && stage !== "full" && choice.kind === "room";
  const [gather] = useState(() =>
    createGather<Heard>((heard) => {
      if (heard.length && !isWindowAway()) announce(burstWords(heard, now.current.shown, i18n.t));
    }),
  );
  useEffect(() => {
    apply((d) => setShown(d, shown));
    gather.clear();
  }, [apply, gather, shown]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new room, nothing of the last is told
  useEffect(() => gather.clear(), [gather, roomId]);
  useEffect(() => () => gather.clear(), [gather]);

  // What a message that comes now does (read by the event listener below).
  const now = useRef({ shown, peek: false, peekOn, choice: choice.kind, away });
  now.current = {
    shown,
    // The previews: over the stage (the page's or the full one), the panel out of sight.
    peek: !shown && peekOn && !away && (stage === "page" || stage === "full"),
    peekOn,
    choice: choice.kind,
    away,
  };

  useEffect(
    () =>
      roomChatApi.onEvent((event) => {
        const data = useRoomChat.getState();
        if (event.room !== data.room) return;
        if (event.kind === "log") {
          data.apply((d) => receiveLog(d, event.msgs, meNow()));
          return;
        }
        if (event.kind === "del") {
          data.apply((d) => markDeleted(d, event.id, event.by));
          return;
        }
        const { msg } = event;
        if (useSocialUi.getState().view.blocked.some((p) => p.id === msg.from)) return;
        const me = meNow();
        const theirs = msg.from !== me;
        // Someone writes in the room while the friend's conversation shows: the room's chat now.
        if (theirs && choiceNow().kind === "dm") {
          data.apply((d) => (d.latched ? d : { ...d, latched: true }));
        }
        const person = personNow(msg.from);
        data.apply((d) =>
          receive(d, msg, {
            me,
            peek: now.current.peek,
            nowMs: Date.now(),
            ...(person ? { person } : {}),
          }),
        );
        const told = now.current;
        if (!theirs || told.away || (!told.shown && !told.peekOn)) return;
        gather.add(
          {
            name: person?.name ?? i18n.t("chat.list.someone"),
            line: shortLine(messageLine(msg, i18n.t, i18n.language)),
          },
          told.shown ? SHOWN_GATHER_MS : HIDDEN_GATHER_MS,
        );
      }),
    [gather],
  );
  return null;
}
