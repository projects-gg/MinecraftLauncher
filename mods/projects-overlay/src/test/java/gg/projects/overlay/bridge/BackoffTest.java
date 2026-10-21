package gg.projects.overlay.bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class BackoffTest {
    @Test
    void growsAndCapsAtAMinute() {
        assertEquals(1_000, Backoff.delayMs(0, 0));
        assertEquals(2_000, Backoff.delayMs(1, 0));
        assertEquals(60_000, Backoff.delayMs(6, 0));
        assertEquals(60_000, Backoff.delayMs(500, 0));
        assertEquals(1_000, Backoff.delayMs(-3, 0));
    }

    @Test
    void jitterStaysWithinTwentyPercent() {
        assertEquals(800, Backoff.delayMs(0, -1));
        assertEquals(1_200, Backoff.delayMs(0, 1));
        long wild = Backoff.delayMs(0, 7);
        assertTrue(wild <= 1_200);
    }
}
