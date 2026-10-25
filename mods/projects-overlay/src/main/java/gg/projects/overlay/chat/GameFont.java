package gg.projects.overlay.chat;

import gg.projects.overlay.ui.Fonts;
import it.unimi.dsi.fastutil.ints.Int2BooleanOpenHashMap;
import net.minecraft.client.gui.GlyphSource;
import net.minecraft.client.gui.font.glyphs.BakedGlyph;

/**
 * What the game's own font, the one chat draws with, can draw now (CHAT-EMOJI). The server resource pack adds its pictures
 * to that font (a Projects pack emoji is a private use character of it), so "does the font have the character" is the
 * question "is the pack loaded", asked of the very thing chat will use: in the lobby without the pack it is no, on Survival
 * once the pack loaded it is yes. The answers are kept only while the font stays the same; every resource reload (a pack
 * coming or going, a move between the network's servers) makes a new one.
 */
final class GameFont {
    /** A code point no font has: what the font draws for a character it lacks. */
    private static final int SURELY_MISSING = 0x10FFFF;

    private static GlyphSource source;
    private static BakedGlyph missing;
    private static final Int2BooleanOpenHashMap KNOWN = new Int2BooleanOpenHashMap();

    private GameFont() {
    }

    /** Whether the game's font draws {@code cp} (true when it cannot tell, so a preview never hides for that). */
    static boolean draws(int cp) {
        GlyphSource now = Fonts.gameGlyphs();
        if (now == null) {
            return true;
        }
        if (now != source) {
            source = now;
            missing = now.getGlyph(SURELY_MISSING);
            KNOWN.clear();
        }
        if (KNOWN.containsKey(cp)) {
            return KNOWN.get(cp);
        }
        boolean has = now.getGlyph(cp) != missing;
        KNOWN.put(cp, has);
        return has;
    }

    /** Whether the font draws a server pack's picture: false when it cannot tell (the look stands in). */
    static boolean drawsPicture(int cp) {
        return Fonts.gameGlyphs() != null && draws(cp);
    }

    /** Forgets every answer (the game moved to another server: its pack may come or go with it). */
    static void forget() {
        source = null;
        missing = null;
        KNOWN.clear();
    }
}
