package gg.projects.overlay.tweaks;

import java.util.function.BiConsumer;

/**
 * One tweak's fail-safe ({@link ClientTweaks}): every step of the tweak runs through {@link #run}, and the first failure
 * turns the tweak off for the rest of the game, said once in the log. The game, the overlay and the other tweaks go on
 * as if it had never been there. Steps run on the game's render thread; the switch is read from any thread.
 */
public final class TweakGuard {
    private final String id;
    /** Hears the one failure: the tweak's id and what went wrong. */
    private final BiConsumer<String, Throwable> report;
    private volatile boolean off;

    TweakGuard(String id, BiConsumer<String, Throwable> report) {
        this.id = id;
        this.report = report;
    }

    /** The tweak's id ({@link ClientTweak#id()}). */
    public String id() {
        return id;
    }

    /** The tweak still runs: nothing of it has failed. */
    public boolean on() {
        return !off;
    }

    /**
     * Runs one step of the tweak while it is on. Whatever the step throws (a bug, or a game version that moved what it
     * relies on) turns the tweak off; only the virtual machine's own failures (out of memory) go on up.
     */
    public void run(Runnable step) {
        if (off) {
            return;
        }
        try {
            step.run();
        } catch (VirtualMachineError fatal) {
            throw fatal;
        } catch (Throwable problem) {
            fail(problem);
        }
    }

    private void fail(Throwable problem) {
        boolean first;
        synchronized (this) {
            first = !off;
            off = true;
        }
        if (first) {
            try {
                report.accept(id, problem);
            } catch (RuntimeException ignored) {
                // The log itself failed: the tweak is off all the same.
            }
        }
    }
}
