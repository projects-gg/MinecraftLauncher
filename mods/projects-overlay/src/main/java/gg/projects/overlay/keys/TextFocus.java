package gg.projects.overlay.keys;

import gg.projects.overlay.compat.Compat;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.components.MultiLineEditBox;
import net.minecraft.client.gui.components.events.ContainerEventHandler;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.AbstractSignEditScreen;
import net.minecraft.client.gui.screens.inventory.BookEditScreen;

/**
 * SQOL-O: whether a text field in the game has the keyboard now ({@link TextFocusGuard} tells the launcher): the game's
 * window is active and its screen's focused element is a text box (the chat, an anvil, a command block, the creative
 * search, the overlay's own fields), or the screen edits text itself (a sign, a book).
 */
public final class TextFocus {
    /** Nested containers walked at most for the focused element. */
    private static final int MAX_DEPTH = 8;

    private TextFocus() {
    }

    public static boolean now(Minecraft mc) {
        return mc.isWindowActive() && typing(Compat.screen(mc));
    }

    static boolean typing(Screen screen) {
        if (screen == null) {
            return false;
        }
        if (screen instanceof AbstractSignEditScreen || screen instanceof BookEditScreen) {
            return true;
        }
        GuiEventListener focused = screen.getFocused();
        for (int depth = 0; focused != null && depth < MAX_DEPTH; depth++) {
            if (focused instanceof EditBox || focused instanceof MultiLineEditBox) {
                return true;
            }
            focused = focused instanceof ContainerEventHandler container ? container.getFocused() : null;
        }
        return false;
    }
}
