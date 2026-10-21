package gg.projects.overlay.ui;

/** Launcher design tokens (docs: obsidian, ash, bone, gold, jade, flare) as ARGB colors for the game's GUI. */
public final class Theme {
    /** obsidian-900 at ~90%: panels and toasts stay readable over any scene. */
    public static final int PANEL = 0xE6121016;
    public static final int TOAST = 0xF0121016;
    /** obsidian-800: rows and inputs. */
    public static final int ROW = 0xFF1B1820;
    /** obsidian-700: borders and dividers. */
    public static final int BORDER = 0xFF27222C;
    /** bone-100: primary text. */
    public static final int TEXT = 0xFFF3EBDD;
    /** ash-300: secondary text on translucent surfaces. */
    public static final int TEXT_DIM = 0xFFBDB6C3;
    /** ash-400: secondary text on solid surfaces. */
    public static final int TEXT_MUTED = 0xFF9B93A3;
    /** gold-500: the single accent. */
    public static final int GOLD = 0xFFE89A2C;
    /** gold-300: highlights. */
    public static final int GOLD_SOFT = 0xFFF6D58E;
    /** jade-400: friend presence only. */
    public static final int JADE = 0xFF5FCB8E;
    /** flare-400: errors only. */
    public static final int FLARE = 0xFFF0624D;
    /** Light dim over the game while the panel is open; the world stays visible and keeps running. */
    public static final int SCRIM = 0x66000000;

    private Theme() {
    }
}
