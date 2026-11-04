package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/** People's pictures from the launcher (docs/tasks/ovl2-protocol.md, OVL-AVATARS). */
class ProtocolAvatarsTest {
    @Test
    void theModSaysItDrawsThemOnlyWithItsPanel() {
        assertTrue(Protocol.hello("t", "0.1.0", "26.1.2", "tr_tr").getAsJsonArray("features").toString()
            .contains("\"avatars\""));
        assertFalse(Protocol.hello("t", "0.1.0", "26.1.2", "tr_tr", false).getAsJsonArray("features").toString()
            .contains("\"avatars\""), "the emoji picker alone draws no faces");
    }

    @Test
    void aWantNamesAtMostThirtyTwo() {
        assertEquals("{\"t\":\"avatar-want\",\"ids\":[\"f-ayse\",\"@me\"]}",
            Protocol.avatarWant(List.of("f-ayse", "@me")).toString());
        List<String> many = new ArrayList<>();
        for (int i = 0; i < 40; i++) {
            many.add("u-" + i);
        }
        assertEquals(Protocol.MAX_AVATAR_WANT, Protocol.avatarWant(many).getAsJsonArray("ids").size());
    }
}
