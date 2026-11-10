package gg.projects.overlay.tweaks;

import java.util.List;
import java.util.Map;
import java.util.WeakHashMap;
import net.fabricmc.fabric.api.client.screen.v1.ScreenEvents;
import net.minecraft.client.gui.ComponentPath;
import net.minecraft.client.gui.components.AbstractScrollArea;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.components.MultiLineEditBox;
import net.minecraft.client.gui.components.events.ContainerEventHandler;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.dialog.DialogScreen;

/**
 * DIALOG-FOCUS: a server window (the game's dialogs, {@link DialogScreen} and its kinds) with a text box opens with its
 * first box ready to type in, the caret where the game puts it, instead of waiting for a click. The window's own
 * layout holds the boxes in a scroll area, so the focus goes down the whole path (window, scroll area, box), the way the
 * game moves it on a click or Tab: the box shows its caret and the game's text input follows it.
 *
 * <p>Only as the window opens, and only when its first box shows without scrolling ({@link FocusPick#seen}); a window
 * without a text box is left alone. The game keeps the widgets when the window is resized or shown again (after its
 * warning screen), and with them the player's focus, which this never takes; should a game version build the window
 * anew there, the box the player had gets the focus back. Enter, the buttons and the rest of the window are the game's.
 * Only the Fabric API's screen events: no mixin, so a build for a new Minecraft version needs nothing more.
 */
final class DialogFocus implements ClientTweak {
    /** Its id ({@link ClientTweaks#PROPERTY}; the launcher's `pl_overlay::tweaks::DIALOG_FOCUS`). */
    static final String ID = "dialog-focus";

    /** How the rules see the game's widgets. */
    private static final FocusPick.Tree<GuiEventListener> TREE = new FocusPick.Tree<>() {
        @Override
        public List<? extends GuiEventListener> children(GuiEventListener node) {
            return node instanceof ContainerEventHandler holder ? holder.children() : List.of();
        }

        @Override
        public boolean shown(GuiEventListener node) {
            return !(node instanceof AbstractWidget widget) || widget.visible;
        }

        @Override
        public boolean textBox(GuiEventListener node) {
            return (node instanceof EditBox || node instanceof MultiLineEditBox) && ((AbstractWidget) node).isActive();
        }

        @Override
        public boolean clips(GuiEventListener node) {
            return node instanceof AbstractScrollArea;
        }

        @Override
        public int top(GuiEventListener node) {
            return node.getRectangle().top();
        }

        @Override
        public int bottom(GuiEventListener node) {
            return node.getRectangle().bottom();
        }
    };

    /** Deeper than any window nests its focus: the search for the focused widget stops there. */
    private static final int MAX_DEPTH = 12;

    /**
     * The server windows initialised in this game (render thread), each with the box that had the focus right before its
     * latest initialisation ({@link FocusPick#NONE}: none). Gone with their windows.
     */
    private final Map<Screen, Integer> windows = new WeakHashMap<>();

    @Override
    public String id() {
        return ID;
    }

    @Override
    public void install(TweakGuard guard) {
        // Fabric calls these around every initialisation of every screen, a resize too; the guard runs the check of the
        // screen's kind as well, so a game version without these classes only turns this tweak off.
        ScreenEvents.BEFORE_INIT.register((mc, screen, width, height) -> guard.run(() -> beforeInit(screen)));
        ScreenEvents.AFTER_INIT.register((mc, screen, width, height) -> guard.run(() -> afterInit(screen, height)));
    }

    /** A window shown before initialises again: which box the player had, should the game build it anew. */
    private void beforeInit(Screen screen) {
        if (!(screen instanceof DialogScreen<?>) || !windows.containsKey(screen)) {
            return;
        }
        windows.put(screen, FocusPick.indexOf(FocusPick.boxes(screen, TREE), focusedLeaf(screen)));
    }

    private void afterInit(Screen screen, int height) {
        if (!(screen instanceof DialogScreen<?>)) {
            return;
        }
        Integer kept = windows.put(screen, FocusPick.NONE);
        List<FocusPick.Box<GuiEventListener>> boxes = FocusPick.boxes(screen, TREE);
        if (boxes.isEmpty()) {
            return;
        }
        boolean opening = kept == null;
        int target = FocusPick.target(
            opening,
            FocusPick.indexOf(boxes, focusedLeaf(screen)),
            screen.getFocused() != null,
            opening ? FocusPick.NONE : kept,
            boxes.size(),
            FocusPick.seen(boxes.getFirst(), TREE, height));
        if (target != FocusPick.KEEP) {
            focus(screen, boxes.get(target));
        }
    }

    /** The widget at the end of the window's focus path (a box, a button), or null. */
    private static GuiEventListener focusedLeaf(ContainerEventHandler window) {
        GuiEventListener at = window.getFocused();
        for (int depth = 0; depth < MAX_DEPTH && at instanceof ContainerEventHandler holder; depth++) {
            GuiEventListener next = holder.getFocused();
            if (next == null) {
                break;
            }
            at = next;
        }
        return at;
    }

    /**
     * Focuses {@code box} as the game itself moves the focus ({@code Screen.changeFocus}): the old path lets go first
     * (a box there loses its caret), then every holder on the way to the box takes its child.
     */
    private static void focus(Screen screen, FocusPick.Box<GuiEventListener> box) {
        ComponentPath path = ComponentPath.leaf(box.box());
        List<GuiEventListener> holders = box.holders();
        for (int i = holders.size() - 1; i >= 0; i--) {
            path = ComponentPath.path((ContainerEventHandler) holders.get(i), path);
        }
        path = ComponentPath.path(screen, path);
        screen.clearFocus();
        path.applyFocus(true);
    }
}
