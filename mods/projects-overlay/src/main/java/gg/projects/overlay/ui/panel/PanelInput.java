package gg.projects.overlay.ui.panel;

/**
 * Whether the player is using the keyboard or the mouse in the panel: the focus ring belongs to the
 * keyboard (as in the launcher, docs/design.md §8), so a click never leaves a gold box behind.
 */
public final class PanelInput {
    private static boolean keyboard;
    /** The panel is drawn behind a dialog over it (a report, a question): the keys are the dialog's. */
    private static boolean behind;
    /** A pointer the scenario driver sets (development runs only; never the OS pointer). */
    private static boolean pointer;
    private static int pointerX;
    private static int pointerY;

    private PanelInput() {
    }

    /** Keys are in use and the panel has them: never while it is drawn behind a dialog (no ring there). */
    public static boolean keyboardMode() {
        return keyboard && !behind;
    }

    /**
     * While {@code on}, the panel is being drawn behind a dialog: none of its controls shows the keyboard's ring (typing
     * a report's note drew a gold box around the reported friend's row; BETA-FB).
     */
    static void drawingBehind(boolean on) {
        behind = on;
    }

    /** The panel is being drawn behind a dialog. */
    static boolean behind() {
        return behind;
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

    /** The scenario driver's pointer is set (the chat's emoji bar reads it too, CHAT-EMOJI). */
    public static boolean pointerSet() {
        return pointer;
    }

    public static int pointerX() {
        return pointerX;
    }

    public static int pointerY() {
        return pointerY;
    }
}
