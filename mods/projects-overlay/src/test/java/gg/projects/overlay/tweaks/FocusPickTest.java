package gg.projects.overlay.tweaks;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/** DIALOG-FOCUS's rules over a made-up widget tree laid out as the game lays out a server window. */
class FocusPickTest {
    /** A widget: a text box, a holder of others (a scroll area clips), or anything else. */
    private static final class Node {
        final String name;
        final boolean box;
        final boolean clips;
        final int top;
        final int height;
        final List<Node> children = new ArrayList<>();
        boolean shown = true;
        boolean active = true;

        Node(String name, boolean box, boolean clips, int top, int height) {
            this.name = name;
            this.box = box;
            this.clips = clips;
            this.top = top;
            this.height = height;
        }

        Node add(Node... more) {
            children.addAll(List.of(more));
            return this;
        }

        @Override
        public String toString() {
            return name;
        }
    }

    private static final FocusPick.Tree<Node> TREE = new FocusPick.Tree<>() {
        @Override
        public List<? extends Node> children(Node node) {
            return node.children;
        }

        @Override
        public boolean shown(Node node) {
            return node.shown;
        }

        @Override
        public boolean textBox(Node node) {
            return node.box && node.active;
        }

        @Override
        public boolean clips(Node node) {
            return node.clips;
        }

        @Override
        public int top(Node node) {
            return node.top;
        }

        @Override
        public int bottom(Node node) {
            return node.top + node.height;
        }
    };

    private static Node box(String name, int top) {
        return new Node(name, true, false, top, 20);
    }

    private static Node other(String name, int top) {
        return new Node(name, false, false, top, 20);
    }

    private static Node scroll(int top, int height) {
        return new Node("scroll", false, true, top, height);
    }

    /** A form as the game lays it out: title, scroll area (text, check box, two labelled boxes), buttons. */
    private static Node form(Node scroll, Node first, Node second) {
        scroll.add(other("text", 40), other("check", 60), other("label", 84), first, other("label2", 118), second);
        return new Node("window", false, false, 0, 240).add(other("title", 12), scroll, other("ok", 210));
    }

    private static List<String> names(List<FocusPick.Box<Node>> boxes) {
        return boxes.stream().map(found -> found.box().name).toList();
    }

    @Test
    void findsTheBoxesInReadingOrderThroughTheScrollArea() {
        Node scroll = scroll(33, 174);
        Node window = form(scroll, box("name", 94), box("tag", 128));
        List<FocusPick.Box<Node>> boxes = FocusPick.boxes(window, TREE);
        assertEquals(List.of("name", "tag"), names(boxes));
        assertEquals(List.of(scroll), boxes.getFirst().holders());
    }

    @Test
    void skipsHiddenAndInactiveBoxesAndWhatHiddenHoldersHold() {
        Node hiddenBox = box("hidden", 94);
        hiddenBox.shown = false;
        Node inactive = box("inactive", 110);
        inactive.active = false;
        Node hiddenHolder = new Node("holder", false, false, 130, 40).add(box("inside", 140));
        hiddenHolder.shown = false;
        Node window = new Node("window", false, false, 0, 240)
            .add(scroll(33, 174).add(hiddenBox, inactive, hiddenHolder, box("last", 180)));
        assertEquals(List.of("last"), names(FocusPick.boxes(window, TREE)));
    }

    @Test
    void findsBoxesNestedInRowsAndNoneInAWindowWithout() {
        Node row = new Node("row", false, false, 60, 20).add(other("check", 60), box("code", 60));
        Node window = new Node("window", false, false, 0, 240).add(scroll(33, 174).add(other("text", 40), row));
        List<FocusPick.Box<Node>> boxes = FocusPick.boxes(window, TREE);
        assertEquals(List.of("code"), names(boxes));
        assertEquals(2, boxes.getFirst().holders().size());

        Node plain = new Node("window", false, false, 0, 240)
            .add(scroll(33, 174).add(other("text", 40), other("check", 60)), other("ok", 210));
        assertTrue(FocusPick.boxes(plain, TREE).isEmpty());
    }

    @Test
    void stopsAtADepthNoWindowReaches() {
        Node deepest = new Node("window", false, false, 0, 240);
        Node at = deepest;
        for (int i = 0; i < 40; i++) {
            Node inner = new Node("level" + i, false, false, 0, 240);
            at.add(inner);
            at = inner;
        }
        at.add(box("far", 10));
        assertTrue(FocusPick.boxes(deepest, TREE).isEmpty());
    }

    @Test
    void aBoxIsSeenWhenItsFirstLineShowsWithoutScrolling() {
        Node scroll = scroll(33, 174);
        Node window = form(scroll, box("name", 94), box("tag", 128));
        FocusPick.Box<Node> first = FocusPick.boxes(window, TREE).getFirst();
        assertTrue(FocusPick.seen(first, TREE, 240));
        // Its first line just inside the scroll area's bottom (207), the rest of it below.
        assertTrue(FocusPick.seen(new FocusPick.Box<>(box("cut", 193), List.of(scroll)), TREE, 240));
        // Below the fold: under a long text, past the scroll area.
        assertFalse(FocusPick.seen(new FocusPick.Box<>(box("low", 194), List.of(scroll)), TREE, 240));
        assertFalse(FocusPick.seen(new FocusPick.Box<>(box("far", 400), List.of(scroll)), TREE, 240));
        // Above the scroll area (scrolled past), or off the window.
        assertFalse(FocusPick.seen(new FocusPick.Box<>(box("above", 20), List.of(scroll)), TREE, 240));
        assertFalse(FocusPick.seen(new FocusPick.Box<>(box("off", 230), List.of()), TREE, 240));
        assertFalse(FocusPick.seen(new FocusPick.Box<>(box("negative", -5), List.of()), TREE, 240));
        // A holder that does not clip draws its children wherever they are.
        Node row = new Node("row", false, false, 0, 10);
        assertTrue(FocusPick.seen(new FocusPick.Box<>(box("beside", 100), List.of(row)), TREE, 240));
    }

    @Test
    void indexOfFindsTheFocusedBoxOrNone() {
        Node name = box("name", 94);
        Node tag = box("tag", 128);
        List<FocusPick.Box<Node>> boxes = FocusPick.boxes(form(scroll(33, 174), name, tag), TREE);
        assertEquals(0, FocusPick.indexOf(boxes, name));
        assertEquals(1, FocusPick.indexOf(boxes, tag));
        assertEquals(FocusPick.NONE, FocusPick.indexOf(boxes, other("ok", 210)));
        assertEquals(FocusPick.NONE, FocusPick.indexOf(boxes, null));
    }

    @Test
    void anOpeningWindowFocusesItsFirstBox() {
        // Nothing focused (the mouse brought the window).
        assertEquals(0, FocusPick.target(true, FocusPick.NONE, false, FocusPick.NONE, 2, true));
        // The game's own first focus after a key press (its warning button): the player gave it nothing yet.
        assertEquals(0, FocusPick.target(true, FocusPick.NONE, true, FocusPick.NONE, 2, true));
        // A box has it already.
        assertEquals(FocusPick.KEEP, FocusPick.target(true, 0, true, FocusPick.NONE, 2, true));
        assertEquals(FocusPick.KEEP, FocusPick.target(true, 1, true, FocusPick.NONE, 2, true));
        // The first box does not show without scrolling.
        assertEquals(FocusPick.KEEP, FocusPick.target(true, FocusPick.NONE, false, FocusPick.NONE, 2, false));
        // No box at all.
        assertEquals(FocusPick.KEEP, FocusPick.target(true, FocusPick.NONE, false, FocusPick.NONE, 0, true));
    }

    @Test
    void aLaterInitialisationNeverTakesThePlayersFocus() {
        // A resize: the game kept the player's focus (the second box, a button, anything).
        assertEquals(FocusPick.KEEP, FocusPick.target(false, 1, true, 1, 2, true));
        assertEquals(FocusPick.KEEP, FocusPick.target(false, FocusPick.NONE, true, FocusPick.NONE, 2, true));
        // The player had nothing focused: nothing new is focused.
        assertEquals(FocusPick.KEEP, FocusPick.target(false, FocusPick.NONE, false, FocusPick.NONE, 2, true));
        // The game built the window anew and lost the focus: the box the player had gets it back.
        assertEquals(1, FocusPick.target(false, FocusPick.NONE, false, 1, 2, true));
        assertEquals(0, FocusPick.target(false, FocusPick.NONE, false, 0, 2, false));
        // That box is gone.
        assertEquals(FocusPick.KEEP, FocusPick.target(false, FocusPick.NONE, false, 2, 2, true));
    }
}
