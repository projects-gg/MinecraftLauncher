package gg.projects.overlay.motion;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class MotionTest {
    @Test
    void springSettlesAtItsTargetWithoutLosingItsWay() {
        Spring s = Spring.snappy(0f);
        s.setTarget(100f);
        float peak = 0f;
        for (int i = 0; i < 240; i++) {
            s.step(1f / 120f);
            peak = Math.max(peak, s.value());
        }
        assertEquals(100f, s.value(), 0.001f);
        assertTrue(s.atRest());
        // The launcher's sliding markers overshoot by a hair at most.
        assertTrue(peak < 102.5f, "overshoot " + peak);
    }

    @Test
    void springGivesTheSameMotionForAnyFrameRate() {
        Spring fast = Spring.snappy(0f);
        Spring slow = Spring.snappy(0f);
        fast.setTarget(50f);
        slow.setTarget(50f);
        for (int i = 0; i < 24; i++) {
            fast.step(1f / 240f);
        }
        for (int i = 0; i < 6; i++) {
            slow.step(1f / 60f);
        }
        assertEquals(fast.value(), slow.value(), 0.01f);
    }

    @Test
    void retargetingKeepsPositionAndSpeed() {
        Spring s = Spring.snappy(0f);
        s.setTarget(100f);
        s.step(0.05f);
        float position = s.value();
        float speed = s.velocity();
        s.setTarget(-20f);
        assertEquals(position, s.value());
        assertEquals(speed, s.velocity());
        // Still moving forwards for a moment: an interrupted motion turns around smoothly.
        s.step(0.001f);
        assertTrue(s.value() > position);
    }

    @Test
    void aLongFrameNeverOvershootsWildly() {
        Spring s = Spring.gentle(0f);
        s.setTarget(10f);
        s.step(5f);
        assertEquals(10f, s.value(), 0.001f);
    }

    @Test
    void criticallyAndOverDampedSpringsArrive() {
        Spring critical = new Spring(100f, 20f, 0f);
        Spring over = new Spring(100f, 60f, 0f);
        critical.setTarget(1f);
        over.setTarget(1f);
        for (int i = 0; i < 600; i++) {
            critical.step(1f / 60f);
            over.step(1f / 60f);
        }
        assertEquals(1f, critical.value(), 0.001f);
        assertEquals(1f, over.value(), 0.001f);
    }

    @Test
    void easeCurvesStartAtZeroAndEndAtOne() {
        for (Ease e : new Ease[] {Ease.OUT_EXPO, Ease.DRAWER, Ease.OUT, Ease.LINEAR}) {
            assertEquals(0f, e.at(0f));
            assertEquals(1f, e.at(1f));
            float previous = 0f;
            for (int i = 1; i <= 100; i++) {
                float v = e.at(i / 100f);
                assertTrue(v >= previous - 1e-4f, "not monotonic");
                previous = v;
            }
        }
        assertEquals(0.5f, Ease.LINEAR.at(0.5f), 0.002f);
        // An ease-out is well past halfway at half time.
        assertTrue(Ease.OUT_EXPO.at(0.5f) > 0.85f);
    }

    @Test
    void tweenTurnsAroundFromWhereItIs() {
        Tween t = new Tween(0f);
        t.animateTo(1f, 0L, 200L, Ease.LINEAR);
        assertEquals(0.5f, t.value(100L), 0.01f);
        t.animateTo(0f, 100L, 200L, Ease.LINEAR);
        assertEquals(0.5f, t.value(100L), 0.01f);
        assertEquals(0.25f, t.value(200L), 0.01f);
        assertTrue(t.done(300L));
        assertFalse(t.done(250L));
    }

    @Test
    void tweenToItsOwnTargetChangesNothing() {
        Tween t = new Tween(0f);
        t.animateTo(1f, 0L, 200L, Ease.LINEAR);
        t.animateTo(1f, 150L, 200L, Ease.LINEAR);
        assertTrue(t.done(200L));
    }
}
