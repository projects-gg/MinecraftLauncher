package gg.projects.overlay.ui;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.locale.Language;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.FormattedText;
import net.minecraft.network.chat.MutableComponent;
import net.minecraft.network.chat.Style;
import net.minecraft.util.FormattedCharSequence;

/**
 * Words and lines: translations, ellipsis, wrapping to a number of lines, initials. Lines for drawing come out
 * as {@link PreparedLine}s, laid out once and drawn with that layout until something it depends on changes.
 */
public final class Texts {
    public static final String ELLIPSIS = "…";
    private static final String PREFIX = "projects_overlay.";

    private Texts() {
    }

    public static Component tr(String key, Object... args) {
        return Component.translatable(PREFIX + key, args);
    }

    public static String str(String key, Object... args) {
        return tr(key, args).getString();
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
        return PreparedLine.of(Language.getInstance().getVisualOrder(FormattedText.of(line)));
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
            lines.add(PreparedLine.of(Language.getInstance().getVisualOrder(line)));
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

    /** The first letter of a name, as a capital in the game's language (İ for i in Turkish). */
    public static String initial(String name) {
        if (name == null || name.isBlank()) {
            return "?";
        }
        String trimmed = name.strip();
        int cp = trimmed.codePointAt(0);
        String first = new String(Character.toChars(cp));
        Locale locale = turkish() ? Locale.forLanguageTag("tr") : Locale.ROOT;
        return first.toUpperCase(locale);
    }

    /** The game speaks Turkish. */
    public static boolean turkish() {
        String code = Minecraft.getInstance().options.languageCode;
        return code != null && code.toLowerCase(Locale.ROOT).startsWith("tr");
    }

    /** A percentage in the game's language: "%70" in Turkish, "70%" in English. */
    public static String percent(int value) {
        return turkish() ? "%" + value : value + "%";
    }
}
