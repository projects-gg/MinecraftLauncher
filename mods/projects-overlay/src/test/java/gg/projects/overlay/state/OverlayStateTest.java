package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import org.junit.jupiter.api.Test;

class OverlayStateTest {
    private final OverlayState state = new OverlayState();

    private OverlayState.ToastRequest apply(String json) {
        return state.apply(Protocol.parse(json), 1_000);
    }

    @Test
    void snapshotReplacesFriendsConversationsAndCalls() {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Ece\"},\"reducedMotion\":true}");
        apply("{\"t\":\"snapshot\",\"friends\":[{\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"projects\","
            + "\"joinable\":true}],\"convs\":[{\"id\":\"dm:1\",\"title\":\"Ayşe\",\"unread\":2}],"
            + "\"calls\":[{\"id\":\"c1\",\"crew\":\"Gece\",\"from\":\"Mert\",\"answers\":{\"coming\":1}}]}");
        assertEquals("Ece", state.me());
        assertTrue(state.reducedMotion());
        assertEquals(1, state.friends().size());
        assertTrue(state.friends().iterator().next().joinable());
        assertEquals(2, state.conversation("dm:1").unread());
        assertEquals(1, state.calls().iterator().next().coming());
        apply("{\"t\":\"snapshot\",\"friends\":[],\"convs\":[],\"calls\":[]}");
        assertTrue(state.friends().isEmpty());
        assertNull(state.conversation("dm:1"));
    }

    @Test
    void ownMessageGoesFromPendingToSent() {
        String cid = state.addPending("dm:1", "selam", 1_000);
        OverlayState.ChatLine line = state.lines("dm:1").getLast();
        assertEquals(OverlayState.Delivery.PENDING, line.deliveryAt(2_000));
        apply("{\"t\":\"sent\",\"cid\":\"" + cid + "\",\"id\":\"m9\"}");
        assertEquals(OverlayState.Delivery.SENT, line.deliveryAt(2_000));
        assertEquals("m9", line.id);
    }

    @Test
    void unconfirmedMessageShowsAsFailedAfterTheTimeout() {
        state.addPending("dm:1", "selam", 1_000);
        OverlayState.ChatLine line = state.lines("dm:1").getLast();
        assertEquals(OverlayState.Delivery.FAILED, line.deliveryAt(1_000 + OverlayState.SEND_TIMEOUT_MS + 1));
    }

    @Test
    void launcherRefusalMarksTheMessage() {
        String cid = state.addPending("dm:1", "bak site.gg", 1_000);
        apply("{\"t\":\"send-failed\",\"cid\":\"" + cid + "\",\"reason\":\"link\"}");
        OverlayState.ChatLine line = state.lines("dm:1").getLast();
        assertEquals(OverlayState.Delivery.FAILED, line.delivery);
        assertEquals("link", line.failure);
    }

    @Test
    void historyKeepsPendingLinesAndDropsDuplicates() {
        state.addPending("dm:1", "bekleyen", 1_000);
        apply("{\"t\":\"history\",\"conv\":\"dm:1\",\"msgs\":[{\"id\":\"m1\",\"from\":\"Ayşe\",\"text\":\"a\"},"
            + "{\"id\":\"m1\",\"from\":\"Ayşe\",\"text\":\"a\"}]}");
        assertEquals(2, state.lines("dm:1").size());
    }

    @Test
    void keepsOnlyTheNewestLines() {
        for (int i = 0; i < OverlayState.MAX_LINES + 20; i++) {
            apply("{\"t\":\"msg\",\"conv\":\"c\",\"id\":\"m" + i + "\",\"from\":\"A\",\"text\":\"" + i + "\"}");
        }
        assertEquals(OverlayState.MAX_LINES, state.lines("c").size());
        assertEquals("119", state.lines("c").getLast().text);
    }

    @Test
    void toastTextIsPlainAndBounded() {
        String section = new String(Character.toChars(0xA7));
        OverlayState.ToastRequest t = apply("{\"t\":\"toast\",\"kind\":\"call\",\"key\":\"call:c1\",\"title\":\""
            + section + "kGece\",\"body\":\"" + "ç".repeat(400) + "\"}");
        assertNotNull(t);
        assertEquals("call", t.kind());
        assertEquals("kGece", t.title());
        assertEquals(Protocol.MAX_MESSAGE_CHARS, t.body().length());
    }

    @Test
    void unknownFramesChangeNothing() {
        int before = state.revision();
        assertNull(apply("{\"t\":\"something-new\",\"x\":1}"));
        assertEquals(before, state.revision());
    }

    @Test
    void callAnswersAndEnd() {
        apply("{\"t\":\"call\",\"id\":\"c1\",\"crew\":\"Gece\",\"from\":\"Mert\",\"answers\":{\"coming\":2,"
            + "\"soon\":1,\"cant\":0},\"mine\":\"soon\"}");
        OverlayState.CrewCall call = state.calls().iterator().next();
        assertEquals(2, call.coming());
        assertEquals("soon", call.mine());
        apply("{\"t\":\"call-end\",\"id\":\"c1\"}");
        assertTrue(state.calls().isEmpty());
    }
}
