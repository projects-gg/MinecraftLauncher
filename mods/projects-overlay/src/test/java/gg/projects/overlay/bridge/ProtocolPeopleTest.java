package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import org.junit.jupiter.api.Test;

/** The friend menu's frames to the launcher (docs/tasks/ovl2-protocol.md, OVL-UX: P3-P7). */
class ProtocolPeopleTest {
    @Test
    void eachActionNamesItsFriend() {
        assertEquals("{\"t\":\"invite\",\"friend\":\"f1\"}", Protocol.invite("f1").toString());
        assertEquals("{\"t\":\"discord\",\"friend\":\"f1\"}", Protocol.discord("f1").toString());
        assertEquals("{\"t\":\"unfriend\",\"friend\":\"f2\"}", Protocol.unfriend("f2").toString());
        assertEquals("{\"t\":\"block\",\"friend\":\"f3\"}", Protocol.block("f3").toString());
    }

    @Test
    void aReportCarriesItsReasonAPlainNoteAndMaybeABlock() {
        JsonObject bare = Protocol.report("f4", "spam", "   ", false);
        assertEquals("{\"t\":\"report\",\"friend\":\"f4\",\"reason\":\"spam\"}", bare.toString());
        JsonObject full = Protocol.report("f4", "harassment", "§csürekli\nküfür", true);
        assertEquals("csürekli küfür", full.get("note").getAsString(),
            "plain text: the formatting sign dropped, the line break a space");
        assertTrue(full.get("block").getAsBoolean());
        String longNote = "a".repeat(250);
        assertEquals(Protocol.MAX_REPORT_NOTE_CHARS, Protocol.report("f4", "other", longNote, false).get("note")
            .getAsString().length());
        assertTrue(Protocol.REPORT_REASONS.contains("profile"));
        assertFalse(Protocol.REPORT_REASONS.contains("inappropriate"), "a message's reason, not a player's");
        assertTrue(Protocol.message("x").toString().length() < Protocol.MAX_OUTGOING_FRAME_BYTES);
    }
}
