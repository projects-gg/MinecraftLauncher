package gg.projects.overlay.ui.draw;

import static org.junit.jupiter.api.Assertions.assertEquals;

import gg.projects.overlay.ui.Theme;
import org.junit.jupiter.api.Test;

/** The launcher's grass block, pixel for pixel (packages/ui content-icon.tsx). */
class PixelSpritesTest {
    @Test
    void theDeepDirtIsTheLaunchersOklabMix() {
        // color-mix(in oklab, gold-700 62%, obsidian-950), worked out apart from this code.
        assertEquals(0xFF573016, PixelSprites.DIRT);
    }

    @Test
    void theGrassBlockUsesTheGoldsAndLeavesItsCornersEmpty() {
        int[] px = PixelSprites.pixels(PixelSprites.Sprite.WORLD);
        assertEquals(16 * 16, px.length);
        assertEquals(0, px[0]);
        assertEquals(0, px[15 * 16 + 15]);
        assertEquals(Theme.GOLD_300, px[6]);
        assertEquals(Theme.GOLD_200, px[1 * 16 + 5]);
        assertEquals(PixelSprites.GOLD_700, px[6 * 16]);
        assertEquals(PixelSprites.DIRT, px[6 * 16 + 15]);
        for (int argb : px) {
            int alpha = argb >>> 24;
            assertEquals(true, alpha == 0 || alpha == 0xFF);
        }
    }
}
