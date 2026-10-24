package gg.projects.overlay.ui.panel;

/**
 * Whether the player is using the keyboard or the mouse in the panel: the focus ring belongs to the
 * keyboard (as in the launcher, docs/design.md §8), so a click never leaves a gold box behind.
 */
public final class PanelInput {
    private static boolean keyboard;
    /** A pointer the scenario driver sets (development runs only; never the OS pointer). */
    private static boolean pointer;
    private static int pointerX;
    private static int pointerY;

    private PanelInput() {
    }

    public static boolean keyboardMode() {
        return keyboard;
    }

    static void usedKeyboard() {
        keyboard = true;
    }

    static void usedMouse() {
        keyboard = false;
    }

    /**
     * Lets the scenario driver rest a pointer on the panel (a tag's chip, to open its bubble) without moving the
     * player's own: the panel draws as if the mouse were at {@code x, y} (GUI pixels) until {@link #clearPointer()}.
     */
    public static void pointAt(int x, int y) {
        pointer = true;
        pointerX = x;
        pointerY = y;
        keyboard = false;
    }

    public static void clearPointer() {
        pointer = false;
    }

    static boolean pointerSet() {
        return pointer;
    }

    static int pointerX() {
        return pointerX;
    }

    static int pointerY() {
        return pointerY;
    }
}
