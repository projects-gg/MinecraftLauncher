package gg.projects.overlay.ui.panel;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import org.junit.jupiter.api.Test;

/** The bubble's founding month ("Kuruluş Eylül 2026") from what the launcher sends (TEAM6). */
class BadgeBubbleTest {
    @Test
    void theFoundingMonthReadsOnlyAsYearAndMonth() {
        assertArrayEquals(new int[] {2026, 9}, BadgeBubble.yearMonth("2026-09"));
        assertArrayEquals(new int[] {2025, 12}, BadgeBubble.yearMonth("2025-12"));
        assertNull(BadgeBubble.yearMonth("2026-13"));
        assertNull(BadgeBubble.yearMonth("2026-00"));
        assertNull(BadgeBubble.yearMonth("2026-9"));
        assertNull(BadgeBubble.yearMonth("2026-09-14"), "the launcher sends the month only");
        assertNull(BadgeBubble.yearMonth(""));
        assertNull(BadgeBubble.yearMonth(null));
    }
}
