package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** The rest of the person menu's frames to the launcher (docs/tasks/ovl2-protocol.md, OVL-UX2: Q1, Q8-Q12). */
class ProtocolPeople2Test {
    @Test
    void theModSaysItHasTheWholeMenu() {
        assertTrue(Protocol.hello("t", "0.1.0", "26.1.2", "tr_tr").getAsJsonArray("features").toString()
            .contains("\"people2\""));
    }

    @Test
    void eachRowNamesItsPersonAndWhatItChooses() {
        assertEquals("{\"t\":\"favorite\",\"friend\":\"f1\",\"on\":true}", Protocol.favorite("f1", true).toString());
        assertEquals("{\"t\":\"alert\",\"friend\":\"f1\",\"on\":false}", Protocol.alert("f1", false).toString());
        assertEquals("{\"t\":\"move\",\"friend\":\"f2\",\"group\":\"g1\"}", Protocol.move("f2", "g1").toString());
        assertEquals("{\"t\":\"move\",\"friend\":\"f2\",\"group\":\"\"}", Protocol.move("f2", null).toString(),
            "\"Arkadaşlar\" is the empty category");
        assertEquals("{\"t\":\"crew-invite\",\"friend\":\"f3\",\"crew\":\"t1\"}",
            Protocol.crewInvite("f3", "t1").toString());
        assertEquals("{\"t\":\"add-friend\",\"friend\":\"u-cem\"}", Protocol.addFriend("u-cem").toString());
    }
}
