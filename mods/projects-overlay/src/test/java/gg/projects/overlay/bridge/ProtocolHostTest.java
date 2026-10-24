package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * World sharing's frames as the launcher reads them: the JSON here is the one {@code crates/pl-overlay} parses in its
 * own test ({@code host_frames_read_and_write_as_the_log_says}), rows H1–H17 of docs/tasks/ovl2-protocol.md.
 */
class ProtocolHostTest {
    private static JsonObject json(String s) {
        return JsonParser.parseString(s).getAsJsonObject();
    }

    @Test
    void theHelloSaysTheModHosts() {
        JsonObject hello = Protocol.hello("t", "0.1.0", "26.1.2", "tr_tr");
        assertTrue(hello.getAsJsonArray("features").toString().contains("\"host\""));
    }

    @Test
    void theWorldIsToldAsTheLauncherReadsIt() {
        assertEquals(json("{\"t\":\"host-world\",\"open\":true,\"folder\":\"Yeni Dünya\",\"mode\":\"survival\","
            + "\"lan\":false}"), Protocol.hostWorld(true, "Yeni Dünya", "survival", false));
        assertEquals(json("{\"t\":\"host-world\",\"open\":false}"), Protocol.hostWorld(false, null, null, false));
    }

    @Test
    void requestsAndAnswers() {
        assertEquals(json("{\"t\":\"host-request\",\"mode\":\"creative\",\"commands\":true,\"invite\":[\"f1\",\"f2\"]}"),
            Protocol.hostRequest("creative", true, List.of("f1", "f2")));
        assertEquals(json("{\"t\":\"host-published\",\"id\":\"p1\",\"ok\":true,\"port\":51234,\"proto\":774}"),
            Protocol.hostPublished("p1", 51234, 774));
        assertEquals(json("{\"t\":\"host-published\",\"id\":\"p1\",\"ok\":false,\"reason\":\"lan\"}"),
            Protocol.hostPublishFailed("p1", "lan"));
        assertEquals(json("{\"t\":\"host-invite\",\"invite\":[\"f3\"]}"), Protocol.hostInvite(List.of("f3")));
        assertEquals(json("{\"t\":\"host-remove\",\"guest\":\"g2\"}"), Protocol.hostRemove("g2"));
        assertEquals(json("{\"t\":\"host-stop\",\"reason\":\"player\"}"), Protocol.hostStop("player"));
        assertEquals(json("{\"t\":\"host-stop\"}"), Protocol.hostStop(null));
        assertEquals(json("{\"t\":\"host-meet\",\"invite\":[\"f1\"]}"), Protocol.hostMeet(List.of("f1")));
    }

    @Test
    void guestsInOutAndRefused() {
        assertEquals(json("{\"t\":\"host-guest\",\"conn\":3,\"guest\":\"g1\",\"name\":\"Notch\",\"state\":\"in\"}"),
            Protocol.hostGuest(3, "g1", "Notch", "in", null));
        JsonObject out = Protocol.hostGuest(3, "g1", "Notch", "out", "left");
        assertEquals("left", out.get("reason").getAsString());
        JsonObject refused = Protocol.hostGuest(4, "g2", "Ayse", "refused", "name");
        assertEquals("refused", refused.get("state").getAsString());
        assertFalse(Protocol.hostGuest(5, "g1", "Notch", "in", null).has("reason"));
    }
}
