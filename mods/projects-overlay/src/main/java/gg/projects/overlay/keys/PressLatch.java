package gg.projects.overlay.keys;

/**
 * One toggle per press of the overlay key (F7-OPEN). A key held down repeats (the system's typematic repeat, from about
 * half a second on, some 30 times a second), and the game hands every repeat on as a press: a press held a little long
 * opened the panel, closed it, opened it again and let go wherever it stood. A toggle now holds the latch until the key
 * is up again, so the repeats of that press do nothing; the next press toggles again. Pure.
 */
public final class PressLatch {
    private boolean held;

    /** Whether a press of the key toggles the panel now (else it is the held press repeating). */
    public boolean accepts() {
        return !held;
    }

    /** The key toggled the panel; {@code down}: it is still held (a quick tap may be up already). */
    public void toggled(boolean down) {
        held = down;
    }

    /** The key is up: its release came, or a look at the keyboard found it up. */
    public void released() {
        held = false;
    }

    /** Whether the press that last toggled is still held. */
    public boolean held() {
        return held;
    }
}
