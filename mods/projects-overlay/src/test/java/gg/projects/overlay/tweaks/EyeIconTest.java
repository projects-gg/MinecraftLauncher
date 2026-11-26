package gg.projects.overlay.tweaks;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import org.junit.jupiter.api.Test;

/** PASSWORD-EYE's eye: both drawings the same size, fitting a text box, and drawn as runs of lit pixels. */
class EyeIconTest {
    @Test
    void bothEyesAreTheSameSizeAndFitATextBox() {
        assertEquals(EyeIcon.OPEN.length, EyeIcon.STRUCK.length);
        for (int y = 0; y < EyeIcon.H; y++) {
            assertEquals(EyeIcon.W, EyeIcon.OPEN[y].length());
            assertEquals(EyeIcon.W, EyeIcon.STRUCK[y].length());
        }
        // The game's text box is 20 high, its border and a pixel of room above and below.
        assertTrue(EyeIcon.H <= 20 - 2 * 4);
    }

    @Test
    void theStrikeRunsCornerToCornerWithRoomOnItsRight() {
        assertNotEquals(String.join("", EyeIcon.OPEN), String.join("", EyeIcon.STRUCK));
        assertEquals('#', EyeIcon.STRUCK[0].charAt(1));
        assertEquals('#', EyeIcon.STRUCK[8].charAt(9));
        assertEquals('.', EyeIcon.STRUCK[4].charAt(6));
    }

    @Test
    void runsCoverExactlyTheLitPixels() {
        List<EyeIcon.Run> runs = EyeIcon.runs(new String[] {"#..##", ".###."});
        assertEquals(List.of(new EyeIcon.Run(0, 0, 1), new EyeIcon.Run(3, 0, 2), new EyeIcon.Run(1, 1, 3)), runs);
        int lit = 0;
        for (String row : EyeIcon.OPEN) {
            lit += (int) row.chars().filter(c -> c == '#').count();
        }
        assertEquals(lit, EyeIcon.runs(EyeIcon.OPEN).stream().mapToInt(EyeIcon.Run::length).sum());
    }
}
