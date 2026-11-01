package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import org.junit.jupiter.api.Test;

/** CHAT-CALM: a crew chat's slow mode as the mod mirrors it (the wait in the composer). */
class OverlayStateSlowTest {
    private static final long NOW = 1_800_000_000_000L;
    private final OverlayState state = new OverlayState();

    private void apply(String json) {
        state.apply(Protocol.parse(json), NOW);
    }

    @Test
    void theModSaysItKnowsSlowMode() {
        assertTrue(Protocol.FEATURES.contains("slow"));
    }

    @Test
    void theWaitCountsFromThePlayersNewestMessage() {
        apply("{\"t\":\"conv\",\"id\":\"c:t1\",\"kind\":\"crew\",\"title\":\"Gece\",\"slow\":30}");
        assertEquals(30, state.conversation("c:t1").slow());
        // Nothing of the player's yet: no wait.
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"m1\",\"from\":\"Cem\",\"text\":\"toplanın\",\"at\":"
            + (NOW - 5_000) + "}");
        assertEquals(0, state.slowUntil("c:t1"));
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"m2\",\"from\":\"Ece\",\"mine\":true,\"text\":\"geliyorum\","
            + "\"at\":" + (NOW - 12_000) + "}");
        assertEquals(NOW + 18_000, state.slowUntil("c:t1"));
    }

    @Test
    void noWaitWithoutSlowModeOrFromAnOlderLauncher() {
        apply("{\"t\":\"conv\",\"id\":\"c:t1\",\"kind\":\"crew\",\"title\":\"Gece\"}");
        apply("{\"t\":\"msg\",\"conv\":\"c:t1\",\"id\":\"m2\",\"from\":\"Ece\",\"mine\":true,\"text\":\"geliyorum\","
            + "\"at\":" + (NOW - 2_000) + "}");
        assertEquals(0, state.conversation("c:t1").slow());
        assertEquals(0, state.slowUntil("c:t1"));
        // A value out of reason is held to an hour.
        apply("{\"t\":\"conv\",\"id\":\"c:t2\",\"kind\":\"crew\",\"title\":\"Kuşlar\",\"slow\":99999}");
        assertEquals(3600, state.conversation("c:t2").slow());
    }
}
