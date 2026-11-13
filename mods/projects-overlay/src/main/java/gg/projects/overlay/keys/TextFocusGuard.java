package gg.projects.overlay.keys;

/**
 * SQOL-O: what the launcher is told about a text field in the game having the keyboard (the game's side of its
 * {@code voice_typing} guard: its single-key voice shortcuts rest meanwhile, so typing "v" in the chat never opens
 * push-to-talk and "m" never mutes). Nothing is said until the launcher asks ({@code keys-guard}). Gaining the
 * keyboard is said at once (the first letters come right after a chat opens); losing it only after
 * {@link #OFF_DELAY_MS}, so a focus that moves between fields or screens says nothing. While it lasts it is said again
 * every {@link #REFRESH_MS}: the launcher lets a word go after 45 s, so a lost "off" never leaves its shortcuts dead.
 * No game classes: tested on its own.
 */
public final class TextFocusGuard {
    /** Focus lost for this long before the launcher hears it. */
    public static final long OFF_DELAY_MS = 250;
    /** Said again this often while a field keeps the keyboard (the launcher's hold is 45 s). */
    public static final long REFRESH_MS = 20_000;

    private boolean asked;
    private boolean said;
    private long saidAt;
    private long lostAt = -1;

    /** The launcher asked to hear it ({@code keys-guard}). */
    public void ask() {
        asked = true;
    }

    /** The launcher went away: nothing is said until a new one asks (it forgets the word itself). */
    public void reset() {
        asked = false;
        said = false;
        lostAt = -1;
    }

    /** Whether the launcher was told a field has the keyboard (and not told otherwise since). */
    public boolean said() {
        return said;
    }

    /**
     * Looks at the game now: {@code focused}, a text field has the keyboard while the game's window is active.
     *
     * @return the word to send ({@code TRUE}: it has the keyboard, {@code FALSE}: not any more), or null for nothing
     */
    public Boolean look(boolean focused, long nowMs) {
        if (!asked) {
            return null;
        }
        if (focused) {
            lostAt = -1;
            if (!said || nowMs - saidAt >= REFRESH_MS) {
                said = true;
                saidAt = nowMs;
                return Boolean.TRUE;
            }
            return null;
        }
        if (!said) {
            return null;
        }
        if (lostAt < 0) {
            lostAt = nowMs;
        }
        if (nowMs - lostAt < OFF_DELAY_MS) {
            return null;
        }
        said = false;
        lostAt = -1;
        return Boolean.FALSE;
    }
}
