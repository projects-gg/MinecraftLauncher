package gg.projects.overlay.ui.panel;

/**
 * Whether the player is using the keyboard or the mouse in the panel: the focus ring belongs to the
 * keyboard (as in the launcher, docs/design.md §8), so a click never leaves a gold box behind.
 */
public final class PanelInput {
    private static boolean keyboard;

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
}
