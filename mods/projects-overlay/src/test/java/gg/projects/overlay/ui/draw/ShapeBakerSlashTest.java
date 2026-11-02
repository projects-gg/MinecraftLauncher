package gg.projects.overlay.ui.draw;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.ui.Theme;
import java.util.Arrays;
import org.junit.jupiter.api.Test;

/** Phosphor's slash cut into an icon that has no slashed form (VOICE-KEYS: the headphones). */
class ShapeBakerSlashTest {
    private static float alpha(ShapeBaker.Image image, int x, int y) {
        return Theme.alpha(image.argb()[y * image.width() + x]) / 255f;
    }

    @Test
    void theLineIsDrawnAndAGapIsCutBesideIt() {
        // A full square, 32 pixels for the 256 view box: the slash runs (6, 6) to (26, 28).
        int[] full = new int[32 * 32];
        Arrays.fill(full, 0xFFFFFFFF);
        ShapeBaker.Image slashed = ShapeBaker.slashed(new ShapeBaker.Image(32, 32, full), 256f, 16f, 16f);
        assertEquals(32, slashed.width());
        // On the line: drawn.
        assertTrue(alpha(slashed, 15, 16) > 0.9f);
        // Two pixels beside it: in the gap, empty.
        assertEquals(0f, alpha(slashed, 17, 15));
        assertEquals(0f, alpha(slashed, 14, 18));
        // Far from it: the shape as it was.
        assertEquals(1f, alpha(slashed, 2, 30));
        assertEquals(1f, alpha(slashed, 30, 2));
    }

    @Test
    void anEmptyIconGetsTheLineAlone() {
        ShapeBaker.Image slashed = ShapeBaker.slashed(new ShapeBaker.Image(32, 32, new int[32 * 32]), 256f, 16f, 16f);
        assertTrue(alpha(slashed, 15, 16) > 0.9f);
        assertEquals(0f, alpha(slashed, 30, 2));
    }
}
