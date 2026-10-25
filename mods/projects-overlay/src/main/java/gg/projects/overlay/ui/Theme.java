package gg.projects.overlay.ui;

/**
 * The launcher's design tokens (packages/ui/src/styles.css, docs/design.md §2) as ARGB colors, and the
 * small color math the overlay needs. Gold is the single accent; jade only for presence, flare only for
 * errors. Pure (no game classes), so it is tested on its own.
 */
public final class Theme {
    public static final int OBSIDIAN_950 = 0xFF0A090C;
    public static final int OBSIDIAN_900 = 0xFF121016;
    public static final int OBSIDIAN_850 = 0xFF16131A;
    public static final int OBSIDIAN_800 = 0xFF1B1820;
    public static final int OBSIDIAN_700 = 0xFF27222C;
    public static final int OBSIDIAN_600 = 0xFF3A3340;
    public static final int ASH_500 = 0xFF7D7586;
    public static final int ASH_400 = 0xFF9B93A3;
    public static final int ASH_300 = 0xFFBDB6C3;
    public static final int BONE_100 = 0xFFF3EBDD;
    public static final int BONE_50 = 0xFFFBF6EC;
    public static final int GOLD_200 = 0xFFFAE5B3;
    public static final int GOLD_300 = 0xFFF6D58E;
    public static final int GOLD_400 = 0xFFF0B849;
    public static final int GOLD_500 = 0xFFE89A2C;
    public static final int GOLD_600 = 0xFFC9741A;
    public static final int GOLD_700 = 0xFF8E4A12;
    public static final int GOLD_INK = 0xFF1A0F05;
    public static final int EMBER_950 = 0xFF220606;
    public static final int EMBER_900 = 0xFF3F0A0A;
    public static final int JADE_400 = 0xFF5FCB8E;
    public static final int FLARE_300 = 0xFFF6836F;
    public static final int FLARE_400 = 0xFFF0624D;
    public static final int WHITE = 0xFFFFFFFF;
    /** A calmer bone for reading text on glass (message bodies): warm, below the titles' bone-100. */
    public static final int BONE_300 = mix(BONE_100, ASH_300, 0.45f);

    /** The world behind the open panel: a light dim, never a blur (the game keeps running). */
    public static final int SCRIM = 0x47000000;
    /** The player's own chat bubble: gold mixed into obsidian-800, as in the launcher. */
    public static final int BUBBLE_MINE = mix(OBSIDIAN_800, GOLD_500, 0.17f);

    private Theme() {
    }

    /** {@code argb} with its alpha multiplied by {@code factor} (0..1). */
    public static int fade(int argb, float factor) {
        if (factor >= 1f) {
            return argb;
        }
        int a = (argb >>> 24) & 0xFF;
        int scaled = Math.round(a * Math.max(0f, factor));
        return (scaled << 24) | (argb & 0x00FFFFFF);
    }

    /** {@code rgb} at an alpha of {@code alpha} (0..1). */
    public static int withAlpha(int rgb, float alpha) {
        int a = Math.round(Math.max(0f, Math.min(1f, alpha)) * 255f);
        return (a << 24) | (rgb & 0x00FFFFFF);
    }

    public static int alpha(int argb) {
        return (argb >>> 24) & 0xFF;
    }

    /** Linear mix of two colors (channels and alpha), {@code t} = 0 gives {@code a}. */
    public static int mix(int a, int b, float t) {
        float u = Math.max(0f, Math.min(1f, t));
        int aa = (a >>> 24) & 0xFF;
        int ar = (a >>> 16) & 0xFF;
        int ag = (a >>> 8) & 0xFF;
        int ab = a & 0xFF;
        int ba = (b >>> 24) & 0xFF;
        int br = (b >>> 16) & 0xFF;
        int bg = (b >>> 8) & 0xFF;
        int bb = b & 0xFF;
        return (Math.round(aa + (ba - aa) * u) << 24) | (Math.round(ar + (br - ar) * u) << 16)
            | (Math.round(ag + (bg - ag) * u) << 8) | Math.round(ab + (bb - ab) * u);
    }

    /** Source-over compositing of two straight-alpha colors. */
    public static int over(int top, int bottom) {
        float ta = ((top >>> 24) & 0xFF) / 255f;
        float ba = ((bottom >>> 24) & 0xFF) / 255f;
        float oa = ta + ba * (1f - ta);
        if (oa <= 0f) {
            return 0;
        }
        float r = (((top >>> 16) & 0xFF) * ta + ((bottom >>> 16) & 0xFF) * ba * (1f - ta)) / oa;
        float g = (((top >>> 8) & 0xFF) * ta + ((bottom >>> 8) & 0xFF) * ba * (1f - ta)) / oa;
        float b = ((top & 0xFF) * ta + (bottom & 0xFF) * ba * (1f - ta)) / oa;
        return (Math.round(oa * 255f) << 24) | (Math.round(r) << 16) | (Math.round(g) << 8) | Math.round(b);
    }
}
