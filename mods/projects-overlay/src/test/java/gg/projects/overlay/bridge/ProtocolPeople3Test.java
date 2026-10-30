package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** A crewmate's crew rows and "Yeni kategori…" to the launcher (docs/tasks/ovl2-protocol.md, OVL-UX3: U1-U5). */
class ProtocolPeople3Test {
    @Test
    void theModSaysItHasTheCrewRowsOnlyWithItsPanel() {
        assertTrue(Protocol.hello("t", "0.1.0", "26.1.2", "tr_tr").getAsJsonArray("features").toString()
            .contains("\"people3\""));
        assertFalse(Protocol.hello("t", "0.1.0", "26.1.2", "tr_tr", false).getAsJsonArray("features").toString()
            .contains("\"people3\""), "the emoji picker alone has no menu");
    }

    @Test
    void eachRowNamesTheCrewAndTheCrewmate() {
        assertEquals("{\"t\":\"crew-mod\",\"friend\":\"u-cem\",\"crew\":\"t1\",\"on\":true}",
            Protocol.crewModerator("t1", "u-cem", true).toString());
        assertEquals("{\"t\":\"crew-mod\",\"friend\":\"u-ali\",\"crew\":\"t1\",\"on\":false}",
            Protocol.crewModerator("t1", "u-ali", false).toString());
        assertEquals("{\"t\":\"crew-lead\",\"friend\":\"u-cem\",\"crew\":\"t1\"}",
            Protocol.crewLead("t1", "u-cem").toString());
        assertEquals("{\"t\":\"crew-remove\",\"friend\":\"u-ali\",\"crew\":\"t2\"}",
            Protocol.crewRemove("t2", "u-ali").toString());
    }

    @Test
    void aNewCategoryIsAMoveWithANameAndNoCategory() {
        assertEquals("{\"t\":\"move\",\"friend\":\"f1\",\"group\":\"\",\"name\":\"Okul arkadaşları\"}",
            Protocol.moveToNew("f1", "Okul arkadaşları").toString());
    }
}
