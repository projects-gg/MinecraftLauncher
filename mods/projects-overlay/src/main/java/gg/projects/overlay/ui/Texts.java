package gg.projects.overlay.ui;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.locale.Language;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.FormattedText;
import net.minecraft.network.chat.Style;
import net.minecraft.util.FormattedCharSequence;

/** Words and lines: translations, ellipsis, wrapping to a number of lines, initials. */
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

    /** One line in display order (right-to-left names read right). */
    public static FormattedCharSequence visual(String line) {
        return Language.getInstance().getVisualOrder(FormattedText.of(line));
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
