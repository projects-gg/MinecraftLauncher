package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import java.util.List;
import org.junit.jupiter.api.Test;

/** OVL-SOC 2's fields and answers (docs/tasks/ovl2-protocol.md, rows S) as the mod mirrors them. */
class OverlayStatePeople4Test {
    private final OverlayState state = new OverlayState();

    private void apply(String json) {
        state.apply(Protocol.parse(json), 1_000);
    }

    private void welcome(String rest) {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Ece\"}," + rest + "}");
    }

    @Test
    void theStatusAndATimedOnesEndComeWithTheWelcomeAndTheDndFrame() {
        welcome("\"dnd\":true,\"status\":\"dnd\",\"until\":1800000000000,"
            + "\"features\":[\"friends\",\"people\",\"people2\",\"people3\",\"people4\",\"next\"]");
        assertTrue(state.people4() && state.socNext() && !state.streamer());
        assertEquals("dnd", state.presence());
        assertEquals(1_800_000_000_000L, state.presenceUntil());
        apply("{\"t\":\"dnd\",\"on\":false,\"status\":\"invisible\"}");
        assertEquals("invisible", state.presence());
        assertEquals(0, state.presenceUntil(), "no end: \"Ben kapatana kadar\"");
        apply("{\"t\":\"dnd\",\"on\":false}");
        assertEquals("online", state.presence());
    }

    @Test
    void anOlderLaunchersDndStillReadsAsAStatus() {
        welcome("\"dnd\":true,\"features\":[\"friends\",\"people\"]");
        assertFalse(state.people4(), "no switcher for a launcher that takes no status");
        assertEquals("dnd", state.presence());
        apply("{\"t\":\"dnd\",\"on\":false}");
        assertEquals("online", state.presence());
        // A status word this build does not know reads as the dnd flag says.
        apply("{\"t\":\"dnd\",\"on\":true,\"status\":\"away\",\"until\":5}");
        assertEquals("dnd", state.presence());
    }

    @Test
    void theRequestsComeWithTheListsWholeEachTime() {
        welcome("\"features\":[\"friends\",\"people\",\"people2\",\"people3\",\"people4\"]");
        apply("{\"t\":\"lists\",\"groups\":[],\"crews\":[],\"requests\":["
            + "{\"id\":\"r-blok\",\"name\":\"BlokUstasi\"},{\"name\":\"no id\"},{\"id\":\"r-gece\",\"name\":\"GeceKartali_\"}]}");
        assertEquals(List.of(new OverlayState.Request("r-blok", "BlokUstasi"),
            new OverlayState.Request("r-gece", "GeceKartali_")), state.requests(), "a request without an id is none");
        apply("{\"t\":\"lists\",\"groups\":[],\"crews\":[]}");
        assertTrue(state.requests().isEmpty());
    }

    @Test
    void anAnsweredRequestSaysSoOnlyWhenItWentAndWhenAccepted() {
        welcome("\"features\":[\"friends\",\"people\",\"people2\",\"people3\",\"people4\"]");
        state.actionSent("r-blok", "BlokUstasi", null);
        apply("{\"t\":\"friend-result\",\"friend\":\"r-blok\",\"action\":\"accept\",\"ok\":true}");
        OverlayState.PersonNote note = state.personNote(1_000);
        assertEquals("now_friends", note.key());
        assertEquals("BlokUstasi", note.name());
        state.actionSent("r-gece", "GeceKartali_", null);
        apply("{\"t\":\"friend-result\",\"friend\":\"r-gece\",\"action\":\"decline\",\"ok\":true}");
        assertEquals("now_friends", state.personNote(1_000).key(), "a decline says nothing new");
        apply("{\"t\":\"friend-result\",\"friend\":\"r-x\",\"action\":\"decline\",\"ok\":false,\"reason\":\"failed\"}");
        assertEquals("failed", state.personNote(1_000).key());
    }

    @Test
    void aNicknamesRealNameIsKeptBesideTheFriend() {
        welcome("\"features\":[\"friends\",\"people\",\"people2\",\"people3\",\"people4\",\"next\"]");
        apply("{\"t\":\"friend\",\"id\":\"f-ayse\",\"name\":\"Kanka\",\"real\":\"PixelAvci\",\"where\":\"projects\"}");
        assertEquals("Kanka", state.friend("f-ayse").name());
        assertEquals("PixelAvci", state.realName("f-ayse"));
        apply("{\"t\":\"friend\",\"id\":\"f-ayse\",\"name\":\"PixelAvci\",\"where\":\"projects\"}");
        assertNull(state.realName("f-ayse"), "the nickname taken away");
    }

    @Test
    void ignoredAndMentionedLinesAreMarkedNeverThePlayersOwn() {
        welcome("\"features\":[\"friends\",\"chat\",\"people\",\"people2\",\"people3\",\"people4\",\"next\"]");
        apply("{\"t\":\"conv\",\"id\":\"c:t1\",\"kind\":\"crew\",\"title\":\"Gece\",\"unread\":0,\"muted\":false,"
            + "\"writable\":true}");
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"m1\",\"from\":\"MaviElmas\",\"text\":\"selam\",\"at\":1,"
            + "\"mine\":false,\"ignored\":true}");
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"m2\",\"from\":\"NetherKral\",\"text\":\"@Ece gel\",\"at\":2,"
            + "\"mine\":false,\"mention\":true}");
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"m3\",\"from\":\"Ece\",\"text\":\"tamam\",\"at\":3,"
            + "\"mine\":true,\"ignored\":true,\"mention\":true}");
        List<OverlayState.ChatLine> lines = List.copyOf(state.lines("c:t1"));
        assertTrue(lines.get(0).ignored && !lines.get(0).mention);
        assertTrue(lines.get(1).mention && !lines.get(1).ignored);
        assertFalse(lines.get(2).ignored || lines.get(2).mention, "the player's own line is never either");
    }

    @Test
    void streamerModeComesAsAFeature() {
        welcome("\"features\":[\"friends\",\"people\",\"people2\",\"people3\",\"people4\",\"next\",\"streamer\"]");
        assertTrue(state.streamer());
        welcome("\"features\":[\"friends\",\"people\",\"people2\",\"people3\",\"people4\",\"next\"]");
        assertFalse(state.streamer());
    }
}
