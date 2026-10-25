package gg.projects.overlay.emoji;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import org.junit.jupiter.api.Test;

class InsertionTest {
    private static final int MAX = 256;

    @Test
    void aCodeStandsApartFromTheWordsAroundIt() {
        assertEquals(":kalp: ", Insertion.around("", 0, 0, ":kalp:", MAX));
        assertEquals(" :kalp: ", Insertion.around("selam", 5, 5, ":kalp:", MAX));
        assertEquals(":kalp: ", Insertion.around("selam ", 6, 6, ":kalp:", MAX));
        // In the middle of a line, before a space: no second space.
        assertEquals(" :kalp:", Insertion.around("a b", 1, 1, ":kalp:", MAX));
        // At the start, before a word.
        assertEquals(":kalp: ", Insertion.around("selam", 0, 0, ":kalp:", MAX));
    }

    @Test
    void aSelectionIsReplaced() {
        // "abc" with "b" picked: the spaces follow the characters around the selection.
        assertEquals(" ❤ ", Insertion.around("abc", 1, 2, "❤", MAX));
        assertEquals("❤ ", Insertion.around("abc", 0, 3, "❤", MAX));
    }

    @Test
    void theLineNeverCutsACode() {
        String full = "x".repeat(MAX - 7);
        assertEquals(" :kalp:", Insertion.around(full, full.length(), full.length(), ":kalp:", MAX),
            "the space after goes first");
        String fuller = "x".repeat(MAX - 6);
        assertEquals(":kalp:", Insertion.around(fuller, fuller.length(), fuller.length(), ":kalp:", MAX));
        String fullest = "x".repeat(MAX - 5);
        assertNull(Insertion.around(fullest, fullest.length(), fullest.length(), ":kalp:", MAX));
        // A selection makes room.
        assertEquals("❤", Insertion.around(fullest, 0, 5, "❤", MAX).strip());
    }

    @Test
    void outOfRangeCaretsAreClamped() {
        assertEquals(" ❤ ", Insertion.around("ab", 9, 9, "❤", MAX));
        assertEquals("❤ ", Insertion.around("ab", -3, -3, "❤", MAX));
    }
}
