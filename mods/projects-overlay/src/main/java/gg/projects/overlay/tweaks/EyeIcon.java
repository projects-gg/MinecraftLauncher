package gg.projects.overlay.tweaks;

import java.util.ArrayList;
import java.util.List;

/**
 * PASSWORD-EYE's eye, drawn in the game's own pixels (no texture): an open eye while what was typed is hidden (press
 * it to show), the same eye struck through while it shows (press it to hide), as browsers draw theirs. Both
 * {@link #W} by {@link #H}; the strike leaves a pixel of room on its right so the eye still reads.
 */
final class EyeIcon {
    /** The open eye: an almond with its iris. */
    static final String[] OPEN = {
        "...........",
        "...#####...",
        ".##.....##.",
        "#...###...#",
        "#..#####..#",
        "#...###...#",
        ".##.....##.",
        "...#####...",
        "...........",
    };
    /** The eye struck through, top left to bottom right. */
    static final String[] STRUCK = strike(OPEN);
    static final int W = OPEN[0].length();
    static final int H = OPEN.length;

    /** One run of lit pixels in a row: from {@code x}, {@code length} long. */
    record Run(int x, int y, int length) {
    }

    private EyeIcon() {
    }

    /** The lit pixels of {@code rows}, as runs along each row (one fill each). */
    static List<Run> runs(String[] rows) {
        List<Run> runs = new ArrayList<>();
        for (int y = 0; y < rows.length; y++) {
            String row = rows[y];
            int x = 0;
            while (x < row.length()) {
                if (row.charAt(x) != '#') {
                    x++;
                    continue;
                }
                int start = x;
                while (x < row.length() && row.charAt(x) == '#') {
                    x++;
                }
                runs.add(new Run(start, y, x - start));
            }
        }
        return List.copyOf(runs);
    }

    private static String[] strike(String[] rows) {
        String[] struck = new String[rows.length];
        for (int y = 0; y < rows.length; y++) {
            char[] row = rows[y].toCharArray();
            int x = 1 + y;
            if (x + 1 < row.length) {
                row[x + 1] = '.';
            }
            if (x < row.length) {
                row[x] = '#';
            }
            struck[y] = new String(row);
        }
        return struck;
    }
}
