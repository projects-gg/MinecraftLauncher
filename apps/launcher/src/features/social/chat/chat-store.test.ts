import { afterEach, describe, expect, it } from "vitest";
import type { ChatMessage, ChatReaction } from "@/lib/api/social-types";
import { useChat } from "./chat-store";
import { onlineIn, useTyping } from "./typing";

const CONV = "d:100:301";
const writing = () => Object.keys(useTyping.getState().writers[CONV] ?? {});

function message(key: string, at: number, extra: Partial<ChatMessage> = {}): ChatMessage {
  return {
    key,
    id: at,
    from: "301",
    mine: false,
    text: "selam",
    at,
    edited: false,
    state: "sent",
    ...extra,
  };
}

function loaded(messages: ChatMessage[]) {
  useChat.setState({
    threads: {
      [CONV]: { messages, more: false, loaded: true, loading: false, stale: false, older: false },
    },
  });
}

const upsert = (...messages: ChatMessage[]) =>
  useChat.getState().apply({ conv: CONV, upsert: messages, remove: [], reset: false });

describe("the chat store's extras", () => {
  afterEach(() => useChat.getState().forget());

  it("ends someone's 'yazıyor…' with a new message of theirs, not a change to an old one", () => {
    const old = message("k1", Date.now() - 120_000);
    loaded([old]);
    useTyping.getState().saw(CONV, "301");
    upsert({ ...old, reactions: [{ e: "👍", by: ["100"], mine: true }] });
    expect(writing()).toEqual(["301"]);
    upsert(message("k2", Date.now()));
    expect(writing()).toEqual([]);
  });

  it("tells a new message by its age in a thread not opened here", () => {
    useTyping.getState().saw(CONV, "301");
    upsert(message("k1", Date.now() - 120_000));
    expect(writing()).toEqual(["301"]);
    upsert(message("k2", Date.now()));
    expect(writing()).toEqual([]);
  });

  it("keeps the player's reactions on their way until every change is answered", () => {
    const shown: ChatReaction[] = [{ e: "🔥", by: ["100"], mine: true }];
    const target = message("k1", Date.now() - 60_000);
    loaded([target]);
    const { startReacting, endReacting } = useChat.getState();
    startReacting("k1", shown);
    startReacting("k1", shown);
    upsert({ ...target, reactions: [] });
    expect(useChat.getState().reacting.k1).toEqual(shown);
    endReacting("k1");
    expect(useChat.getState().reacting.k1).toEqual(shown);
    endReacting("k1");
    expect(useChat.getState().reacting.k1).toBeUndefined();
  });
});

describe("onlineIn", () => {
  it("reads friends and crewmates, never the player", () => {
    const online = onlineIn(
      [
        { id: "301", state: "online" },
        { id: "302", state: "offline" },
      ],
      [
        {
          members: [
            { id: "100", me: true, state: "online" },
            { id: "701", state: "playing" },
          ],
        },
      ],
    );
    expect(["301", "302", "701", "100"].map(online)).toEqual([true, false, true, false]);
  });
});
