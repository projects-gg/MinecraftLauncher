package gg.projects.overlay.ui;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.BitSet;

/**
 * The emoji the overlay's faces draw as colour pictures (CHAT-EMOJI; tools/build-emoji.py: the launcher's set, Noto Color
 * Emoji, one picture per GUI scale). The game draws a font's picture tinted with the text's colour, so a line puts these in
 * white ({@link Texts#visual}): a 👍 in an ash reaction chip or a bone message keeps its own colours. The characters come
 * from {@code font/emoji_chars.txt}, ranges of hex code points the build writes.
 */
public final class EmojiGlyphs {
    static final String RESOURCE = "/assets/projects_overlay/font/emoji_chars.txt";
    private static final BitSet CHARS = load(EmojiGlyphs.class.getResourceAsStream(RESOURCE));

    private EmojiGlyphs() {
    }

    /** Whether {@code cp} is one of the colour emoji. */
    public static boolean colour(int cp) {
        return cp >= 0 && CHARS.get(cp);
    }

    /** Whether {@code text} has any colour emoji (most lines have none: they stay one plain run). */
    public static boolean any(String text) {
        if (text == null) {
            return false;
        }
        for (int i = 0; i < text.length(); ) {
            int cp = text.codePointAt(i);
            if (colour(cp)) {
                return true;
            }
            i += Character.charCount(cp);
        }
        return false;
    }

    /** The ranges file ("1f600-1f64f" or "2764" a line); a missing or broken file is none. */
    static BitSet load(InputStream in) {
        BitSet set = new BitSet();
        if (in == null) {
            return set;
        }
        try (BufferedReader reader = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8))) {
            for (String line; (line = reader.readLine()) != null; ) {
                line = line.strip();
                if (line.isEmpty()) {
                    continue;
                }
                int dash = line.indexOf('-');
                try {
                    int from = Integer.parseInt(dash < 0 ? line : line.substring(0, dash), 16);
                    int to = dash < 0 ? from : Integer.parseInt(line.substring(dash + 1), 16);
                    if (from >= 0 && to >= from && to <= Character.MAX_CODE_POINT) {
                        set.set(from, to + 1);
                    }
                } catch (NumberFormatException e) {
                    // A line that is not a range is left out.
                }
            }
        } catch (IOException e) {
            return new BitSet();
        }
        return set;
    }
}
