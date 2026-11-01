package gg.projects.overlay.keys;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/**
 * F7-OPEN: one toggle per press of the overlay key. A held key repeats and the game hands every repeat on as a press;
 * these play the presses the way the game hands them to {@code OverlayClient} (a press, its repeats, its release).
 */
class PressLatchTest {
    /** The panel as the overlay key leaves it: each press that the latch accepts toggles it. */
    private static final class Panel {
        final PressLatch latch = new PressLatch();
        boolean open;
        int toggles;

        /** A press (or a repeat) of the key; {@code down}: the key is still held when the toggle looks. */
        void press(boolean down) {
            if (latch.accepts()) {
                open = !open;
                toggles++;
                latch.toggled(down);
            }
        }

        void release() {
            latch.released();
        }
    }

    @Test
    void aHeldKeyOpensThePanelOnceAndItsRepeatsLeaveItOpen() {
        Panel panel = new Panel();
        panel.press(true);
        for (int repeat = 0; repeat < 40; repeat++) {
            // About a second and a half of the system's key repeat.
            panel.press(true);
        }
        assertTrue(panel.open, "the panel stays open while the key is held");
        assertEquals(1, panel.toggles);
        panel.release();
        panel.press(true);
        assertFalse(panel.open, "the next press closes it");
        for (int repeat = 0; repeat < 40; repeat++) {
            panel.press(true);
        }
        assertFalse(panel.open, "and its repeats do not open it again");
        assertEquals(2, panel.toggles);
    }

    @Test
    void quickTapsToggleEveryTime() {
        Panel panel = new Panel();
        // A tap is often up before the tick that opens the panel: nothing is held then.
        panel.press(false);
        assertTrue(panel.open);
        assertTrue(panel.latch.accepts());
        panel.press(false);
        assertFalse(panel.open);
        panel.press(true);
        panel.release();
        panel.press(true);
        panel.release();
        assertFalse(panel.open);
        assertEquals(4, panel.toggles);
    }

    @Test
    void aReleaseSeenLateStillFreesTheNextPress() {
        // The release went to another screen (the panel closed back to the pause menu): the tick's look at the
        // keyboard finds the key up and lets go.
        Panel panel = new Panel();
        panel.press(true);
        assertTrue(panel.latch.held());
        panel.latch.released();
        assertFalse(panel.latch.held());
        panel.press(true);
        assertFalse(panel.open);
    }
}
