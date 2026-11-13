// The voice room panel's chat (SQOL-U3, sqol.md §1.3), in English: the same places as
// tr-room-chat.ts. Everything else is chat's own words (`chat.*`, `chat2.*`).
import type { trRoomChat } from "./tr-room-chat";

export const enRoomChat: typeof trRoomChat = {
  panel: {
    label: "Voice Room",
    menu: "Room Options",
  },
  resize: {
    label: "Panel Width",
    tooltip: "Drag to Widen",
    value: {
      one: "{{percent}}, one column",
      two: "{{percent}}, two columns",
    },
  },
  people: {
    title: "In the Room",
    label_one: "In the room, {{count}} person",
    label_other: "In the room, {{count}} people",
  },
  title: "Room Chat",
  dm: "Messages",
  crewChat: "Crew Chat",
  empty: "Only people in this room see what you write here, and it's deleted when the room closes.",
  composer: {
    placeholder: "Message the room",
    label: "Message the room",
  },
  tombstone: {
    mod: "This message was removed",
  },
  refused: {
    notAllowed: "You can't delete this message.",
  },
  speaking: "speaking",
  connection: {
    good: "Connection: Good",
    weak: "Connection: Fair",
    bad: "Connection: Poor",
  },
  announce: {
    one: "Room chat, {{name}}: {{text}}",
    many_one: "{{count}} new message in room chat",
    many_other: "{{count}} new messages in room chat",
  },
  settings: {
    peek: "Show Message Previews",
  },
};
