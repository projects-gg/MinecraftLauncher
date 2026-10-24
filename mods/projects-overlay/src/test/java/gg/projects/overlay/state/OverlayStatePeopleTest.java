package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/** The friend menu's fields and frames (docs/tasks/ovl2-protocol.md, OVL-UX: P1-P11) as the mod mirrors them. */
class OverlayStatePeopleTest {
    private final OverlayState state = new OverlayState();

    private void apply(String json) {
        state.apply(Protocol.parse(json), 1_000);
    }

    private void apply(String json, long now) {
        state.apply(Protocol.parse(json), now);
    }

    private void welcome(String me, String features) {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":" + me + ",\"features\":" + features + "}");
    }

    private static List<String> ids(Iterable<OverlayState.Conversation> convs) {
        List<String> out = new ArrayList<>();
        convs.forEach(c -> out.add(c.id()));
        return out;
    }

    @Test
    void theModSaysItHasTheFriendMenu() {
        assertTrue(Protocol.hello("t", "0.1.0", "26.1.2", "tr_tr").getAsJsonArray("features").toString()
            .contains("\"people\""));
    }

    @Test
    void friendsCarryTheirConversationAndDiscord() {
        welcome("{\"name\":\"Ece\"}", "[\"friends\",\"chat\",\"people\"]");
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"projects\",\"joinable\":true,"
            + "\"dm\":\"d:f1:me\",\"discord\":true}");
        apply("{\"t\":\"friend\",\"id\":\"f2\",\"name\":\"Mert\",\"where\":\"offline\"}");
        assertEquals("d:f1:me", state.dm("f1"));
        assertTrue(state.discord("f1"));
        assertNull(state.dm("f2"));
        assertFalse(state.discord("f2"));
        // A later frame without them takes them away.
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"projects\",\"joinable\":true}");
        assertNull(state.dm("f1"));
        assertFalse(state.discord("f1"));
        // Chat off (the welcome without it): no conversation opens from here.
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"projects\",\"dm\":\"d:f1:me\"}");
        welcome("{\"name\":\"Ece\"}", "[\"friends\",\"people\"]");
        assertNull(state.dm("f1"));
    }

    @Test
    void aNewConversationShowsAtOnceAndBecomesTheRealOne() {
        welcome("{\"name\":\"Ece\"}", "[\"friends\",\"chat\",\"people\"]");
        apply("{\"t\":\"snapshot\",\"friends\":[{\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"launcher\","
            + "\"dm\":\"d:f1:me\"}],\"convs\":[{\"id\":\"c:t1\",\"kind\":\"crew\",\"title\":\"Gece Kuşları\"}],"
            + "\"calls\":[]}");
        int before = state.revision();
        assertEquals("d:f1:me", state.openDirect("f1"));
        assertTrue(state.revision() != before);
        OverlayState.Conversation draft = state.conversation("d:f1:me");
        assertNotNull(draft);
        assertEquals("Ayşe", draft.title());
        assertTrue(draft.writable() && !draft.crew());
        assertEquals(List.of("d:f1:me", "c:t1"), ids(state.conversations()), "the one just opened first");
        // Opening it again changes nothing.
        before = state.revision();
        assertEquals("d:f1:me", state.openDirect("f1"));
        assertEquals(before, state.revision());
        // A new snapshot (a server switch greets again) keeps it while it has no message.
        apply("{\"t\":\"snapshot\",\"friends\":[{\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"launcher\","
            + "\"dm\":\"d:f1:me\"}],\"convs\":[{\"id\":\"c:t1\",\"kind\":\"crew\",\"title\":\"Gece Kuşları\"}],"
            + "\"calls\":[]}");
        assertNotNull(state.conversation("d:f1:me"));
        // Its first message makes it real: the launcher's conversation takes its place.
        apply("{\"t\":\"conv\",\"id\":\"d:f1:me\",\"kind\":\"dm\",\"title\":\"Ayşe\",\"unread\":0,\"writable\":true}");
        assertEquals(List.of("d:f1:me", "c:t1"), ids(state.conversations()));
        // No longer a friend: nothing to open, and an empty one goes.
        apply("{\"t\":\"friend\",\"id\":\"f2\",\"name\":\"Mert\",\"where\":\"offline\",\"dm\":\"d:f2:me\"}");
        assertEquals("d:f2:me", state.openDirect("f2"));
        apply("{\"t\":\"friend-gone\",\"id\":\"f2\"}");
        assertNull(state.conversation("d:f2:me"));
        assertFalse(ids(state.conversations()).contains("d:f2:me"));
        assertNull(state.openDirect("f2"));
        assertNull(state.openDirect(null));
    }

    @Test
    void anotherAccountForgetsTheMenusTraces() {
        welcome("{\"name\":\"Ece\"}", "[\"friends\",\"chat\",\"people\"]");
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"launcher\",\"dm\":\"d:f1:me\"}");
        state.openDirect("f1");
        apply("{\"t\":\"friend-result\",\"friend\":\"f1\",\"action\":\"invite\",\"ok\":true}", 5_000);
        assertTrue(state.inviteWaiting("f1", 6_000));
        welcome("{\"name\":\"Can\"}", "[\"friends\",\"chat\",\"people\"]");
        assertNull(state.conversation("d:f1:me"));
        assertFalse(state.inviteWaiting("f1", 6_000));
        assertNull(state.personNote(6_000));
    }

    @Test
    void howAnActionWentIsSaidWithTheFriendsName() {
        welcome("{\"name\":\"Ece\"}", "[\"friends\",\"people\",\"invite\"]");
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"projects\"}");
        apply("{\"t\":\"friend\",\"id\":\"f2\",\"name\":\"Mert\",\"where\":\"offline\"}");
        apply("{\"t\":\"friend-result\",\"friend\":\"f1\",\"action\":\"invite\",\"ok\":true}", 10_000);
        OverlayState.PersonNote note = state.personNote(10_100);
        assertEquals("invited", note.key());
        assertFalse(note.error());
        assertTrue(state.inviteWaiting("f1", 10_000 + OverlayState.INVITE_WAIT_MS - 1));
        assertFalse(state.inviteWaiting("f1", 10_000 + OverlayState.INVITE_WAIT_MS));
        apply("{\"t\":\"friend-result\",\"friend\":\"f2\",\"action\":\"invite\",\"ok\":true,\"queued\":true}", 11_000);
        assertEquals("invite_queued", state.personNote(11_000).key());
        assertEquals("Mert", state.personNote(11_000).name());
        apply("{\"t\":\"friend-result\",\"friend\":\"f2\",\"action\":\"invite\",\"ok\":false,\"reason\":\"wait\"}",
            12_000);
        assertEquals("invite_wait", state.personNote(12_000).key());
        assertTrue(state.inviteWaiting("f2", 12_500), "a refusal for being too soon waits too, as the launcher");
        // The note lasts a moment.
        assertNull(state.personNote(12_000 + OverlayState.NOTE_MS));
        // An unfriended friend leaves the list before the answer: the name was kept when the action went.
        state.actionSent("f2");
        apply("{\"t\":\"friend-gone\",\"id\":\"f2\"}");
        apply("{\"t\":\"friend-result\",\"friend\":\"f2\",\"action\":\"unfriend\",\"ok\":true}", 20_000);
        assertEquals("removed", state.personNote(20_000).key());
        assertEquals("Mert", state.personNote(20_000).name());
        apply("{\"t\":\"friend-result\",\"friend\":\"f1\",\"action\":\"block\",\"ok\":false,\"reason\":\"offline\"}",
            21_000);
        assertEquals("failed", state.personNote(21_000).key());
        assertTrue(state.personNote(21_000).error());
        apply("{\"t\":\"friend-result\",\"friend\":\"f1\",\"action\":\"report\",\"ok\":true}", 22_000);
        assertEquals("reported", state.personNote(22_000).key());
        apply("{\"t\":\"friend-result\",\"friend\":\"f1\",\"action\":\"report\",\"ok\":false}", 23_000);
        assertEquals("report_failed", state.personNote(23_000).key());
        // Reported with "Ayrıca engelle": the row goes, and the note says so; the next report without it does not.
        state.reportSent("f1", true);
        apply("{\"t\":\"friend-result\",\"friend\":\"f1\",\"action\":\"report\",\"ok\":true}", 23_500);
        assertEquals("reported_blocked", state.personNote(23_500).key());
        state.reportSent("f1", false);
        apply("{\"t\":\"friend-result\",\"friend\":\"f1\",\"action\":\"report\",\"ok\":false}", 23_000);
        // Opening Discord says nothing when it went; something newer than this build says nothing either.
        apply("{\"t\":\"friend-result\",\"friend\":\"f1\",\"action\":\"discord\",\"ok\":true}", 24_000);
        apply("{\"t\":\"friend-result\",\"friend\":\"f1\",\"action\":\"later\",\"ok\":true}", 24_000);
        assertEquals("report_failed", state.personNote(24_000).key());
        state.personNoteText("Dünyan şu an açılamıyor.", true, 30_000);
        assertEquals("Dünyan şu an açılamıyor.", state.personNote(30_000).text());
    }

    @Test
    void thePlayersOwnTagShowsOnlyWhileTagsAreOn() {
        welcome("{\"name\":\"Ece\",\"badge\":\"t1\"}", "[\"friends\",\"tags\"]");
        apply("{\"t\":\"badges\",\"all\":true,\"cards\":{\"t1\":{\"tag\":\"GKŞ\",\"name\":\"Gece Kuşları\","
            + "\"members\":3,\"mine\":true}}}");
        assertEquals("t1", state.meBadge());
        assertEquals("GKŞ", state.badge(state.meBadge()).tag());
        // Taken off: the welcome comes again without it.
        welcome("{\"name\":\"Ece\"}", "[\"friends\",\"tags\"]");
        assertNull(state.meBadge());
        // Tags switched off: none, even if a welcome still named one.
        welcome("{\"name\":\"Ece\",\"badge\":\"t1\"}", "[\"friends\"]");
        assertNull(state.meBadge());
        assertNull(state.badge(state.meBadge()));
    }
}
