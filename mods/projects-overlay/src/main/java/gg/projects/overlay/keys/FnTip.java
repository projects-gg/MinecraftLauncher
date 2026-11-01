package gg.projects.overlay.keys;

/**
 * "Olmazsa [Fn] + [F7] tuşlarına bas" under a toast's key hint (F7-OPEN). On many laptops and compact keyboards the top
 * row is brightness, sound, media or flight-mode keys, and F1 to F12 reach the game only with Fn held (or, with Fn
 * Lock, the other way round: so the line says "if it does nothing", never just "press Fn"). "F7 ile arayüzü aç" did
 * nothing for those players. The line shows only where Fn can be what is missing: the key is one of F1 to F12, the
 * computer is a laptop or a Mac ({@link Laptop}), and the key never opened the panel on this computer (once it has,
 * the player's key works: never again); and on the first {@link #PER_SESSION} toasts of a game at most, so a player
 * who leaves the panel closed is not told it on every message. Pure.
 */
public final class FnTip {
    /** Toasts of one game that may carry the line. */
    public static final int PER_SESSION = 3;

    private int shown;

    /** Whether the next toast with a key hint carries the line. */
    public boolean wanted(boolean functionKey, boolean laptop, boolean keyUsed) {
        return functionKey && laptop && !keyUsed && shown < PER_SESSION;
    }

    /** A toast with the line went on screen (a toast updated in place counts again: the player saw it once more). */
    public void shown() {
        shown++;
    }
}
