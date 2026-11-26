package gg.projects.overlay.tweaks;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/** A tweak's fail-safe: its first failure turns it off for the game, said once; the game never hears of it. */
class TweakGuardTest {
    @Test
    void runsStepsWhileNothingFailed() {
        List<String> reported = new ArrayList<>();
        TweakGuard guard = new TweakGuard("dialog-focus", (id, problem) -> reported.add(id));
        int[] runs = {0};
        guard.run(() -> runs[0]++);
        guard.run(() -> runs[0]++);
        assertEquals(2, runs[0]);
        assertTrue(guard.on());
        assertEquals("dialog-focus", guard.id());
        assertTrue(reported.isEmpty());
    }

    @Test
    void theFirstFailureTurnsTheTweakOffAndIsReportedOnce() {
        List<Throwable> reported = new ArrayList<>();
        TweakGuard guard = new TweakGuard("dialog-focus", (id, problem) -> reported.add(problem));
        IllegalStateException bug = new IllegalStateException("a bug");
        guard.run(() -> {
            throw bug;
        });
        assertFalse(guard.on());
        int[] runs = {0};
        guard.run(() -> runs[0]++);
        assertEquals(0, runs[0]);
        assertEquals(List.of(bug), reported);
    }

    @Test
    void aGameVersionThatMovedAClassOnlyTurnsTheTweakOff() {
        List<Throwable> reported = new ArrayList<>();
        TweakGuard guard = new TweakGuard("dialog-focus", (id, problem) -> reported.add(problem));
        guard.run(() -> {
            throw new NoSuchMethodError("Screen.clearFocus");
        });
        assertFalse(guard.on());
        assertEquals(1, reported.size());
    }

    @Test
    void theVirtualMachinesOwnFailuresGoOnUp() {
        TweakGuard guard = new TweakGuard("dialog-focus", (id, problem) -> {
        });
        OutOfMemoryError full = new OutOfMemoryError("test");
        assertSame(full, assertThrows(OutOfMemoryError.class, () -> guard.run(() -> {
            throw full;
        })));
    }

    @Test
    void aFailingLogStillTurnsTheTweakOff() {
        TweakGuard guard = new TweakGuard("dialog-focus", (id, problem) -> {
            throw new IllegalStateException("the log is gone");
        });
        guard.run(() -> {
            throw new IllegalArgumentException("a bug");
        });
        assertFalse(guard.on());
    }

    @Test
    void aStepWithAnAnswerFallsBackWhenItFailsOrTheTweakIsOff() {
        List<String> reported = new ArrayList<>();
        TweakGuard guard = new TweakGuard("tooltip-fit", (id, problem) -> reported.add(id));
        assertEquals("placed", guard.call(() -> "placed", "game"));
        assertEquals("game", guard.call(() -> {
            throw new IllegalStateException("a version moved a field");
        }, "game"));
        assertFalse(guard.on());
        assertEquals(List.of("tooltip-fit"), reported);
        // Off now: the step does not run at all.
        int[] runs = {0};
        assertEquals("game", guard.call(() -> {
            runs[0]++;
            return "placed";
        }, "game"));
        assertEquals(0, runs[0]);
        assertThrows(OutOfMemoryError.class, () -> new TweakGuard("x", (id, problem) -> { }).call(() -> {
            throw new OutOfMemoryError("fatal");
        }, "game"));
    }
}
