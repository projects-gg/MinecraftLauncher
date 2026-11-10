package gg.projects.overlay.tweaks;

import java.util.ArrayList;
import java.util.List;

/**
 * DIALOG-FOCUS's rules, free of the game's classes so they are tested on their own ({@link DialogFocus} applies them to
 * a server window): the window's text boxes in reading order, whether the first one shows without scrolling, and what
 * an initialisation of the window does with the focus.
 */
final class FocusPick {
    /** {@link #target}: the focus stays as it is. */
    static final int KEEP = -1;
    /** {@link #indexOf}: none of the boxes. */
    static final int NONE = -1;
    /**
     * How far down a box its first line of text reaches (GUI pixels): a one-line box draws its text 6 to 14 below its
     * top, a box of several lines its first line 4 to 13.
     */
    static final int FIRST_LINE = 14;
    /** Deeper than any window lays out its widgets: the walk stops there. */
    private static final int MAX_DEPTH = 12;

    private FocusPick() {
    }

    /** How the walk sees a tree of widgets ({@link DialogFocus} reads the game's widgets through it). */
    interface Tree<N> {
        /** Its children, in the order the window lays them out; none for a widget that holds no others. */
        List<? extends N> children(N node);

        /** It is drawn (a hidden widget, and everything in it, is skipped). */
        boolean shown(N node);

        /** A text box of one line or several that can be typed in now (shown and active). */
        boolean textBox(N node);

        /** It draws its children only inside its own rectangle (a scroll area). */
        boolean clips(N node);

        /** The top of its rectangle on the screen (GUI pixels). */
        int top(N node);

        /** The bottom of its rectangle on the screen (GUI pixels). */
        int bottom(N node);
    }

    /** A text box and the widgets that hold it between the window and it, outermost first. */
    record Box<N>(N box, List<N> holders) {
    }

    /** The text boxes under {@code window}, in reading order: as the window lays out its rows, top to bottom. */
    static <N> List<Box<N>> boxes(N window, Tree<N> tree) {
        List<Box<N>> found = new ArrayList<>();
        walk(window, tree, new ArrayList<>(), found, 0);
        return found;
    }

    private static <N> void walk(N holder, Tree<N> tree, List<N> holders, List<Box<N>> found, int depth) {
        if (depth >= MAX_DEPTH) {
            return;
        }
        for (N child : tree.children(holder)) {
            if (child == null || !tree.shown(child)) {
                continue;
            }
            if (tree.textBox(child)) {
                found.add(new Box<>(child, List.copyOf(holders)));
                continue;
            }
            List<? extends N> inner = tree.children(child);
            if (!inner.isEmpty()) {
                holders.add(child);
                walk(child, tree, holders, found, depth + 1);
                holders.removeLast();
            }
        }
    }

    /**
     * Whether the box's first line shows without scrolling: inside every scroll area that holds it and inside the window
     * ({@code windowHeight} GUI pixels high). A box the player cannot see takes no focus: a key pressed by habit (E to
     * close) would land in it unseen.
     */
    static <N> boolean seen(Box<N> box, Tree<N> tree, int windowHeight) {
        int top = tree.top(box.box());
        int line = top + Math.min(Math.max(tree.bottom(box.box()) - top, 0), FIRST_LINE);
        if (top < 0 || line > windowHeight) {
            return false;
        }
        for (N holder : box.holders()) {
            if (tree.clips(holder) && (top < tree.top(holder) || line > tree.bottom(holder))) {
                return false;
            }
        }
        return true;
    }

    /** Which of {@code boxes} is {@code widget}, or {@link #NONE}. */
    static <N> int indexOf(List<Box<N>> boxes, N widget) {
        if (widget == null) {
            return NONE;
        }
        for (int i = 0; i < boxes.size(); i++) {
            if (boxes.get(i).box() == widget) {
                return i;
            }
        }
        return NONE;
    }

    /**
     * What a server window's initialisation does with the focus: the box to focus (its index in reading order), or
     * {@link #KEEP}.
     *
     * <p>As the window opens ({@code opening}) the player has given it nothing yet: the first box takes the focus,
     * also from the game's own first focus (the warning button when the window came after a key press), unless a box
     * has it already or the first box does not show without scrolling. A later initialisation (a resize, the window
     * shown again after its warning screen) never gives a new focus: the player's stays, and when the game built the
     * window anew and lost it, the box they had ({@code keptBox}) gets it back.
     *
     * @param opening this window initialises for the first time
     * @param focusedBox the box that has the focus now, or {@link #NONE}
     * @param anyFocus something has the focus now (a box, a button, the game's own first focus)
     * @param keptBox the box that had the focus before this initialisation, or {@link #NONE}
     * @param boxCount how many boxes the window has
     * @param firstSeen the first box shows without scrolling ({@link #seen})
     */
    static int target(boolean opening, int focusedBox, boolean anyFocus, int keptBox, int boxCount,
                      boolean firstSeen) {
        if (boxCount <= 0) {
            return KEEP;
        }
        if (opening) {
            return focusedBox == NONE && firstSeen ? 0 : KEEP;
        }
        if (anyFocus) {
            return KEEP;
        }
        return keptBox >= 0 && keptBox < boxCount ? keptBox : KEEP;
    }
}
