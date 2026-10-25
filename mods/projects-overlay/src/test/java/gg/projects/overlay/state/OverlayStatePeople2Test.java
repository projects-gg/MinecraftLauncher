package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;

/** OVL-UX2's fields and frames (docs/tasks/ovl2-protocol.md, rows Q) as the mod mirrors them. */
class OverlayStatePeople2Test {
    private final OverlayState state = new OverlayState();

    private void apply(String json) {
        state.apply(Protocol.parse(json), 1_000);
    }

    private void welcome(String features) {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Ece\"},\"features\":" + features + "}");
    }

    @Test
    void anOfflineFriendSaysWhenTheyWereSeenInTheLaunchersWords() {
        welcome("[\"friends\",\"chat\",\"people\"]");
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Can\",\"where\":\"offline\",\"seen\":\"yesterday\"}");
        assertEquals("yesterday", state.friend("f1").seen());
        // Only the launcher's rough words: a time or anything else is never shown.
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Can\",\"where\":\"offline\",\"seen\":\"2026-10-24T10:00\"}");
        assertNull(state.friend("f1").seen());
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Can\",\"where\":\"offline\",\"seen\":5}");
        assertNull(state.friend("f1").seen());
        // Gone with the next frame without it (they stopped sharing it).
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Can\",\"where\":\"offline\",\"seen\":\"week\"}");
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Can\",\"where\":\"offline\"}");
        assertNull(state.friend("f1").seen());
    }

    @Test
    void friendsCarryTheirMarksCategoryAndCrews() {
        welcome("[\"friends\",\"chat\",\"people\",\"people2\"]");
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"projects\",\"fav\":true,\"alert\":true,"
            + "\"group\":\"g1\",\"crews\":{\"t1\":\"invited\",\"t2\":\"member\",\"t3\":\"boss\"}}");
        OverlayState.Friend ayse = state.friend("f1");
        assertTrue(ayse.fav() && ayse.alert());
        assertEquals("g1", ayse.group());
        assertEquals(Map.of("t1", "invited", "t2", "member"), ayse.crews(), "only the launcher's words");
        // A later frame without them: not a favourite, no notice, in "Arkadaşlar", nowhere.
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"projects\",\"group\":\"Not a Id!\"}");
        ayse = state.friend("f1");
        assertFalse(ayse.fav() || ayse.alert());
        assertNull(ayse.group());
        assertTrue(ayse.crews().isEmpty());
    }

    @Test
    void theListsComeWholeAndGoWithTheAccount() {
        welcome("[\"friends\",\"chat\",\"people\",\"people2\"]");
        apply("{\"t\":\"lists\",\"groups\":[{\"id\":\"g1\",\"name\":\"Okul\"},{\"id\":\"BAD\",\"name\":\"x\"}],"
            + "\"crews\":[{\"id\":\"t1\",\"name\":\"Gece Kuşları\",\"full\":true}],\"crewmates\":true,"
            + "\"asked\":[\"u-cem\"],\"asking\":[\"u-ece\"]}");
        assertEquals(List.of(new OverlayState.Group("g1", "Okul")), state.groups());
        assertEquals(List.of(new OverlayState.CrewTarget("t1", "Gece Kuşları", true)), state.inviteCrews());
        assertTrue(state.crewmatesAsk() && state.askedBy("u-cem") && state.asking("u-ece"));
        assertFalse(state.askedBy("u-ece"));
        // The whole set each time.
        apply("{\"t\":\"lists\",\"groups\":[],\"crews\":[]}");
        assertTrue(state.groups().isEmpty() && state.inviteCrews().isEmpty());
        assertFalse(state.crewmatesAsk() || state.askedBy("u-cem"));
        // Another account: nothing of the previous one's lists stays.
        apply("{\"t\":\"lists\",\"groups\":[{\"id\":\"g1\",\"name\":\"Okul\"}],\"crews\":[]}");
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Mert\"},\"features\":[\"friends\",\"people2\"]}");
        assertTrue(state.groups().isEmpty());
    }

    @Test
    void crewmatesOnProjectsAndTheirDiscordComeWithTheLists() {
        welcome("[\"friends\",\"chat\",\"people\",\"people2\"]");
        apply("{\"t\":\"lists\",\"groups\":[],\"crews\":[],\"projects\":[\"u-cem\",\"\",7],\"discord\":[\"u-cem\","
            + "\"u-ali\"]}");
        assertTrue(state.mateOnProjects("u-cem") && state.mateDiscord("u-cem") && state.mateDiscord("u-ali"));
        assertFalse(state.mateOnProjects("u-ali") || state.mateOnProjects(""));
        // Whole each time: Cem left Projects.
        apply("{\"t\":\"lists\",\"groups\":[],\"crews\":[],\"discord\":[\"u-cem\"]}");
        assertFalse(state.mateOnProjects("u-cem") || state.mateDiscord("u-ali"));
        // A launcher without the whole person menu: never offered.
        welcome("[\"friends\",\"chat\",\"people\"]");
        apply("{\"t\":\"lists\",\"groups\":[],\"crews\":[],\"projects\":[\"u-cem\"],\"discord\":[\"u-cem\"]}");
        assertFalse(state.mateOnProjects("u-cem") || state.mateDiscord("u-cem"));
    }

    @Test
    void theAnswersSayWhatHappenedWithWhatTheyNamed() {
        welcome("[\"friends\",\"chat\",\"people\",\"people2\"]");
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"projects\"}");
        state.actionSent("f1", "Ayşe", "Okul");
        apply("{\"t\":\"friend-result\",\"friend\":\"f1\",\"action\":\"move\",\"ok\":true}");
        OverlayState.PersonNote note = state.personNote(1_000);
        assertEquals("moved", note.key());
        assertEquals("Okul", note.detail());
        state.actionSent("f1", "Ayşe", "Gece Kuşları");
        apply("{\"t\":\"friend-result\",\"friend\":\"f1\",\"action\":\"crew-invite\",\"ok\":true}");
        assertEquals("crew_invited", state.personNote(1_000).key());
        // A crewmate (not on the friend list): their name came with the action.
        state.actionSent("u-cem", "Cem", null);
        apply("{\"t\":\"friend-result\",\"friend\":\"u-cem\",\"action\":\"add-friend\",\"ok\":true,\"queued\":true}");
        note = state.personNote(1_000);
        assertEquals("request_sent", note.key());
        assertEquals("Cem", note.name());
        assertTrue(state.asking("u-cem"), "the next menu says the request went");
        state.actionSent("u-ali", "Ali", null);
        apply("{\"t\":\"friend-result\",\"friend\":\"u-ali\",\"action\":\"add-friend\",\"ok\":true}");
        assertEquals("now_friends", state.personNote(1_000).key());
        // A favourite is said by the mod itself; only a refusal comes back.
        state.actionSent("f1", "Ayşe", null);
        apply("{\"t\":\"friend-result\",\"friend\":\"f1\",\"action\":\"favorite\",\"ok\":false,\"reason\":\"rate\"}");
        note = state.personNote(1_000);
        assertEquals("failed", note.key());
        assertTrue(note.error());
    }

    @Test
    void aSendersIdComesWithTheirMessageInCrewChat() {
        welcome("[\"friends\",\"chat\",\"people\",\"people2\"]");
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:1\",\"from\":\"Cem\",\"text\":\"selam\",\"at\":1,"
            + "\"mine\":false,\"by\":\"u-cem\"}");
        assertEquals("u-cem", state.lines("c:t1").getFirst().by);
        // The player's own never carries one.
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"i:2\",\"from\":\"Ece\",\"text\":\"hey\",\"at\":2,"
            + "\"mine\":true,\"by\":\"me\"}");
        assertNull(state.lines("c:t1").getLast().by);
    }

    @Test
    void aChipsFriendIsFoundByTheirConversation() {
        welcome("[\"friends\",\"chat\",\"people\",\"people2\"]");
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"projects\",\"dm\":\"d:f1:me\"}");
        assertEquals("f1", state.friendOfDirect("d:f1:me"));
        assertNull(state.friendOfDirect("c:t1"));
    }
}
