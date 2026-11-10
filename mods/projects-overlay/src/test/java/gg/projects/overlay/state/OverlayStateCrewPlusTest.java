package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * CREW-PLUS (docs/tasks/ovl2-protocol.md, "CREW-PLUS"): the player's own plan templates in {@code lists} and a friend's
 * game session start on the friend frame, as the mod mirrors them; a launcher from before sends neither.
 */
class OverlayStateCrewPlusTest {
    private final OverlayState state = new OverlayState();

    private void apply(String json) {
        state.apply(Protocol.parse(json), 1_000);
    }

    @Test
    void theOwnTemplatesComeWithTheListsAndOnlyValidOnes() {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Ece\"},\"features\":[\"friends\",\"people\","
            + "\"people2\",\"plans\"]}");
        assertTrue(state.planTemplates().isEmpty(), "a launcher from before: the built-in ones only");
        apply("{\"t\":\"lists\",\"groups\":[],\"crews\":[],\"planners\":[\"t1\"],\"templates\":["
            + "{\"name\":\"Ada turu\",\"day\":\"sat\",\"time\":900,\"note\":\"Sesliye gelin\"},"
            + "{\"name\":\"Gece\",\"day\":\"today\",\"time\":1290},"
            + "{\"name\":\"Bozuk\",\"day\":\"someday\",\"time\":900},"
            + "{\"name\":\"Yamuk\",\"day\":\"fri\",\"time\":1291}]}");
        assertEquals(List.of(new OverlayState.PlanTemplate("Ada turu", "sat", 900, "Sesliye gelin"),
            new OverlayState.PlanTemplate("Gece", "today", 1290, "")), state.planTemplates());
        // The next lists without them: none.
        apply("{\"t\":\"lists\",\"groups\":[],\"crews\":[],\"planners\":[\"t1\"]}");
        assertTrue(state.planTemplates().isEmpty());
    }

    @Test
    void aFriendsGameSessionStartRidesOnTheirFrame() {
        apply("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Ece\"},\"features\":[\"friends\",\"people\"]}");
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"projects\",\"since\":1792400100000}");
        assertEquals(1_792_400_100_000L, state.friend("f1").since());
        apply("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Ayşe\",\"where\":\"launcher\"}");
        assertEquals(0L, state.friend("f1").since());
        apply("{\"t\":\"friend\",\"id\":\"f2\",\"name\":\"Mert\",\"where\":\"game\",\"since\":-5}");
        assertEquals(0L, state.friend("f2").since());
    }
}
