package gg.projects.overlay.stickers;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** The atlas baked at the size it is drawn: colours kept, the cut edge without a dark fringe, cells apart. */
class StickerBakerTest {
    private static final int TILE = 16;

    /** A 2 x 1 atlas: a gold square on a clear tile, and a half-clear blue tile. */
    private static int[] atlas() {
        int[] px = new int[2 * TILE * TILE];
        for (int y = 0; y < TILE; y++) {
            for (int x = 0; x < TILE; x++) {
                boolean inside = x >= 4 && x < 12 && y >= 4 && y < 12;
                px[y * 2 * TILE + x] = inside ? 0xFFE89A2C : 0x00000000;
                px[y * 2 * TILE + TILE + x] = 0x800000FF;
            }
        }
        return px;
    }

    @Test
    void aSmallerSizeAveragesWithoutDarkening() {
        StickerBaker.Sheet sheet = StickerBaker.bake(atlas(), 2 * TILE, TILE, 2, 1, 8);
        assertEquals(2 * (8 + 2), sheet.width());
        assertEquals(8 + 2, sheet.height());
        int centre = sheet.argb()[(sheet.cellY(0) + 4) * sheet.width() + sheet.cellX(0) + 4];
        assertEquals(0xFFE89A2C, centre, "inside the square: the gold itself");
        // On the square's edge the picture thins out, but its colour stays gold (premultiplied alpha).
        int edge = sheet.argb()[(sheet.cellY(0) + 2) * sheet.width() + sheet.cellX(0) + 2];
        assertEquals(0xFF, (edge >>> 24) & 0xFF);
        int corner = sheet.argb()[(sheet.cellY(0) + 1) * sheet.width() + sheet.cellX(0) + 1];
        assertEquals(0, (corner >>> 24) & 0xFF, "outside it stays clear");
        // A half-clear tile keeps its alpha and its colour.
        int blue = sheet.argb()[(sheet.cellY(0) + 3) * sheet.width() + sheet.cellX(1) + 3];
        assertEquals(0x800000FF, blue);
        // The pixel around each cell is clear: no neighbour bleeds in under filtering.
        for (int y = 0; y < sheet.height(); y++) {
            assertEquals(0, sheet.argb()[y * sheet.width()] >>> 24);
            assertEquals(0, sheet.argb()[y * sheet.width() + sheet.cellX(1) - 1] >>> 24);
        }
    }

    @Test
    void anEdgePixelMixesOnlyWithWhatIsThere() {
        // Half of the source area under one pixel is gold, half clear: half alpha, full gold.
        int[] px = new int[2 * 2];
        px[0] = 0xFFE89A2C;
        px[1] = 0x00000000;
        px[2] = 0xFFE89A2C;
        px[3] = 0x00000000;
        int[] out = new int[3 * 3];
        StickerBaker.resample(px, 2, 0, 0, 2, out, 3, 1, 1, 1);
        int mixed = out[1 * 3 + 1];
        assertEquals(128, (mixed >>> 24) & 0xFF, 1);
        assertEquals(0xE89A2C, mixed & 0xFFFFFF);
    }

    @Test
    void aLargerSizeBlendsTheNearest() {
        StickerBaker.Sheet sheet = StickerBaker.bake(atlas(), 2 * TILE, TILE, 2, 1, 32);
        int centre = sheet.argb()[(sheet.cellY(0) + 16) * sheet.width() + sheet.cellX(0) + 16];
        assertEquals(0xFFE89A2C, centre);
        int clear = sheet.argb()[(sheet.cellY(0) + 1) * sheet.width() + sheet.cellX(0) + 1];
        assertTrue(((clear >>> 24) & 0xFF) < 8);
    }
}
