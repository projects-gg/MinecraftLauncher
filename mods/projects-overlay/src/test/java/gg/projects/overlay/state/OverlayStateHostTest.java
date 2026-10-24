package gg.projects.overlay.state;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;

/** World sharing's mirror (H8) and friends' world invites (H13, H14) in the overlay's state. */
class OverlayStateHostTest {
    private static JsonObject f(String json) {
        return JsonParser.parseString(json).getAsJsonObject();
    }

    @Test
    void theLaunchersMirrorOfTheSessionWhole() {
        OverlayState s = new OverlayState();
        int before = s.revision();
        s.apply(f("{\"t\":\"host-state\",\"open\":true,\"max\":7,\"guests\":["
            + "{\"guest\":\"g1\",\"friend\":\"f1\",\"label\":\"Mert\",\"name\":\"Notch\",\"state\":\"in\"},"
            + "{\"guest\":\"\",\"friend\":\"f2\",\"label\":\"Ayşe\",\"state\":\"invited\"},"
            + "{\"guest\":\"g2\",\"friend\":\"f3\",\"label\":\"Can\",\"state\":\"joining\"},"
            + "{\"guest\":\"g9\",\"friend\":\"\",\"label\":\"nobody\",\"state\":\"in\"}]}"), 0);
        assertTrue(s.revision() > before);
        OverlayState.HostView view = s.host();
        assertNotNull(view);
        assertTrue(view.open());
        assertEquals(7, view.max());
        assertEquals(3, view.guests().size());
        assertEquals(1, view.inWorld());
        assertEquals("Notch", view.byGuest("g1").name());
        assertTrue(view.byGuest("g2").joining());
        assertNull(view.byGuest(""));
        assertNull(view.error());
        s.hostEnded();
        assertNull(s.host());
    }

    @Test
    void aWorldThatDidNotOpenSaysWhy() {
        OverlayState s = new OverlayState();
        s.apply(f("{\"t\":\"host-state\",\"open\":false,\"max\":7,\"guests\":[],\"error\":\"limit\"}"), 0);
        assertFalse(s.host().open());
        assertEquals("limit", s.host().error());
        s.apply(f("{\"t\":\"host-state\",\"open\":false,\"max\":99}"), 0);
        assertEquals("unavailable", s.host().error());
        assertEquals(7, s.host().max());
    }

    @Test
    void aFriendsWorldInviteComesAndGoesWithTheirRow() {
        OverlayState s = new OverlayState();
        s.apply(f("{\"t\":\"snapshot\",\"friends\":[{\"id\":\"a\",\"name\":\"Ayşe\",\"where\":\"single\","
            + "\"joinable\":true,\"world\":{\"guests\":1,\"max\":7}},{\"id\":\"b\",\"name\":\"Bora\","
            + "\"where\":\"single\",\"joinable\":false}],\"convs\":[],\"calls\":[]}"), 0);
        assertEquals(new OverlayState.WorldMark(1, 7), s.world("a"));
        assertNull(s.world("b"));
        s.apply(f("{\"t\":\"friend\",\"id\":\"a\",\"name\":\"Ayşe\",\"where\":\"single\",\"joinable\":false}"), 0);
        assertNull(s.world("a"));
        s.apply(f("{\"t\":\"friend\",\"id\":\"b\",\"name\":\"Bora\",\"where\":\"single\",\"joinable\":true,"
            + "\"world\":{\"guests\":3,\"max\":4}}"), 0);
        assertEquals(new OverlayState.WorldMark(3, 4), s.world("b"));
        s.apply(f("{\"t\":\"friend-gone\",\"id\":\"b\"}"), 0);
        assertNull(s.world("b"));
    }

    @Test
    void anotherAccountLeavesNothingOfTheLastOnesWorlds() {
        OverlayState s = new OverlayState();
        s.apply(f("{\"t\":\"welcome\",\"me\":{\"name\":\"Ayşe\"},\"features\":[\"friends\",\"host\"]}"), 0);
        assertTrue(s.has("host"));
        s.apply(f("{\"t\":\"friend\",\"id\":\"b\",\"name\":\"Bora\",\"where\":\"single\",\"joinable\":true,"
            + "\"world\":{\"guests\":0,\"max\":7}}"), 0);
        s.apply(f("{\"t\":\"host-state\",\"open\":true,\"max\":7,\"guests\":[]}"), 0);
        s.apply(f("{\"t\":\"welcome\",\"me\":{\"name\":\"Mert\"},\"features\":[\"friends\"]}"), 0);
        assertNull(s.world("b"));
        assertNull(s.host());
        assertFalse(s.has("host"));
    }

    @Test
    void aWorldInviteToastIsItsOwnKind() {
        OverlayState s = new OverlayState();
        OverlayState.ToastRequest t = s.apply(f("{\"t\":\"toast\",\"kind\":\"world\",\"key\":\"world:s1\","
            + "\"title\":\"Ayşe\"}"), 0);
        assertEquals("world", t.kind());
        assertEquals("Ayşe", t.title());
    }
}
