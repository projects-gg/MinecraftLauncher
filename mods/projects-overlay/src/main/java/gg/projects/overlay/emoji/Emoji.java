package gg.projects.overlay.emoji;

import java.util.List;
import java.util.Locale;

/**
 * One entry of the picker. For the server's list (docs/content-format.md, "Chat emojis") the picker writes the {@code
 * code} and the server shows the {@code look} (or the resource pack's {@code glyph}); for the picker's own unicode set
 * code and look are the text itself and the "meaning" is its name.
 *
 * @param code    what the picker writes into the chat line
 * @param aliases other codes the server takes for it (the picker never shows them)
 * @param look    how it shows in chat, with {@code &} codes; null for a shortcut
 * @param glyph   a pack emoji's private use code point, or -1
 * @param rank    the rank the server asks for ("VIP"), or null
 * @param servers the network's servers it works on (empty: all)
 * @param nameTr  a shortcut's meaning, or a unicode entry's name, in Turkish (null: none)
 * @param nameEn  the same in English
 */
public record Emoji(String code, List<String> aliases, String look, int glyph, EmojiKind kind, String rank,
    List<String> servers, String nameTr, String nameEn) {

    public Emoji {
        aliases = List.copyOf(aliases);
        servers = List.copyOf(servers);
    }

    /** A unicode entry of the picker's own set: written as it is, named in both languages. */
    public static Emoji plain(String text, EmojiKind kind, String nameTr, String nameEn) {
        return new Emoji(text, List.of(), text, -1, kind, null, List.of(), nameTr, nameEn);
    }

    /** Its name or meaning in the overlay's language (the other one when that is missing), or null. */
    public String name(boolean turkish) {
        String first = turkish ? nameTr : nameEn;
        return first != null ? first : turkish ? nameEn : nameTr;
    }

    /** Whether its code or one of its aliases is {@code code}, without case. */
    public boolean answersTo(String code) {
        String wanted = code.toLowerCase(Locale.ROOT);
        if (this.code.toLowerCase(Locale.ROOT).equals(wanted)) {
            return true;
        }
        for (String alias : aliases) {
            if (alias.toLowerCase(Locale.ROOT).equals(wanted)) {
                return true;
            }
        }
        return false;
    }
}
