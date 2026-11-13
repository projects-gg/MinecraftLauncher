package gg.projects.overlay.keys;

import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.recipebook.RecipeBookComponent;

/**
 * SQOL-O: the recipe book's search (the inventory, the crafting table, the furnaces) has the keys while its text box is
 * focused. The book is one of its screen's children but keeps that box to itself (its own {@code isFocused()} is always
 * false), so the box is read by its name, as 26.1 to 26.3 call it; a version that names it otherwise is not seen.
 */
final class RecipeSearch {
    /** The book's {@code searchBox}; none when this version has no such field. */
    private static final VarHandle SEARCH_BOX = searchBox();

    private RecipeSearch() {
    }

    /** Whether this version's recipe book can be read (its search box is where it is expected). */
    static boolean known() {
        return SEARCH_BOX != null;
    }

    /** Whether a visible recipe book of {@code screen} has its search focused. */
    static boolean typing(Screen screen) {
        if (SEARCH_BOX == null) {
            return false;
        }
        for (GuiEventListener child : screen.children()) {
            if (child instanceof RecipeBookComponent<?> book && book.isVisible() && searching(book)) {
                return true;
            }
        }
        return false;
    }

    private static boolean searching(RecipeBookComponent<?> book) {
        Object box;
        try {
            box = SEARCH_BOX.get(book);
        } catch (RuntimeException e) {
            return false;
        }
        // None before the book was first opened.
        return box instanceof EditBox search && search.isVisible() && search.isFocused();
    }

    private static VarHandle searchBox() {
        try {
            return MethodHandles.privateLookupIn(RecipeBookComponent.class, MethodHandles.lookup())
                .findVarHandle(RecipeBookComponent.class, "searchBox", EditBox.class);
        } catch (ReflectiveOperationException | RuntimeException | LinkageError e) {
            return null;
        }
    }
}
