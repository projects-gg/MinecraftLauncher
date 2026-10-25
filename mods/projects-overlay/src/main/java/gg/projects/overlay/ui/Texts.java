package gg.projects.overlay.ui;

import gg.projects.overlay.lang.Dictionary;
import gg.projects.overlay.lang.Language;
import gg.projects.overlay.lang.LanguageChoice;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.FormattedText;
import net.minecraft.network.chat.MutableComponent;
import net.minecraft.network.chat.Style;
import net.minecraft.util.FormattedCharSequence;

/**
 * Words and lines: translations, ellipsis, wrapping to a number of lines, initials. Lines for drawing come out
 * as {@link PreparedLine}s, laid out once and drawn with that layout until something it depends on changes.
 *
 * <p>The words are the overlay's own ({@link Dictionary}) in the language the launcher names ({@link LanguageChoice},
 * OVL-LANG), whatever language the game itself shows; without the launcher's word, the game's.
 */
public final class Texts {
    public static final String ELLIPSIS = "…";
    private static final String PREFIX = "projects_overlay.";

    private static volatile LanguageChoice language = new LanguageChoice(null, Texts::gameLanguage);

    private Texts() {
    }

    /** The overlay speaks what {@code choice} says from now on (set once, as the overlay starts). */
    public static void speak(LanguageChoice choice) {
        language = choice;
    }

    /** Which language the overlay shows, and the launcher's word on it. */
    public static LanguageChoice language() {
        return language;
    }

    /** The language the overlay shows now. */
    public static Language current() {
        return language.current();
    }

    /** The game's own language code (what the overlay follows without the launcher's word). */
    public static String gameLanguage() {
        Minecraft mc = Minecraft.getInstance();
        return mc == null || mc.options == null ? Language.ENGLISH.gameCode : mc.options.languageCode;
    }

    /** {@code key}'s words in the overlay's language; arguments that are components keep their style. */
    public static Component tr(String key, Object... args) {
        Dictionary words = Dictionary.shared();
        List<Dictionary.Part> parts = words.parts(words.template(current(), PREFIX + key));
        if (parts.size() == 1 && !parts.get(0).argument()) {
            return Component.literal(parts.get(0).text());
        }
        MutableComponent out = Component.empty();
        for (Dictionary.Part part : parts) {
            if (!part.argument()) {
                out.append(Component.literal(part.text()));
            } else if (args != null && part.index() < args.length && args[part.index()] != null) {
                Object arg = args[part.index()];
                out.append(arg instanceof Component c ? c : Component.literal(String.valueOf(arg)));
            }
        }
        return out;
    }

    /** {@code key}'s words in the overlay's language, as plain text. */
    public static String str(String key, Object... args) {
        Dictionary words = Dictionary.shared();
        return Dictionary.fill(words.parts(words.template(current(), PREFIX + key)), args,
            arg -> arg instanceof Component c ? c.getString() : String.valueOf(arg));
    }

    /** {@code text} cut to {@code maxWidth} with an ellipsis when it does not fit. */
    public static String fit(Font font, String text, int maxWidth) {
        if (font.width(text) <= maxWidth) {
            return text;
        }
        int room = Math.max(0, maxWidth - font.width(ELLIPSIS));
        return font.plainSubstrByWidth(text, room).stripTrailing() + ELLIPSIS;
    }

    /** {@code text} wrapped to {@code width}, at most {@code maxLines}; the last kept line ends in an ellipsis. */
    public static List<FormattedCharSequence> wrap(Font font, String text, int width, int maxLines) {
        List<FormattedText> parts = font.getSplitter().splitLines(text, width, Style.EMPTY);
        List<FormattedCharSequence> out = new ArrayList<>(Math.min(parts.size(), maxLines));
        for (int i = 0; i < parts.size() && i < maxLines; i++) {
            String line = parts.get(i).getString();
            if (i == maxLines - 1 && parts.size() > maxLines) {
                line = fit(font, line + " " + rest(parts, i + 1), width);
            }
            out.add(visual(line));
        }
        if (out.isEmpty()) {
            out.add(FormattedCharSequence.EMPTY);
        }
        return out;
    }

    /**
     * One line in display order (right-to-left names read right), keeping its layout between frames while it
     * stays on screen unchanged ({@link PreparedLine}).
     */
    public static FormattedCharSequence visual(String line) {
        return PreparedLine.of(net.minecraft.locale.Language.getInstance().getVisualOrder(FormattedText.of(line)));
    }

    /** A link the player may press, where it sits in a wrapped text (line index, x range in GUI pixels). */
    public record LinkSpan(int line, float x0, float x1, String url, boolean trusted) {
    }

    /** A text wrapped with its links drawn as links, and where they are. */
    public record Wrapped(List<FormattedCharSequence> lines, List<LinkSpan> links) {
    }

    /**
     * {@code text} wrapped to {@code width} (at most {@code maxLines}), its links ({@code urls}, as written, in
     * order; each found once from where the previous one ended) underlined in {@code linkColor}. Links that are
     * not found, and a text longer than {@code maxLines}, come out as plain text.
     */
    public static Wrapped wrapWithLinks(Font font, String text, List<String> urls, List<Boolean> trusted, int width,
        int maxLines, int linkColor) {
        int n = urls.size();
        int[] starts = new int[n];
        int[] ends = new int[n];
        int found = 0;
        int from = 0;
        for (int i = 0; i < n; i++) {
            int at = text.indexOf(urls.get(i), from);
            starts[i] = at;
            ends[i] = at < 0 ? -1 : at + urls.get(i).length();
            if (at >= 0) {
                found++;
                from = ends[i];
            }
        }
        if (found == 0) {
            return new Wrapped(wrap(font, text, width, maxLines), List.of());
        }
        List<int[]> ranges = new ArrayList<>();
        font.getSplitter().splitLines(text, width, Style.EMPTY, false, (style, start, end) -> ranges.add(
            new int[] {start, end}));
        if (ranges.isEmpty() || ranges.size() > maxLines) {
            return new Wrapped(wrap(font, text, width, maxLines), List.of());
        }
        Style link = Style.EMPTY.withColor(linkColor & 0xFFFFFF).withUnderlined(true);
        List<FormattedCharSequence> lines = new ArrayList<>(ranges.size());
        List<LinkSpan> spans = new ArrayList<>(found);
        for (int li = 0; li < ranges.size(); li++) {
            int s = ranges.get(li)[0];
            int e = ranges.get(li)[1];
            MutableComponent line = Component.empty();
            int pos = s;
            for (int k = 0; k < n; k++) {
                if (starts[k] < 0) {
                    continue;
                }
                int ls = Math.max(s, starts[k]);
                int le = Math.min(e, ends[k]);
                if (ls >= le) {
                    continue;
                }
                if (pos < ls) {
                    line.append(Component.literal(text.substring(pos, ls)));
                }
                line.append(Component.literal(text.substring(ls, le)).withStyle(link));
                spans.add(new LinkSpan(li, font.width(text.substring(s, ls)), font.width(text.substring(s, le)),
                    urls.get(k), trusted.get(k)));
                pos = le;
            }
            if (pos < e) {
                line.append(Component.literal(text.substring(pos, e)));
            }
            lines.add(PreparedLine.of(net.minecraft.locale.Language.getInstance().getVisualOrder(line)));
        }
        return new Wrapped(lines, spans);
    }

    private static String rest(List<FormattedText> parts, int from) {
        StringBuilder b = new StringBuilder();
        for (int i = from; i < parts.size(); i++) {
            b.append(parts.get(i).getString());
        }
        return b.toString();
    }

    /** The first letter of a name, as a capital in the overlay's language (İ for i in Turkish). */
    public static String initial(String name) {
        if (name == null || name.isBlank()) {
            return "?";
        }
        String trimmed = name.strip();
        int cp = trimmed.codePointAt(0);
        String first = new String(Character.toChars(cp));
        return first.toUpperCase(current().locale);
    }

    /** The overlay speaks Turkish. */
    public static boolean turkish() {
        return current() == Language.TURKISH;
    }

    /** A percentage in the overlay's language: "%70" in Turkish, "70%" in English. */
    public static String percent(int value) {
        return turkish() ? "%" + value : value + "%";
    }
}
