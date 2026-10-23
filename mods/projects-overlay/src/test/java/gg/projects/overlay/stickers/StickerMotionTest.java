package gg.projects.overlay.stickers;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** Every arrival motion starts and ends at rest and stays small (CHAT2 §8: once, transform only). */
class StickerMotionTest {
    private static final String[] PATTERNS = {"bounce", "wobble", "beat", "shake", "shiver", "wave", "nod", "sway",
        "float", "pop", "tilt", "swing", "sink", "glitch", "unknown"};

    @Test
    void everyPatternStartsAndEndsAtRest() {
        float[] pose = new float[StickerMotion.SIZE];
        for (String p : PATTERNS) {
            for (float t : new float[] {0f, 1f, -0.5f, 1.5f}) {
                StickerMotion.at(p, t, 48, pose);
                assertRest(p + " at " + t, pose);
            }
            // Close to the ends it is close to rest too: no jump when the motion starts or stops.
            StickerMotion.at(p, 0.995f, 48, pose);
            assertTrue(Math.abs(pose[StickerMotion.DX]) < 0.6f && Math.abs(pose[StickerMotion.DY]) < 0.6f, p);
            assertTrue(Math.abs(pose[StickerMotion.ANGLE]) < 1.5f, p);
            assertTrue(Math.abs(pose[StickerMotion.SX] - 1f) < 0.03f, p);
        }
    }

    @Test
    void motionsStaySmallAndMoveSomewhere() {
        float[] pose = new float[StickerMotion.SIZE];
        for (String p : PATTERNS) {
            float moved = 0f;
            for (int i = 1; i < 100; i++) {
                StickerMotion.at(p, i / 100f, 48, pose);
                assertTrue(Math.abs(pose[StickerMotion.DX]) <= 3.01f, p);
                assertTrue(Math.abs(pose[StickerMotion.DY]) <= 6.01f, p);
                assertTrue(Math.abs(pose[StickerMotion.ANGLE]) <= 14.01f, p);
                assertTrue(pose[StickerMotion.SX] >= 0.9f && pose[StickerMotion.SX] <= 1.23f, p);
                assertTrue(pose[StickerMotion.SY] >= 0.9f && pose[StickerMotion.SY] <= 1.23f, p);
                moved = Math.max(moved, Math.abs(pose[StickerMotion.DX]) + Math.abs(pose[StickerMotion.DY])
                    + Math.abs(pose[StickerMotion.ANGLE]) + Math.abs(pose[StickerMotion.SX] - 1f) * 10f);
            }
            assertTrue(moved > 0.5f, p + " moves");
            assertTrue(StickerMotion.duration(p) >= 520 && StickerMotion.duration(p) <= 1_100, p);
        }
    }

    @Test
    void amplitudesFollowTheSize() {
        float[] big = new float[StickerMotion.SIZE];
        float[] small = new float[StickerMotion.SIZE];
        StickerMotion.at("shake", 0.08f, 48, big);
        StickerMotion.at("shake", 0.08f, 24, small);
        assertEquals(big[StickerMotion.DX] / 2f, small[StickerMotion.DX], 1e-4f);
    }

    private static void assertRest(String what, float[] pose) {
        assertEquals(0f, pose[StickerMotion.DX], 1e-6f, what);
        assertEquals(0f, pose[StickerMotion.DY], 1e-6f, what);
        assertEquals(1f, pose[StickerMotion.SX], 1e-6f, what);
        assertEquals(1f, pose[StickerMotion.SY], 1e-6f, what);
        assertEquals(0f, pose[StickerMotion.ANGLE], 1e-6f, what);
    }
}
