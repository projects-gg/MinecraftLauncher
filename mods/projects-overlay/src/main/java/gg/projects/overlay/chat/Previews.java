package gg.projects.overlay.chat;

import gg.projects.overlay.emoji.Emoji;
import gg.projects.overlay.emoji.EmojiKind;
import gg.projects.overlay.emoji.Look;
import java.util.List;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MutableComponent;
import net.minecraft.network.chat.Style;

/**
 * An emoji as chat will show it (CHAT-EMOJI): its look in the game's font with the server's colours, or, for a Projects pack
 * emoji while the game's font has the pack's picture, that picture. Never the launcher's colour emoji: the picker shows
 * what the line will look like.
 */
final class Previews {
    /** The colour of a look's text that names none: the chat's own white, warmed to the launcher's bone. */
    static final int PLAIN_TEXT = 0xF3EBDD;

    private Previews() {
    }

    /** Whether {@code emoji} shows as the server pack's picture now. */
    static boolean picture(Emoji emoji) {
        return emoji.kind() == EmojiKind.PACK && emoji.glyph() >= 0 && GameFont.drawsPicture(emoji.glyph());
    }

    /** The preview: the pack's picture (white, so it keeps its colours) or the look with its colours. */
    static Component of(Emoji emoji) {
        if (picture(emoji)) {
            return Component.literal(new String(Character.toChars(emoji.glyph()))).withStyle(Style.EMPTY.withColor(0xFFFFFF));
        }
        return look(emoji.look() == null ? emoji.code() : emoji.look());
    }

    /** A look's spans as the game's styled text; a look that breaks the rules shows as written. */
    static Component look(String look) {
        List<Look.Span> spans = Look.spans(look);
        if (spans == null) {
            return Component.literal(look).withStyle(Style.EMPTY.withColor(PLAIN_TEXT));
        }
        MutableComponent out = Component.empty();
        for (Look.Span span : spans) {
            Style style = Style.EMPTY.withColor(span.colour() < 0 ? PLAIN_TEXT : span.colour())
                .withBold(span.bold()).withItalic(span.italic()).withUnderlined(span.underlined())
                .withStrikethrough(span.struck());
            out.append(Component.literal(span.text()).withStyle(style));
        }
        return out;
    }
}
