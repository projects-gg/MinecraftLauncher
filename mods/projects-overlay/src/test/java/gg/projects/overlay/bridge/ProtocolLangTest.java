package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import org.junit.jupiter.api.Test;

/** The overlay's language on the bridge (docs/tasks/ovl2-protocol.md, OVL-LANG: G1, G4). */
class ProtocolLangTest {
    @Test
    void theModSaysItsWordsFollowTheLauncher() {
        JsonObject hello = Protocol.hello("t", "0.1.0", "26.1.2", "en_us");
        assertTrue(hello.getAsJsonArray("features").toString().contains("\"lang\""));
        assertEquals("en_us", Protocol.str(hello, "lang", ""), "the language it shows now");
    }

    @Test
    void thePlayersPickNamesTheLanguage() {
        assertEquals("{\"t\":\"lang\",\"lang\":\"en\"}", Protocol.lang("en").toString());
        assertEquals("{\"t\":\"lang\",\"lang\":\"tr\"}", Protocol.lang("tr").toString());
    }
}
