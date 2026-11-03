package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** Friend requests and the status switcher to the launcher (docs/tasks/ovl2-protocol.md, OVL-SOC 2: S1, S4, S6). */
class ProtocolPeople4Test {
    @Test
    void theModSaysItHasThemOnlyWithItsPanel() {
        assertTrue(Protocol.hello("t", "0.1.0", "26.1.2", "tr_tr").getAsJsonArray("features").toString()
            .contains("\"people4\""));
        assertFalse(Protocol.hello("t", "0.1.0", "26.1.2", "tr_tr", false).getAsJsonArray("features").toString()
            .contains("\"people4\""), "the emoji picker alone has no list");
    }

    @Test
    void aStatusGoesWithItsDurationOnlyWhenThereIsOne() {
        assertEquals("{\"t\":\"status\",\"status\":\"dnd\",\"for\":\"hour\"}", Protocol.status("dnd", "hour").toString());
        assertEquals("{\"t\":\"status\",\"status\":\"online\"}", Protocol.status("online", null).toString());
    }

    @Test
    void aRequestIsAnsweredByItsId() {
        assertEquals("{\"t\":\"request\",\"id\":\"r-blok\",\"accept\":true}", Protocol.request("r-blok", true).toString());
        assertEquals("{\"t\":\"request\",\"id\":\"r-gece\",\"accept\":false}",
            Protocol.request("r-gece", false).toString());
    }
}
