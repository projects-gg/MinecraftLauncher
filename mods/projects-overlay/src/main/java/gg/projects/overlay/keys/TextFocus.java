package gg.projects.overlay.keys;

import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.ui.panel.OverlayScreen;
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
 * window is active and its screen's focused element is a visible text box (the chat, an anvil, a command block), a
 * visible text box of the screen has the keys without being its focus (the creative search), the screen edits text
 * itself (a sign, a book), the recipe book's search is focused (the inventory, the crafting table, the furnaces:
 * {@link RecipeSearch}), or the overlay's panel types (its own fields, the menu's name, the emoji search).
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
        if (screen instanceof OverlayScreen panel) {
            return panel.typesText();
        }
        if (screen instanceof AbstractSignEditScreen || screen instanceof BookEditScreen) {
            return true;
        }
        GuiEventListener focused = screen.getFocused();
        for (int depth = 0; focused != null && depth < MAX_DEPTH; depth++) {
            if (textBox(focused)) {
                return true;
            }
            focused = focused instanceof ContainerEventHandler container ? container.getFocused() : null;
        }
        for (GuiEventListener child : screen.children()) {
            if (child instanceof EditBox box && box.isVisible() && box.isFocused()) {
                return true;
            }
        }
        return RecipeSearch.typing(screen);
    }

    private static boolean textBox(GuiEventListener element) {
        if (element instanceof EditBox box) {
            return box.isVisible();
        }
        return element instanceof MultiLineEditBox area && area.visible;
    }
}
