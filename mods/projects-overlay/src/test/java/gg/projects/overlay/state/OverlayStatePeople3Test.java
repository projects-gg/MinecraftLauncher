package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import java.util.Map;
import org.junit.jupiter.api.Test;

/** OVL-UX3's fields and answers (docs/tasks/ovl2-protocol.md, rows U) as the mod mirrors them. */
class OverlayStatePeople3Test {
    private final OverlayState state = new OverlayState();

    private void apply(String json) {
        state.apply(Protocol.parse(json), 1_000);
    }

    private void welcome(String features) {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Ece\"},\"features\":" + features + "}");
    }

    private static final String ROLES = "{\"t\":\"lists\",\"groups\":[],\"crews\":[],\"roles\":{"
        + "\"t1\":{\"lead\":true,\"roles\":true,\"modsFull\":true,\"conv\":\"c:t1\","
        + "\"mates\":{\"u-ali\":{\"mod\":true,\"kick\":true},\"u-cem\":{\"kick\":true},\"\":{\"kick\":true}}},"
        + "\"t2\":{\"mates\":{\"u-cem\":{\"kick\":true}}},\"t3\":5}}";

    @Test
    void theCrewsRightsFollowTheirChats() {
        welcome("[\"friends\",\"chat\",\"people\",\"people2\",\"people3\"]");
        apply(ROLES);
        OverlayState.CrewRights led = state.crewRightsIn("c:t1");
        assertEquals("t1", led.crew());
        assertTrue(led.lead() && led.roles() && led.modsFull());
        assertEquals(Map.of("u-ali", new OverlayState.MateRights(true, true), "u-cem",
            new OverlayState.MateRights(false, true)), led.mates(), "never an empty id");
        // A crew without a chat has no sender to carry its rows; another chat has none.
        assertNull(state.crewRightsIn("c:t2"));
        assertNull(state.crewRightsIn(null));
        // The whole set each time.
        apply("{\"t\":\"lists\",\"groups\":[],\"crews\":[]}");
        assertNull(state.crewRightsIn("c:t1"));
    }

    @Test
    void anOlderLauncherOffersNoCrewRows() {
        welcome("[\"friends\",\"chat\",\"people\",\"people2\"]");
        apply(ROLES);
        assertNull(state.crewRightsIn("c:t1"), "without people3 in its welcome the launcher takes no crew row");
    }

    @Test
    void theRowsAnswersSayWhatHappened() {
        welcome("[\"friends\",\"chat\",\"people\",\"people2\",\"people3\"]");
        state.actionSent("u-cem", "Cem", "on");
        apply("{\"t\":\"friend-result\",\"friend\":\"u-cem\",\"action\":\"crew-mod\",\"ok\":true}");
        assertEquals("made_moderator", state.personNote(1_000).key());
        state.actionSent("u-ali", "Ali", "off");
        apply("{\"t\":\"friend-result\",\"friend\":\"u-ali\",\"action\":\"crew-mod\",\"ok\":true}");
        assertEquals("unmade_moderator", state.personNote(1_000).key());
        assertNull(state.personNote(1_000).detail(), "the moderator row's detail only chose the words");
        state.actionSent("u-cem", "Cem", null);
        apply("{\"t\":\"friend-result\",\"friend\":\"u-cem\",\"action\":\"crew-lead\",\"ok\":true}");
        assertEquals("made_founder", state.personNote(1_000).key());
        state.actionSent("u-cem", "Cem", null);
        apply("{\"t\":\"friend-result\",\"friend\":\"u-cem\",\"action\":\"crew-remove\",\"ok\":false,"
            + "\"reason\":\"failed\"}");
        assertEquals("failed", state.personNote(1_000).key());
        assertTrue(state.personNote(1_000).error());
        assertEquals("Cem", state.personNote(1_000).name());
    }
}
