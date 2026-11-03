package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import org.junit.jupiter.api.Test;

/** OVL-SOC: a friend's presence dot (V23), the launcher's states, from the friend frames. */
class PresenceTest {
    private final OverlayState state = new OverlayState();

    private Presence presenceOf(String friend) {
        state.apply(Protocol.parse(friend), 1_000);
        return state.friend("f1").presence();
    }

    @Test
    void everyStateHasItsDotAndOfflineNone() {
        state.apply(Protocol.parse("{\"t\":\"welcome\",\"proto\":1,\"me\":{\"name\":\"Ece\"},"
            + "\"features\":[\"friends\"]}"), 1_000);
        assertEquals(Presence.PLAYING, presenceOf("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Kaan_07\","
            + "\"where\":\"projects\"}"));
        assertEquals(Presence.PLAYING, presenceOf("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Kaan_07\","
            + "\"where\":\"game\"}"), "in the game's menus");
        assertEquals(Presence.LAUNCHER, presenceOf("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Kaan_07\","
            + "\"where\":\"launcher\"}"));
        assertEquals(Presence.DND, presenceOf("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Kaan_07\","
            + "\"where\":\"single\",\"dnd\":true}"), "do-not-disturb wins over where they are");
        assertEquals(Presence.OFFLINE, presenceOf("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Kaan_07\","
            + "\"where\":\"offline\",\"dnd\":true}"), "offline has no dot, whatever else it says");
        assertFalse(Presence.OFFLINE.online());
        assertTrue(Presence.DND.online());
        // An older launcher never says dnd: the dots are as before, by where they are.
        assertEquals(Presence.LAUNCHER, presenceOf("{\"t\":\"friend\",\"id\":\"f1\",\"name\":\"Kaan_07\","
            + "\"where\":\"launcher\"}"));
    }
}
