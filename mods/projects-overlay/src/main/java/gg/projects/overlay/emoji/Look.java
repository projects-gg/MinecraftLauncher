package gg.projects.overlay.emoji;

import java.util.ArrayList;
import java.util.List;

/**
 * An emoji's look as the server writes it in chat: text with Minecraft's {@code &} codes (docs/content-format.md, "Chat
 * emojis"): {@code &} then a colour ({@code 0}–{@code 9}, {@code a}–{@code f}) or a style ({@code l} bold, {@code o}
 * italic, {@code n} underlined, {@code m} struck through, {@code r} reset; either case). As in the game, a colour ends the
 * styles before it. Pure, so it is tested on its own; the picker turns the spans into the game's styled text.
 */
public final class Look {
    /** The game's sixteen chat colours, by code. */
    private static final int[] COLOURS = {
        0x000000, 0x0000AA, 0x00AA00, 0x00AAAA, 0xAA0000, 0xAA00AA, 0xFFAA00, 0xAAAAAA,
        0x555555, 0x5555FF, 0x55FF55, 0x55FFFF, 0xFF5555, 0xFF55FF, 0xFFFF55, 0xFFFFFF,
    };
    /** Longest look in characters shown (the content format's limit). */
    public static final int MAX_CHARS = 48;

    /** A run of text in one style; {@code colour} is RGB, or -1 for the line's own colour. */
    public record Span(String text, int colour, boolean bold, boolean italic, boolean underlined, boolean struck) {
    }

    private Look() {
    }

    /**
     * The look's runs of text, or null when it breaks the rules (an {@code &} without a code, a control, {@code §} or
     * private use character, nothing to show, more than {@link #MAX_CHARS} characters).
     */
    public static List<Span> spans(String look) {
        if (look == null) {
            return null;
        }
        List<Span> out = new ArrayList<>(2);
        StringBuilder run = new StringBuilder();
        int colour = -1;
        boolean bold = false;
        boolean italic = false;
        boolean underlined = false;
        boolean struck = false;
        int shown = 0;
        boolean visible = false;
        for (int i = 0; i < look.length(); ) {
            int cp = look.codePointAt(i);
            i += Character.charCount(cp);
            if (cp == '&') {
                if (i >= look.length()) {
                    return null;
                }
                char code = Character.toLowerCase(look.charAt(i));
                i++;
                int digit = Character.digit(code, 16);
                if (digit < 0 && "lonmr".indexOf(code) < 0) {
                    return null;
                }
                if (run.length() > 0) {
                    out.add(new Span(run.toString(), colour, bold, italic, underlined, struck));
                    run.setLength(0);
                }
                if (digit >= 0) {
                    colour = COLOURS[digit];
                    bold = italic = underlined = struck = false;
                } else {
                    switch (code) {
                        case 'l' -> bold = true;
                        case 'o' -> italic = true;
                        case 'n' -> underlined = true;
                        case 'm' -> struck = true;
                        default -> {
                            colour = -1;
                            bold = italic = underlined = struck = false;
                        }
                    }
                }
                continue;
            }
            if (Character.isISOControl(cp) || cp == 0xA7 || isPrivateUse(cp)) {
                return null;
            }
            run.appendCodePoint(cp);
            shown++;
            visible |= !Character.isWhitespace(cp);
        }
        if (run.length() > 0) {
            out.add(new Span(run.toString(), colour, bold, italic, underlined, struck));
        }
        if (!visible || shown > MAX_CHARS) {
            return null;
        }
        return out;
    }

    /** The look's text without its codes ("&c❤" is "❤"); empty for a look that breaks the rules. */
    public static String plain(String look) {
        List<Span> spans = spans(look);
        if (spans == null) {
            return "";
        }
        StringBuilder b = new StringBuilder();
        for (Span span : spans) {
            b.append(span.text());
        }
        return b.toString();
    }

    /** The private use areas a server font gives its pictures (U+E000–U+F8FF, planes 15 and 16). */
    public static boolean isPrivateUse(int cp) {
        return (cp >= 0xE000 && cp <= 0xF8FF) || (cp >= 0xF0000 && cp <= 0xFFFFD) || (cp >= 0x100000 && cp <= 0x10FFFD);
    }
}
