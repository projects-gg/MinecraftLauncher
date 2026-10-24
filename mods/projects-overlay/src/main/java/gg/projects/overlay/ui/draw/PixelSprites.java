package gg.projects.overlay.ui.draw;

import com.mojang.blaze3d.platform.NativeImage;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.ui.Theme;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.Identifier;

/**
 * The launcher's content sprites (packages/ui {@code content-icon.tsx}, ICON1; the Android app draws the same rows): the
 * thing a tile stands for as 16 x 16 pixel art in the golds, drawn on whole pixels. A world is a grass block, never a
 * globe (docs/design.md §6). Uploaded once, on the render thread, the first time one is drawn; 16 GUI pixels keep every
 * sprite pixel whole at every GUI scale.
 */
public final class PixelSprites {
    /** The sprites the overlay draws. */
    public enum Sprite {
        /** A grass block: lit grass on top and over the edges, dark dirt on the sides. */
        WORLD(new String[] {
            "......lmll......",
            "....lhllllml....",
            "..llmllllhllll..",
            "lllhllmllllmhlll",
            "mmllmllhlllllldd",
            "mmmmlmllllhldddd",
            "ommmmmllmldddddk",
            "ooommmmmddddkdkk",
            "ooomommmdddkkdkk",
            "oosooomokdkkkkkk",
            "osooooookdkokkkk",
            "oooosoookkkkkkok",
            "..oooosokokkkk..",
            "....ooookkkk....",
            "......ookk......",
            "................",
        });

        final String[] rows;
        private Identifier texture;

        Sprite(String[] rows) {
            this.rows = rows;
        }
    }

    /** The size every sprite is drawn at, in GUI pixels. */
    public static final int SIZE = 16;
    static final int GOLD_700 = 0xFF8E4A12;
    /** The deep dirt under the golds: gold-700 62 % in obsidian-950, mixed in OKLab as the launcher does. */
    static final int DIRT = oklabMix(GOLD_700, Theme.OBSIDIAN_950, 0.38f);

    private PixelSprites() {
    }

    /** Draws {@code sprite} with its top-left corner on the GUI pixel at {@code x, y}, faded with the current opacity. */
    public static void draw(GuiGraphicsExtractor g, Sprite sprite, float x, float y) {
        Identifier tex = texture(sprite);
        if (tex == null) {
            return;
        }
        Draw.texture(g, tex, Math.round(x), Math.round(y), SIZE, SIZE, 0, 0, SIZE, SIZE, SIZE, SIZE, 0xFFFFFFFF);
    }

    private static Identifier texture(Sprite sprite) {
        if (sprite.texture != null) {
            return sprite.texture;
        }
        int[] argb = pixels(sprite);
        NativeImage image = new NativeImage(SIZE, SIZE, false);
        for (int y = 0; y < SIZE; y++) {
            for (int x = 0; x < SIZE; x++) {
                image.setPixel(x, y, argb[y * SIZE + x]);
            }
        }
        Identifier id = Identifier.fromNamespaceAndPath(OverlayClient.MOD_ID,
            "sprites/" + sprite.name().toLowerCase(java.util.Locale.ROOT));
        Minecraft.getInstance().getTextureManager().register(id, new DynamicTexture(
            () -> "Projects Overlay sprite " + sprite.name(), image));
        sprite.texture = id;
        return id;
    }

    /** A sprite's pixels, row by row, ARGB (transparent where the drawing has "."). */
    static int[] pixels(Sprite sprite) {
        int[] out = new int[SIZE * SIZE];
        for (int y = 0; y < SIZE; y++) {
            String row = sprite.rows[y];
            for (int x = 0; x < SIZE; x++) {
                out[y * SIZE + x] = tone(row.charAt(x));
            }
        }
        return out;
    }

    /** A sprite tone: "h" to "o" the golds 200 to 700, "k" the deep dirt, "." nothing. */
    static int tone(char c) {
        return switch (c) {
            case 'h' -> Theme.GOLD_200;
            case 'l' -> Theme.GOLD_300;
            case 'm' -> Theme.GOLD_400;
            case 'd' -> Theme.GOLD_500;
            case 's' -> Theme.GOLD_600;
            case 'o' -> GOLD_700;
            case 'k' -> DIRT;
            default -> 0;
        };
    }

    /** CSS {@code color-mix(in oklab, a, b t)}: {@code t} of {@code b} into {@code a}, opaque. */
    static int oklabMix(int a, int b, float t) {
        double[] la = oklab(a);
        double[] lb = oklab(b);
        double l = la[0] + (lb[0] - la[0]) * t;
        double m = la[1] + (lb[1] - la[1]) * t;
        double s = la[2] + (lb[2] - la[2]) * t;
        double l_ = l + 0.3963377774 * m + 0.2158037573 * s;
        double m_ = l - 0.1055613458 * m - 0.0638541728 * s;
        double s_ = l - 0.0894841775 * m - 1.2914855480 * s;
        double ll = l_ * l_ * l_;
        double mm = m_ * m_ * m_;
        double ss = s_ * s_ * s_;
        double r = 4.0767416621 * ll - 3.3077115913 * mm + 0.2309699292 * ss;
        double g = -1.2684380046 * ll + 2.6097574011 * mm - 0.3413193965 * ss;
        double bl = -0.0041960863 * ll - 0.7034186147 * mm + 1.7076147010 * ss;
        return 0xFF000000 | (srgb(r) << 16) | (srgb(g) << 8) | srgb(bl);
    }

    private static double[] oklab(int argb) {
        double r = linear((argb >>> 16) & 0xFF);
        double g = linear((argb >>> 8) & 0xFF);
        double b = linear(argb & 0xFF);
        double l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
        double m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
        double s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
        return new double[] {
            0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
        };
    }

    private static double linear(int channel) {
        double c = channel / 255.0;
        return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
    }

    private static int srgb(double linear) {
        double c = Math.max(0.0, Math.min(1.0, linear));
        double v = c <= 0.0031308 ? c * 12.92 : 1.055 * Math.pow(c, 1 / 2.4) - 0.055;
        return (int) Math.round(v * 255.0);
    }
}
