package gg.projects.overlay.emoji;

/**
 * What the picker writes into the chat line at the caret (pure, so it is tested on its own): the piece, with a space before
 * it when the character before is not one (a code glued to a word would still work on the server, but reads badly) and a
 * space after it so the next word or emoji can follow (the game trims the line when it is sent). The line holds at most
 * {@code max} characters (the chat's 256): the spaces go first, and a piece that does not fit at all is not written at
 * all (the game would cut ":kalp:" to ":ka").
 */
public final class Insertion {
    private Insertion() {
    }

    /**
     * The text replacing {@code [start, end)} of {@code value} (the selection, or the caret when both are the same), or null
     * when {@code piece} does not fit.
     */
    public static String around(String value, int start, int end, String piece, int max) {
        int from = Math.max(0, Math.min(start, value.length()));
        int to = Math.max(from, Math.min(end, value.length()));
        String before = from > 0 && !Character.isWhitespace(value.charAt(from - 1)) ? " " : "";
        String after = to < value.length() && Character.isWhitespace(value.charAt(to)) ? "" : " ";
        int room = max - (value.length() - (to - from));
        String full = before + piece + after;
        if (full.length() <= room) {
            return full;
        }
        if ((before + piece).length() <= room) {
            return before + piece;
        }
        return piece.length() <= room ? piece : null;
    }
}
