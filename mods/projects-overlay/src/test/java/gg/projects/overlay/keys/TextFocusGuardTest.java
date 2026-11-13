package gg.projects.overlay.keys;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import org.junit.jupiter.api.Test;

/** SQOL-O: what the launcher hears about a text field in the game having the keyboard (rows SQ13-SQ14). */
class TextFocusGuardTest {
    @Test
    void nothingIsSaidUntilTheLauncherAsks() {
        TextFocusGuard guard = new TextFocusGuard();
        assertNull(guard.look(true, 0));
        guard.ask();
        assertEquals(Boolean.TRUE, guard.look(true, 10), "said at once: the first letters come right after");
        assertNull(guard.look(true, 60));
        assertTrue(guard.said());
    }

    @Test
    void losingTheKeyboardWaitsOutAFlicker() {
        TextFocusGuard guard = new TextFocusGuard();
        guard.ask();
        guard.look(true, 0);
        // The focus moves between two fields: nothing said.
        assertNull(guard.look(false, 100));
        assertNull(guard.look(true, 150));
        assertNull(guard.look(false, 200));
        assertNull(guard.look(false, 200 + TextFocusGuard.OFF_DELAY_MS - 1));
        assertEquals(Boolean.FALSE, guard.look(false, 200 + TextFocusGuard.OFF_DELAY_MS));
        assertFalse(guard.said());
        assertNull(guard.look(false, 10_000), "said once");
    }

    @Test
    void aLongTypeIsSaidAgainSoTheLauncherKeepsIt() {
        TextFocusGuard guard = new TextFocusGuard();
        guard.ask();
        guard.look(true, 0);
        assertNull(guard.look(true, TextFocusGuard.REFRESH_MS - 1));
        assertEquals(Boolean.TRUE, guard.look(true, TextFocusGuard.REFRESH_MS));
        assertTrue(TextFocusGuard.REFRESH_MS < 45_000, "inside the launcher's hold");
    }

    @Test
    void aLauncherThatGoesForgetsIt() {
        TextFocusGuard guard = new TextFocusGuard();
        guard.ask();
        guard.look(true, 0);
        guard.reset();
        assertNull(guard.look(true, 10), "a new launcher asks first");
        guard.ask();
        assertEquals(Boolean.TRUE, guard.look(true, 20));
        assertEquals("{\"t\":\"text-focus\",\"on\":true}", Protocol.textFocus(true).toString());
        assertTrue(Protocol.hello("t", "1", "26.1.2", "tr_tr").toString().contains("\"text-focus\""));
        assertFalse(Protocol.hello("t", "1", "26.1.2", "tr_tr", false).toString().contains("\"text-focus\""),
            "the emoji picker alone does not say it");
    }
}
