package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import java.util.List;
import org.junit.jupiter.api.Test;

/** The frames OVL2 added (docs/tasks/ovl2-protocol.md, L1-L15), as the mod mirrors them. */
class OverlayStateFramesTest {
    private final OverlayState state = new OverlayState();

    private OverlayState.ToastRequest apply(String json) {
        return state.apply(Protocol.parse(json), 1_000);
    }

    private void welcome(String name, String features) {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"" + name + "\"},\"features\":" + features + "}");
    }

    @Test
    void featuresTellWhetherTheLauncherIsSignedIn() {
        welcome("", "[]");
        assertTrue(state.welcomed());
        assertFalse(state.signedIn());
        welcome("Ece", "[\"friends\",\"chat\",\"calls\"]");
        assertTrue(state.signedIn());
        assertTrue(state.has("chat"));
        assertFalse(state.has("something"));
    }

    @Test
    void anotherAccountClearsWhatTheLastOneHad() {
        welcome("Ece", "[\"friends\",\"chat\"]");
        apply("{\"t\":\"snapshot\",\"friends\":[{\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"projects\"}],"
            + "\"convs\":[{\"id\":\"d:1\",\"title\":\"Ayşe\"}],\"calls\":[]}");
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"m1\",\"from\":\"Ayşe\",\"text\":\"selam\"}");
        welcome("Mert", "[\"friends\",\"chat\"]");
        assertTrue(state.friends().isEmpty());
        assertTrue(state.lines("d:1").isEmpty());
        // The same account again keeps everything (a welcome after a reduced-motion change).
        apply("{\"t\":\"msg\",\"conv\":\"d:2\",\"id\":\"m2\",\"from\":\"Can\",\"text\":\"hey\"}");
        welcome("Mert", "[\"friends\"]");
        assertEquals(1, state.lines("d:2").size());
        assertFalse(state.has("chat"));
    }

    @Test
    void friendsCarryWhereToJoin() {
        apply("{\"t\":\"friend\",\"id\":\"f2\",\"name\":\"Elif\",\"where\":\"server\",\"joinable\":true,"
            + "\"host\":\"mc.ornek.net\"}");
        apply("{\"t\":\"friend\",\"id\":\"f3\",\"name\":\"Deniz\",\"where\":\"game\"}");
        OverlayState.Friend elif = state.friend("f2");
        assertEquals("mc.ornek.net", elif.host());
        assertTrue(elif.joinable() && elif.inGame() && elif.online());
        assertTrue(state.friend("f3").inGame());
        assertNull(state.friend("f3").host());
    }

    @Test
    void conversationsKnowWhetherThePlayerMayWrite() {
        apply("{\"t\":\"conv\",\"id\":\"d:1\",\"kind\":\"dm\",\"title\":\"Can\",\"writable\":false}");
        apply("{\"t\":\"conv\",\"id\":\"c:1\",\"kind\":\"crew\",\"title\":\"Gece\"}");
        assertFalse(state.conversation("d:1").writable());
        assertTrue(state.conversation("c:1").writable());
        assertTrue(state.conversation("c:1").crew());
    }

    @Test
    void newActivityPutsAConversationFirst() {
        apply("{\"t\":\"snapshot\",\"friends\":[],\"convs\":[{\"id\":\"a\",\"title\":\"A\"},{\"id\":\"b\","
            + "\"title\":\"B\"}],\"calls\":[]}");
        apply("{\"t\":\"msg\",\"conv\":\"b\",\"id\":\"m1\",\"from\":\"B\",\"text\":\"hi\"}");
        assertEquals(List.of("b", "a"), state.conversations().stream().map(OverlayState.Conversation::id).toList());
        apply("{\"t\":\"conv\",\"id\":\"c\",\"title\":\"C\"}");
        assertEquals("c", state.conversations().iterator().next().id());
    }

    @Test
    void anEditReplacesTheTextInPlace() {
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"m1\",\"from\":\"Ayşe\",\"text\":\"21:00\"}");
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"m2\",\"from\":\"Ayşe\",\"text\":\"geliyor musun\"}");
        int before = state.lineRevision("d:1");
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"m1\",\"from\":\"Ayşe\",\"text\":\"21:30\",\"edited\":true}");
        OverlayState.ChatLine first = state.lines("d:1").getFirst();
        assertEquals("21:30", first.text);
        assertTrue(first.edited);
        assertEquals(2, state.lines("d:1").size());
        assertTrue(state.lineRevision("d:1") > before);
    }

    @Test
    void aDeletedMessageLeavesAQuietMark() {
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"m1\",\"from\":\"Ayşe\",\"text\":\"pardon\"}");
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"m1\",\"from\":\"Ayşe\",\"text\":\"\",\"deleted\":true}");
        OverlayState.ChatLine line = state.lines("d:1").getFirst();
        assertTrue(line.deleted);
        assertEquals("", line.text);
        // A deletion of a message never shown adds nothing; in a history it is a mark too.
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"m7\",\"from\":\"Ayşe\",\"deleted\":true}");
        assertEquals(1, state.lines("d:1").size());
        apply("{\"t\":\"history\",\"conv\":\"d:2\",\"msgs\":[{\"id\":\"m8\",\"from\":\"Can\",\"deleted\":true}]}");
        assertTrue(state.lines("d:2").getFirst().deleted);
    }

    @Test
    void aDroppedLineLeavesTheList() {
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"m1\",\"from\":\"Ayşe\",\"text\":\"eski\"}");
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"m2\",\"from\":\"Ayşe\",\"text\":\"yeni\"}");
        apply("{\"t\":\"msg-gone\",\"conv\":\"d:1\",\"id\":\"m1\"}");
        assertEquals(1, state.lines("d:1").size());
        assertEquals("yeni", state.lines("d:1").getFirst().text);
        // An unknown message changes nothing.
        int revision = state.lineRevision("d:1");
        apply("{\"t\":\"msg-gone\",\"conv\":\"d:1\",\"id\":\"m9\"}");
        assertEquals(revision, state.lineRevision("d:1"));
    }

    @Test
    void anEmptyHistoryClearsAllButWhatIsStillGoing() {
        apply("{\"t\":\"msg\",\"conv\":\"d:1\",\"id\":\"m1\",\"from\":\"Ayşe\",\"text\":\"a\"}");
        state.addPending("d:1", "giden", 1_000);
        apply("{\"t\":\"history\",\"conv\":\"d:1\",\"msgs\":[]}");
        assertEquals(1, state.lines("d:1").size());
        assertEquals("giden", state.lines("d:1").getFirst().text);
    }

    @Test
    void callsCarryTheirEndAndWhereTheCallerIs() {
        apply("{\"t\":\"call\",\"id\":\"k1\",\"crew\":\"Gece\",\"from\":\"Mert\",\"since\":1000,\"until\":5000,"
            + "\"answers\":{\"coming\":1},\"where\":\"server\",\"host\":\"mc.ornek.net\"}");
        apply("{\"t\":\"call\",\"id\":\"k2\",\"crew\":\"Gece\",\"from\":\"Ben\",\"until\":9000,\"own\":true}");
        OverlayState.CrewCall call = state.activeCall(2_000);
        assertNotNull(call);
        assertEquals("k1", call.id());
        assertEquals("server", call.where());
        assertEquals("mc.ornek.net", call.host());
        // The player's own call is never one to answer; an ended one is gone without a frame.
        assertNull(state.activeCall(6_000));
    }

    @Test
    void refusedMessagesKeepTheirReasonAndCanBeTakenBack() {
        String cid = state.addPending("d:1", "site.gg", 1_000);
        apply("{\"t\":\"send-failed\",\"cid\":\"" + cid + "\",\"reason\":\"daily-limit\"}");
        OverlayState.ChatLine line = state.lines("d:1").getFirst();
        assertEquals(OverlayState.Delivery.FAILED, line.deliveryAt(1_000));
        assertEquals("daily-limit", line.failureAt(1_000));
        OverlayState.ChatLine taken = state.takeFailed(cid, 1_000);
        assertNotNull(taken);
        assertEquals("site.gg", taken.text);
        assertTrue(state.lines("d:1").isEmpty());
    }

    @Test
    void aMessageNobodyAnsweredTimesOut() {
        String cid = state.addPending("d:1", "hey", 1_000);
        long late = 1_000 + OverlayState.SEND_TIMEOUT_MS + 1;
        assertEquals(OverlayState.TIMEOUT, state.lines("d:1").getFirst().failureAt(late));
        assertNotNull(state.takeFailed(cid, late));
        // A message still on its way cannot be taken.
        String other = state.addPending("d:1", "yo", late);
        assertNull(state.takeFailed(other, late));
    }

    @Test
    void crewChatToastsNameTheirCrew() {
        OverlayState.ToastRequest t = apply("{\"t\":\"toast\",\"kind\":\"message\",\"key\":\"c:1\",\"title\":\"Mert\","
            + "\"body\":\"gel\",\"conv\":\"c:1\",\"crew\":\"Gece Kuşları\"}");
        assertEquals("Gece Kuşları", t.crew());
        assertEquals("c:1", t.conversation());
    }
}
