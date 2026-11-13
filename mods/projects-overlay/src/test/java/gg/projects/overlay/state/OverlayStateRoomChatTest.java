package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/** SQOL-O: the voice room's own chat as the mod mirrors it (docs/tasks/ovl2-protocol.md, rows SQ). */
class OverlayStateRoomChatTest {
    private static final String ROOM = "v:f0123456789abcdef";
    private final OverlayState state = new OverlayState();

    private OverlayState.ToastRequest apply(String json) {
        return state.apply(Protocol.parse(json), 1_000);
    }

    private List<String> ids() {
        List<String> out = new ArrayList<>();
        for (OverlayState.Conversation c : state.conversations()) {
            out.add(c.id());
        }
        return out;
    }

    private void greet() {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"KorAtesi\"},\"features\":[\"friends\",\"chat\"]}");
        apply("{\"t\":\"snapshot\",\"friends\":[],\"convs\":[{\"id\":\"d:1:2\",\"kind\":\"dm\",\"title\":\"x7Ender\","
            + "\"unread\":2,\"muted\":false,\"writable\":true}],\"calls\":[],\"dnd\":false}");
    }

    @Test
    void theRoomsChatIsTheFirstChipWhileItShows() {
        greet();
        assertEquals(List.of("d:1:2"), ids());
        apply("{\"t\":\"room-chat\",\"conv\":\"" + ROOM + "\",\"unread\":3}");
        assertEquals(List.of(ROOM, "d:1:2"), ids());
        OverlayState.Conversation room = state.conversation(ROOM);
        assertTrue(room.room() && room.writable() && !room.crew());
        assertEquals(3, room.unread());
        assertEquals(5, state.unreadTotal(), "the room's count joins the chats'");
        assertEquals(3, state.roomUnread(ROOM));
        assertEquals(0, state.roomUnread("d:1:2"));
        // The launcher's lists never carry it: a snapshot keeps its lines.
        apply("{\"t\":\"msg\",\"conv\":\"" + ROOM + "\",\"id\":\"7\",\"from\":\"Cem\",\"by\":\"u-cem\",\"text\":\"selam\","
            + "\"at\":5}");
        greet();
        assertEquals(1, state.lines(ROOM).size());
        assertEquals(List.of(ROOM, "d:1:2"), ids());
    }

    @Test
    void aMessageTakenAwaySaysByWhom() {
        greet();
        apply("{\"t\":\"room-chat\",\"conv\":\"" + ROOM + "\"}");
        apply("{\"t\":\"history\",\"conv\":\"" + ROOM + "\",\"msgs\":[{\"id\":\"1\",\"from\":\"Cem\",\"text\":\"a\","
            + "\"at\":1},{\"id\":\"2\",\"from\":\"x7Ender\",\"text\":\"\",\"at\":2,\"deleted\":true,\"removed\":true}]}");
        assertEquals(2, state.lines(ROOM).size());
        assertTrue(state.line(ROOM, "2").removed);
        // Its sender's own deletion is the chat's own mark.
        apply("{\"t\":\"msg\",\"conv\":\"" + ROOM + "\",\"id\":\"1\",\"from\":\"Cem\",\"text\":\"\",\"at\":1,"
            + "\"deleted\":true}");
        OverlayState.ChatLine own = state.line(ROOM, "1");
        assertTrue(own.deleted && !own.removed);
    }

    @Test
    void theChipGoesWithItsLinesAndNothingStaysOfAnotherRoom() {
        greet();
        apply("{\"t\":\"room-chat\",\"conv\":\"" + ROOM + "\",\"unread\":1}");
        apply("{\"t\":\"msg\",\"conv\":\"" + ROOM + "\",\"id\":\"7\",\"from\":\"Cem\",\"text\":\"selam\",\"at\":5}");
        // Another room's messages (late, or a confused launcher) are kept nowhere.
        apply("{\"t\":\"msg\",\"conv\":\"v:other\",\"id\":\"1\",\"from\":\"Cem\",\"text\":\"x\",\"at\":5}");
        assertTrue(state.lines("v:other").isEmpty());
        apply("{\"t\":\"room-chat\"}");
        assertNull(state.conversation(ROOM));
        assertTrue(state.lines(ROOM).isEmpty());
        assertEquals(List.of("d:1:2"), ids());
        assertEquals(2, state.unreadTotal());
        // A list's conversation is never taken for the room's.
        apply("{\"t\":\"conv\",\"id\":\"v:sneaky\",\"kind\":\"dm\",\"title\":\"?\",\"unread\":0,\"muted\":false,"
            + "\"writable\":true}");
        assertNull(state.conversation("v:sneaky"));
        // Without the launcher nothing of it stays.
        apply("{\"t\":\"room-chat\",\"conv\":\"" + ROOM + "\"}");
        state.setStatus(BridgeClient.Status.CONNECTING, null);
        assertNull(state.roomConversation());
        assertFalse(state.conversations().stream().anyMatch(OverlayState.Conversation::room));
    }

    @Test
    void theRequestsAreTheLaunchers() {
        assertEquals("{\"t\":\"room-delete\",\"conv\":\"" + ROOM + "\",\"id\":\"7\"}",
            Protocol.roomDelete(ROOM, "7").toString());
        assertEquals("{\"t\":\"room-report\",\"conv\":\"" + ROOM + "\",\"id\":\"7\",\"reason\":\"spam\",\"note\":\"bak\"}",
            Protocol.roomReport(ROOM, "7", "spam", "bak").toString());
        assertEquals("{\"t\":\"room-report\",\"conv\":\"" + ROOM + "\",\"id\":\"7\",\"reason\":\"other\"}",
            Protocol.roomReport(ROOM, "7", "other", "").toString(), "no empty note");
        assertTrue(Protocol.roomConversation(ROOM));
        assertFalse(Protocol.roomConversation("v:"));
        assertFalse(Protocol.roomConversation("d:1:2"));
        assertFalse(Protocol.roomConversation(null));
        assertTrue(Protocol.hello("t", "1", "26.1.2", "tr_tr").toString().contains("\"rchat\""));
        assertEquals(List.of("harassment", "inappropriate", "spam", "scam", "other"), Protocol.MESSAGE_REPORT_REASONS);
    }
}
