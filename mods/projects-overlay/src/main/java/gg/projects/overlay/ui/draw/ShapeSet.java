package gg.projects.overlay.ui.draw;

import gg.projects.overlay.ui.Theme;
import java.util.Map;

/** Bakes every frame, disc, strip and icon the overlay draws for one GUI scale into one atlas. Pure. */
public final class ShapeSet {
    /** Disc diameters in GUI pixels: presence dots, switch and slider thumbs, avatars. */
    public static final int[] DISCS = {3, 5, 8, 16, 18};
    /** The shadow left of the panel, and the fades at the ends of a sideways-scrolling row. */
    public static final String PANEL_SHADOW = "strip:panel_shadow";
    public static final String FADE_LEFT = "strip:fade_left";
    public static final String FADE_RIGHT = "strip:fade_right";
    public static final int PANEL_SHADOW_WIDTH = 18;
    public static final int FADE_WIDTH = 14;
    /** The fades at the top and bottom of a list while there is more above or below it (CHAT2-O). */
    public static final String FADE_TOP = "strip:fade_top";
    public static final String FADE_BOTTOM = "strip:fade_bottom";
    public static final int LIST_FADE = 8;
    /** The same fades into a popover's surface (the sticker picker's grid when it scrolls). */
    public static final String POPOVER_FADE_TOP = "strip:popover_fade_top";
    public static final String POPOVER_FADE_BOTTOM = "strip:popover_fade_bottom";
    /** The panel's surface, which the row fades blend into. */
    public static final int PANEL_FILL = Theme.withAlpha(Theme.OBSIDIAN_900, Frames.Alphas.PANEL);

    private static final String[] DISC_KEYS = new String[DISCS.length];

    static {
        for (int i = 0; i < DISCS.length; i++) {
            DISC_KEYS[i] = "disc:" + DISCS[i];
        }
    }

    private ShapeSet() {
    }

    /** A disc's name in the atlas (made once for the baked sizes: discs are looked up every frame). */
    public static String discKey(int diameter) {
        for (int i = 0; i < DISCS.length; i++) {
            if (DISCS[i] == diameter) {
                return DISC_KEYS[i];
            }
        }
        return "disc:" + diameter;
    }

    public static AtlasPacker.Atlas bake(int k, Map<Icon, String> paths) {
        return bake(k, paths, null);
    }

    /** Everything, with the crews' marks: their frames always, their emblems when {@code emblems} has them. */
    public static AtlasPacker.Atlas bake(int k, Map<Icon, String> paths, Map<String, String> emblems) {
        AtlasPacker packer = new AtlasPacker();
        Emblems.bake(packer, k, emblems);
        for (Frames f : Frames.values()) {
            packer.add(f.key(), ShapeBaker.frame(f.spec, k));
        }
        for (int d : DISCS) {
            packer.add(discKey(d), ShapeBaker.disc(d, k));
        }
        packer.add(PANEL_SHADOW, strip(PANEL_SHADOW_WIDTH * k, k, 0x000000, 0f, 0.32f, 2.2f));
        packer.add(FADE_LEFT, strip(FADE_WIDTH * k, k, PANEL_FILL, 1f, 0f, 1f));
        packer.add(FADE_RIGHT, strip(FADE_WIDTH * k, k, PANEL_FILL, 0f, 1f, 1f));
        packer.add(FADE_TOP, column(LIST_FADE * k, k, PANEL_FILL, true, 1.6f));
        packer.add(FADE_BOTTOM, column(LIST_FADE * k, k, PANEL_FILL, false, 1.6f));
        packer.add(POPOVER_FADE_TOP, column(LIST_FADE * k, k, Frames.POPOVER.spec.fillTop(), true, 1.6f));
        packer.add(POPOVER_FADE_BOTTOM, column(LIST_FADE * k, k, Frames.POPOVER.spec.fillTop(), false, 1.6f));
        if (paths != null) {
            for (Map.Entry<Icon, String> e : paths.entrySet()) {
                for (int size : Icon.SIZES) {
                    int pixels = size * k;
                    packer.add(e.getKey().key(size),
                        ShapeBaker.icon(Icon.contours(e.getValue(), pixels), Icon.VIEW_BOX, size, k));
                }
            }
        }
        return packer.pack();
    }

    /**
     * A one GUI pixel tall horizontal strip whose alpha goes from {@code from} (left) to {@code to} (right)
     * of the color's own alpha, eased by {@code power}.
     */
    static ShapeBaker.Image strip(int width, int k, int color, float from, float to, float power) {
        int[] argb = new int[width * k];
        float base = Theme.alpha(color) == 0 ? 1f : Theme.alpha(color) / 255f;
        for (int x = 0; x < width; x++) {
            float t = (x + 0.5f) / width;
            float a = from + (to - from) * (float) Math.pow(t, power);
            int c = Theme.withAlpha(color, a * base);
            for (int y = 0; y < k; y++) {
                argb[y * width + x] = c;
            }
        }
        return new ShapeBaker.Image(width, k, argb);
    }

    /**
     * A one GUI pixel wide vertical strip {@code height} texels tall: the color's own alpha at the top when
     * {@code fromTop} (else at the bottom), clear at the other end. Eased by {@code power}, so it is mostly clear
     * and words under it read until right at the edge.
     */
    static ShapeBaker.Image column(int height, int k, int color, boolean fromTop, float power) {
        int[] argb = new int[k * height];
        float base = Theme.alpha(color) == 0 ? 1f : Theme.alpha(color) / 255f;
        for (int y = 0; y < height; y++) {
            float t = (y + 0.5f) / height;
            float a = (float) Math.pow(fromTop ? 1f - t : t, power);
            int c = Theme.withAlpha(color, a * base);
            for (int x = 0; x < k; x++) {
                argb[y * k + x] = c;
            }
        }
        return new ShapeBaker.Image(k, height, argb);
    }
}
