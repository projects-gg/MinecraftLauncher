package gg.projects.overlay.keys;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/**
 * KEY-HINT: a toast's key hint is one line, the Fn one only where this computer gives F1 to F12 with Fn alone, learnt
 * from what the game's window hears.
 */
class KeyHintTest {
    private static final boolean SHOWN = true;
    private static final boolean F_KEY = true;
    private static final boolean LAPTOP = true;
    private static final boolean MAC = true;

    @Test
    void theKeyAloneWhereFnCannotBeWhatIsMissing() {
        assertEquals(KeyHint.Line.KEY, KeyHint.line(SHOWN, F_KEY, false, false, false, false), "a desktop");
        assertEquals(KeyHint.Line.KEY, KeyHint.line(SHOWN, false, LAPTOP, false, false, true), "a letter or Shift");
        assertEquals(KeyHint.Line.KEY, KeyHint.line(SHOWN, false, LAPTOP, MAC, false, false), "a Mac, not an F-key");
        assertEquals(KeyHint.Line.NONE, KeyHint.line(false, F_KEY, LAPTOP, MAC, false, true), "the hint turned off");
    }

    @Test
    void aWindowsOrLinuxLaptopSaysTheKeyUntilItsTopRowSendsSomethingElse() {
        assertEquals(KeyHint.Line.KEY, KeyHint.line(SHOWN, F_KEY, LAPTOP, false, false, false), "nothing learnt yet");
        assertEquals(KeyHint.Line.FN, KeyHint.line(SHOWN, F_KEY, LAPTOP, false, false, true), "a media key came");
        assertEquals(KeyHint.Line.FN, KeyHint.line(SHOWN, F_KEY, LAPTOP, false, true, true), "and stays after it opened");
        assertEquals(KeyHint.Line.KEY, KeyHint.line(SHOWN, F_KEY, LAPTOP, false, true, false), "the key worked alone");
    }

    @Test
    void aMacSaysFnUntilTheKeyWorksAlone() {
        assertEquals(KeyHint.Line.FN, KeyHint.line(SHOWN, F_KEY, LAPTOP, MAC, false, false), "media keys come first");
        assertEquals(KeyHint.Line.KEY, KeyHint.line(SHOWN, F_KEY, LAPTOP, MAC, true, false), "standard F-keys");
        assertEquals(KeyHint.Line.FN, KeyHint.line(SHOWN, F_KEY, LAPTOP, MAC, true, true), "opened with fn");
    }

    @Test
    void whatTheTopRowSendsInsteadOfAnFKey() {
        int unknown = -1;
        int press = 1;
        int release = 0;
        int repeat = 2;
        assertTrue(KeyHint.media(unknown, press, false), "a sound or media key GLFW does not name");
        assertTrue(KeyHint.media(313, press, false), "F24 (the touchpad key's chord)");
        assertFalse(KeyHint.media(323, press, false), "a number pad key");
        assertTrue(KeyHint.media(314, press, false), "F25");
        assertFalse(KeyHint.media(unknown, release, false), "only a press");
        assertFalse(KeyHint.media(unknown, repeat, false));
        assertFalse(KeyHint.media(296, press, false), "F7 itself");
        assertFalse(KeyHint.media(301, press, false), "F12");
        assertFalse(KeyHint.media(65, press, false), "a letter");
        assertFalse(KeyHint.media(unknown, press, MAC), "a Mac's media keys never reach the game");
    }

    @Test
    void aMacsFnAndTheKeyThatWentDownWithIt() {
        assertTrue(KeyHint.macFn(-1, 63, MAC));
        assertFalse(KeyHint.macFn(-1, 63, false), "elsewhere scancode 63 is another key");
        assertFalse(KeyHint.macFn(-1, 62, MAC));
        assertFalse(KeyHint.macFn(340, 63, MAC), "a named key");
        assertTrue(KeyHint.openedWithFn(MAC, 1_000, 1_000 + KeyHint.FN_HELD_MS));
        assertFalse(KeyHint.openedWithFn(MAC, 1_000, 1_001 + KeyHint.FN_HELD_MS), "fn long before");
        assertFalse(KeyHint.openedWithFn(MAC, Long.MIN_VALUE / 2, 1_000), "fn never moved");
        assertFalse(KeyHint.openedWithFn(false, 1_000, 1_100), "not a Mac");
    }
}
