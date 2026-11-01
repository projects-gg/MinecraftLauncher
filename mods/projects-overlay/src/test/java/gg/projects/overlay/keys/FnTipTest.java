package gg.projects.overlay.keys;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** F7-OPEN: "Olmazsa [Fn] + [F7] tuşlarına bas" shows only where Fn can be what is missing, and only a few times. */
class FnTipTest {
    @Test
    void onlyAnFKeyOnALaptopThatNeverOpenedThePanel() {
        FnTip tip = new FnTip();
        assertTrue(tip.wanted(true, true, false));
        assertFalse(tip.wanted(false, true, false), "a letter or a mouse button needs no Fn");
        assertFalse(tip.wanted(true, false, false), "a desktop keyboard gives F7 as it is");
        assertFalse(tip.wanted(true, true, true), "the key opened the panel here once: it works");
    }

    @Test
    void aGameShowsItOnTheFirstFewToastsOnly() {
        FnTip tip = new FnTip();
        for (int i = 0; i < FnTip.PER_SESSION; i++) {
            assertTrue(tip.wanted(true, true, false), "toast " + i);
            tip.shown();
        }
        assertFalse(tip.wanted(true, true, false), "a player who leaves the panel closed is not told it every time");
        // Asking is not showing: a toast that did not go up (the panel open, toasts off) leaves the count alone.
        FnTip fresh = new FnTip();
        for (int i = 0; i < 10; i++) {
            fresh.wanted(true, true, false);
        }
        assertTrue(fresh.wanted(true, true, false));
    }
}
