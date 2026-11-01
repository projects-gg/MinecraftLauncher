package gg.projects.overlay.ui.draw;

import gg.projects.overlay.ui.Theme;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;
import net.minecraft.client.gui.GuiGraphicsExtractor;

/**
 * Profile badges in the game (BADGES-UI; docs/tasks/badges.md, "Katman (piksel)"): pixel art on the GUI grid, 9 x 9 GUI
 * pixels, the size of the game's own hearts. A badge is put together where it is drawn from three white pieces of the
 * shapes atlas, each tinted: the drop shadow (the game's text shadow: a quarter of the colour, a pixel right and down,
 * baked only where the badge is not, so a fading badge never shows it through), the shape's mask in the badge's
 * colour, and its 5 x 5 glyph in white, centred (in the shield a pixel higher, its point leaving less room below). Three
 * shapes and nine glyphs drawn by hand for the 9-pixel grid, each still itself at GUI scale 1, rather than a sprite for
 * every badge; each GUI pixel is baked as a whole block of screen pixels, so the art stays square at every GUI scale.
 * Pure but for {@link #draw}.
 */
public final class MarkSprites {
    /** A badge, GUI pixels. */
    public static final int SIZE = 9;
    /** Its glyph. */
    public static final int GLYPH = 5;
    /** The badge with its shadow. */
    public static final int FOOT = SIZE + 1;

    /** A shape: its mask, where its glyph's box sits from the top, and its atlas names. */
    record Shape(String[] rows, int glyphY, String fillKey, String shadowKey) {
    }

    /** A glyph: its pixels and its atlas name. */
    record Glyph(String[] rows, String key) {
    }

    private static final Map<String, Shape> SHAPES = new LinkedHashMap<>();
    private static final Map<String, Glyph> GLYPHS = new LinkedHashMap<>();

    static {
        // SOC-VERIFY's rosette on nine pixels: a round body with its corners cut and a point on each side.
        shape("seal", 2,
            "....X....",
            "..XXXXX..",
            ".XXXXXXX.",
            ".XXXXXXX.",
            "XXXXXXXXX",
            ".XXXXXXX.",
            ".XXXXXXX.",
            "..XXXXX..",
            "....X....");
        shape("round", 2,
            "...XXX...",
            ".XXXXXXX.",
            ".XXXXXXX.",
            "XXXXXXXXX",
            "XXXXXXXXX",
            "XXXXXXXXX",
            ".XXXXXXX.",
            ".XXXXXXX.",
            "...XXX...");
        // Square shoulders, straight sides, a point below (softened corners made it read as a map pin at GUI 1).
        shape("shield", 1,
            "XXXXXXXXX",
            "XXXXXXXXX",
            "XXXXXXXXX",
            "XXXXXXXXX",
            "XXXXXXXXX",
            ".XXXXXXX.",
            "..XXXXX..",
            "...XXX...",
            "....X....");
        // The launcher's check: its legs at 45 degrees through whole pixels, two pixels thick so it reads at GUI 1.
        glyph("check",
            "....X",
            "...XX",
            "X.XX.",
            "XXX..",
            ".X...");
        // One point up, the arms out, two feet: a spike over the arms read as a plus or a stick figure.
        glyph("star",
            "..X..",
            "XXXXX",
            ".XXX.",
            ".X.X.",
            ".....");
        // Flat below, its biggest puff right of the middle (as the launcher's).
        glyph("cloud",
            ".....",
            "..XX.",
            ".XXXX",
            "XXXXX",
            ".....");
        // Heavy above, a thin point below, the step between: a one-pixel zigzag read as a slash.
        glyph("bolt",
            "..XXX",
            ".XXX.",
            "XXXX.",
            "..XX.",
            ".X...");
        // Three prongs, the middle one tallest, on a band: one-pixel points read as a castle's top.
        glyph("crown",
            "..X..",
            "X.X.X",
            "X.X.X",
            "XXXXX",
            "XXXXX");
        glyph("heart",
            ".X.X.",
            "XXXXX",
            "XXXXX",
            ".XXX.",
            "..X..");
        // A lab flask: the neck, the shoulders, the wide foot.
        glyph("flask",
            "..X..",
            "..X..",
            ".XXX.",
            "XXXXX",
            "XXXXX");
        // A plain triangle at 45 degrees: one with a straight back read as a pennant.
        glyph("play",
            ".X...",
            ".XX..",
            ".XXX.",
            ".XX..",
            ".X...");
        // A cup with its handles out at the sides, on a stem and a foot.
        glyph("trophy",
            ".XXX.",
            "XXXXX",
            ".XXX.",
            "..X..",
            ".XXX.");
    }

    private MarkSprites() {
    }

    private static void shape(String name, int glyphY, String... rows) {
        SHAPES.put(name, new Shape(rows, glyphY, "mark:" + name, "mark:" + name + ":shadow"));
    }

    private static void glyph(String name, String... rows) {
        GLYPHS.put(name, new Glyph(rows, "mark:glyph:" + name));
    }

    /** The shapes this build draws. */
    public static Set<String> shapes() {
        return SHAPES.keySet();
    }

    /** The glyphs this build draws. */
    public static Set<String> glyphs() {
        return GLYPHS.keySet();
    }

    /** A badge colour's ARGB ({@code --color-mark-<name>}), or 0 for one this build does not know. */
    public static int color(String name) {
        if (name == null) {
            return 0;
        }
        return switch (name) {
            case "blue" -> 0xFF2A8AF0;
            case "red" -> 0xFFE0434A;
            case "purple" -> 0xFF9B5EEB;
            case "green" -> 0xFF219453;
            case "orange" -> 0xFFD95F1A;
            case "teal" -> 0xFF138D89;
            case "indigo" -> 0xFF6673E8;
            case "pink" -> 0xFFDF378B;
            case "slate" -> 0xFF7F7A9F;
            default -> 0;
        };
    }

    /** The shadow under a badge of colour {@code argb}: the game's text shadow, a quarter of each channel. */
    public static int shadow(int argb) {
        return 0xFF000000 | ((argb & 0xFCFCFC) >>> 2);
    }

    /** Whether {@code shape}'s mask covers its pixel {@code x, y} (outside the 9 x 9 square: no). */
    static boolean covers(String shape, int x, int y) {
        Shape s = SHAPES.get(shape);
        return s != null && x >= 0 && y >= 0 && x < SIZE && y < SIZE && s.rows()[y].charAt(x) == 'X';
    }

    /** Whether {@code glyph} has its pixel {@code x, y} of its 5 x 5 box. */
    static boolean inGlyph(String glyph, int x, int y) {
        Glyph g = GLYPHS.get(glyph);
        return g != null && x >= 0 && y >= 0 && x < GLYPH && y < GLYPH && g.rows()[y].charAt(x) == 'X';
    }

    /** Where {@code glyph}'s box sits in a badge of {@code shape}: its left and top pixel, or null for an unknown one. */
    static int[] glyphAt(String shape, String glyph) {
        Shape s = SHAPES.get(shape);
        return s == null || !GLYPHS.containsKey(glyph) ? null : new int[] {(SIZE - GLYPH) / 2, s.glyphY()};
    }

    /** Whether the shadow has its pixel {@code x, y} of the {@link #FOOT} square: the shape a pixel right and down. */
    static boolean inShadow(String shape, int x, int y) {
        return covers(shape, x - 1, y - 1) && !covers(shape, x, y);
    }

    /**
     * The badge as {@link #draw} puts it on the screen at full opacity, {@link #FOOT} pixels square row by row (ARGB,
     * clear around it): the shadow, the shape in {@code color}, the glyph in white. Null for a look this build does not
     * know. For tests and the review sheet.
     */
    public static int[] compose(String shape, String glyph, String color) {
        int fill = color(color);
        int[] at = glyphAt(shape, glyph);
        if (fill == 0 || at == null) {
            return null;
        }
        int[] out = new int[FOOT * FOOT];
        for (int y = 0; y < FOOT; y++) {
            for (int x = 0; x < FOOT; x++) {
                int px = 0;
                if (inShadow(shape, x, y)) {
                    px = shadow(fill);
                }
                if (covers(shape, x, y)) {
                    px = inGlyph(glyph, x - at[0], y - at[1]) ? Theme.WHITE : fill;
                }
                out[y * FOOT + x] = px;
            }
        }
        return out;
    }

    /** Bakes every shape, its shadow and every glyph white at GUI scale {@code k}, each pixel a {@code k x k} block. */
    static void bake(AtlasPacker packer, int k) {
        for (Map.Entry<String, Shape> e : SHAPES.entrySet()) {
            String shape = e.getKey();
            packer.add(e.getValue().fillKey(), blocks(SIZE, k, (x, y) -> covers(shape, x, y)));
            packer.add(e.getValue().shadowKey(), blocks(FOOT, k, (x, y) -> inShadow(shape, x, y)));
        }
        for (Map.Entry<String, Glyph> e : GLYPHS.entrySet()) {
            String glyph = e.getKey();
            packer.add(e.getValue().key(), blocks(GLYPH, k, (x, y) -> inGlyph(glyph, x, y)));
        }
    }

    /** Which pixels of a piece are set. */
    interface Cells {
        boolean set(int x, int y);
    }

    /** A white {@code n x n} piece of pixel art at {@code k} screen pixels per pixel. */
    static ShapeBaker.Image blocks(int n, int k, Cells cells) {
        int side = n * k;
        int[] out = new int[side * side];
        for (int y = 0; y < side; y++) {
            for (int x = 0; x < side; x++) {
                out[y * side + x] = cells.set(x / k, y / k) ? 0xFFFFFFFF : 0;
            }
        }
        return new ShapeBaker.Image(side, side, out);
    }

    /**
     * Draws a badge with its top-left pixel at {@code x, y} (on a screen pixel: {@link Draw#snap}), faded with the
     * current opacity; nothing for a look this build does not know, or until the atlas is baked.
     */
    public static void draw(GuiGraphicsExtractor g, String color, String glyph, String shape, float x, float y) {
        int fill = color(color);
        Shape s = shape == null ? null : SHAPES.get(shape);
        Glyph gl = glyph == null ? null : GLYPHS.get(glyph);
        if (fill == 0 || s == null || gl == null) {
            return;
        }
        Draw.mask(g, s.shadowKey(), x, y, FOOT, shadow(fill));
        Draw.mask(g, s.fillKey(), x, y, SIZE, fill);
        Draw.mask(g, gl.key(), x + (SIZE - GLYPH) / 2, y + s.glyphY(), GLYPH, Theme.WHITE);
    }
}
