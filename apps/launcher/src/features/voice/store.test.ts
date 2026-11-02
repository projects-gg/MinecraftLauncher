import { afterEach, describe, expect, it, vi } from "vitest";
import { createVoiceMock } from "@/lib/api/voice-mock";
import { HIDDEN_VOICE_STATE } from "@/lib/api/voice-types";
import { useVoiceUi } from "./store";

afterEach(() => {
  useVoiceUi.setState({
    state: HIDDEN_VOICE_STATE,
    speaking: [],
    meTalking: false,
    panel: false,
    page: "room",
    ended: null,
  });
});

describe("the voice store", () => {
  it("renders nothing again for a speaking frame that changes nothing", () => {
    const seen = vi.fn();
    const stop = useVoiceUi.subscribe(seen);
    useVoiceUi.getState().setSpeaking({ ids: ["302", "301"], me: false });
    useVoiceUi.getState().setSpeaking({ ids: ["301", "302"], me: false });
    useVoiceUi.getState().setSpeaking({ ids: ["301", "302"], me: true });
    stop();
    expect(seen).toHaveBeenCalledTimes(2);
    expect(useVoiceUi.getState().speaking).toEqual(["301", "302"]);
  });

  it("closes the panel with the room and clears who spoke", () => {
    const room = createVoiceMock({ scenario: "room", timers: false }).current();
    useVoiceUi.getState().setState(room);
    useVoiceUi.setState({ panel: true, page: "invite", speaking: ["301"] });
    useVoiceUi.getState().setState({ ...room, room: null });
    const s = useVoiceUi.getState();
    expect([s.panel, s.page, s.speaking]).toEqual([false, "room", []]);
  });

  it("drops what the pill kept of the last stay once a new one begins", () => {
    useVoiceUi.getState().setEnded({ kind: "noAnswer", friend: "302" });
    useVoiceUi.getState().setState(createVoiceMock({ scenario: "ready", timers: false }).current());
    expect(useVoiceUi.getState().ended).not.toBeNull();
    useVoiceUi
      .getState()
      .setState(createVoiceMock({ scenario: "calling", timers: false }).current());
    expect(useVoiceUi.getState().ended).toBeNull();
  });
});

describe("the preview's voice chat", () => {
  it("calls: the player waits in the room while it rings", async () => {
    const mock = createVoiceMock({ timers: false });
    expect(await mock.call("302")).toEqual({ refused: null, retryAfter: null, until: null });
    const state = mock.current();
    expect(state.calling?.friend).toBe("302");
    expect(state.room?.people).toEqual([]);
    expect((await mock.call("306")).refused).toBe("unreachable");
  });

  it("invites ring until they join or are taken back; a full room refuses", async () => {
    const mock = createVoiceMock({ scenario: "room", timers: false });
    await mock.invite("302");
    expect(mock.current().room?.ringing.map((r) => r.to)).toEqual(["305", "302"]);
    await mock.cancelInvite("302");
    expect(mock.current().room?.ringing.map((r) => r.to)).toEqual(["305"]);
    const full = createVoiceMock({ scenario: "full", timers: false });
    expect((await full.invite("302")).refused).toBe("roomFull");
  });

  it("removes someone and lets them back with the toast's undo", async () => {
    const mock = createVoiceMock({ scenario: "room", timers: false });
    await mock.moderate({ act: "kick", person: "303" });
    expect(mock.current().room?.people.map((p) => p.id)).toEqual(["301", "909"]);
    await mock.moderate({ act: "unkick", person: "303" });
    expect(mock.current().room?.people.map((p) => p.id)).toEqual(["301", "909", "303"]);
  });

  it("deafening closes the microphone; a rename keeps 3 to 24 characters", async () => {
    const mock = createVoiceMock({ scenario: "room", timers: false });
    await mock.setDeaf(true);
    expect(mock.current().room?.me).toMatchObject({ deaf: true, mute: true });
    expect((await mock.moderate({ act: "rename", name: "ab" })).refused).toBe("invalidName");
    expect((await mock.moderate({ act: "rename", name: "Gece Ekibi" })).refused).toBeNull();
    expect(mock.current().room?.name).toBe("Gece Ekibi");
  });

  it("says which keys clash, as the launcher would", async () => {
    const mock = createVoiceMock({ timers: false });
    const check = (code: string) =>
      mock.checkKey({ kind: "keyboard", code, label: code }, "pushToTalk");
    expect((await check("CapsLock")).clash).toBe("gameVoiceChat");
    expect((await check("F7")).clash).toBe("overlay");
    expect((await check("KeyW")).clash).toBe("game");
    expect((await check("AltLeft")).clash).toBe("languageSwitch");
    expect((await check("KeyB")).clash).toBeNull();
    expect((await check("Escape")).usable).toBe(false);
  });
});
