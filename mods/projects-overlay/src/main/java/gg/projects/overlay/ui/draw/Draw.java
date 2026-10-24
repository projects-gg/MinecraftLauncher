package gg.projects.overlay.ui.draw;

import gg.projects.overlay.ui.Theme;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.resources.Identifier;
import net.minecraft.util.FormattedCharSequence;
import org.joml.Matrix3x2fStack;

/**
 * The overlay's drawing calls, in GUI pixels: baked frames (nine-slice or three-slice), discs, icons and
 * strips from {@link Shapes}, hairlines one screen pixel thick, and text. Everything drawn here is
 * multiplied by the current opacity ({@link #alpha}), so a whole card fades as one. Positions may be
 * fractional (motion); static layouts use whole pixels and stay sharp. Render thread only; no allocation
 * besides what the game's own drawing calls make.
 */
public final class Draw {
    /** Lets the measurement driver compare with everything in one shared layer, as before. */
    public static volatile boolean separateStrata = true;

    private static final float[] ALPHA = new float[16];
    private static int depth;

    static {
        ALPHA[0] = 1f;
    }

    private Draw() {
    }

    /**
     * Starts a new layer of the game's GUI drawing for what follows. The game places each element above the
     * highest earlier element it overlaps by scanning every element of the current layer, so in the HUD's layer
     * each toast element paid for the whole HUD, and in one panel-wide layer each element for the whole panel.
     * Layers draw strictly in order and overlapping elements inside a layer keep their order, so what reaches
     * the screen does not change; only the scans get short. (A screen's blur sits before the screen's own layer,
     * so this never moves anything in or out of the blur.)
     */
    public static void stratum(GuiGraphicsExtractor g) {
        if (separateStrata) {
            g.nextStratum();
        }
    }

    /** The opacity everything is drawn with now. */
    public static float alpha() {
        return ALPHA[depth];
    }

    /** Multiplies the opacity until {@link #popAlpha}. */
    public static void pushAlpha(float a) {
        if (depth + 1 < ALPHA.length) {
            ALPHA[depth + 1] = ALPHA[depth] * Math.max(0f, Math.min(1f, a));
            depth++;
        }
    }

    public static void popAlpha() {
        if (depth > 0) {
            depth--;
        }
    }

    public static int color(int argb) {
        return Theme.fade(argb, ALPHA[depth]);
    }

    /** Screen pixels per GUI pixel. */
    public static int scale() {
        return Math.max(1, Minecraft.getInstance().getWindow().getGuiScale());
    }

    /** The nearest screen-pixel position, in GUI pixels. */
    public static float snap(float v) {
        int k = scale();
        return Math.round(v * k) / (float) k;
    }

    /** A baked frame around the box {@code x, y, w, h} (its shadow and ring fall outside the box). */
    public static void frame(GuiGraphicsExtractor g, Frames frame, float x, float y, float w, float h) {
        frame(g, frame, x, y, w, h, 0xFFFFFFFF);
    }

    public static void frame(GuiGraphicsExtractor g, Frames frame, float x, float y, float w, float h, int tint) {
        int color = color(tint);
        if (Theme.alpha(color) == 0) {
            return;
        }
        AtlasPacker.Region r = Shapes.ready() ? Shapes.region(frame.key()) : null;
        if (r == null) {
            fallbackFill(g, frame, x, y, w, h, color);
            return;
        }
        ShapeBaker.FrameSpec spec = frame.spec;
        int c = spec.corner();
        int m = spec.margin();
        int iw = spec.fixedWidth() > 0 ? spec.fixedWidth() : Math.round(w);
        int ih = spec.fixedHeight() > 0 ? spec.fixedHeight() : Math.round(h);
        int ow = iw + 2 * m;
        int oh = ih + 2 * m;
        // Texels per GUI pixel, from the side that is baked 2c + 1 GUI pixels long.
        float kt = (spec.fixedWidth() > 0 ? r.height() : r.width()) / (float) (2 * c + 1);
        int tw = Shapes.textureWidth();
        int th = Shapes.textureHeight();
        Identifier tex = Shapes.texture();
        Matrix3x2fStack pose = g.pose();
        pose.pushMatrix();
        pose.translate(x - m, y - m);
        int ck = Math.round(c * kt);
        int k1 = Math.max(1, Math.round(kt));
        int midW = Math.max(0, ow - 2 * c);
        float u0 = r.x();
        float u1 = r.x() + c * kt;
        float u2 = r.x() + (c + 1) * kt;
        if (spec.fixedWidth() > 0) {
            // Baked whole across: the top, the middle row stretched down, the bottom.
            int fullW = r.width();
            int midH = Math.max(0, oh - 2 * c);
            blit(g, tex, 0, 0, u0, r.y(), ow, c, fullW, ck, tw, th, color);
            if (midH > 0) {
                blit(g, tex, 0, c, u0, r.y() + c * kt, ow, midH, fullW, k1, tw, th, color);
            }
            blit(g, tex, 0, oh - c, u0, r.y() + (c + 1) * kt, ow, c, fullW, ck, tw, th, color);
        } else if (spec.fixedHeight() > 0) {
            int fullH = r.height();
            blit(g, tex, 0, 0, u0, r.y(), c, oh, ck, fullH, tw, th, color);
            if (midW > 0) {
                blit(g, tex, c, 0, u1, r.y(), midW, oh, k1, fullH, tw, th, color);
            }
            blit(g, tex, ow - c, 0, u2, r.y(), c, oh, ck, fullH, tw, th, color);
        } else {
            int midH = Math.max(0, oh - 2 * c);
            float v0 = r.y();
            float v1 = r.y() + c * kt;
            float v2 = r.y() + (c + 1) * kt;
            blit(g, tex, 0, 0, u0, v0, c, c, ck, ck, tw, th, color);
            blit(g, tex, ow - c, 0, u2, v0, c, c, ck, ck, tw, th, color);
            blit(g, tex, 0, oh - c, u0, v2, c, c, ck, ck, tw, th, color);
            blit(g, tex, ow - c, oh - c, u2, v2, c, c, ck, ck, tw, th, color);
            if (midW > 0) {
                blit(g, tex, c, 0, u1, v0, midW, c, k1, ck, tw, th, color);
                blit(g, tex, c, oh - c, u1, v2, midW, c, k1, ck, tw, th, color);
            }
            if (midH > 0) {
                blit(g, tex, 0, c, u0, v1, c, midH, ck, k1, tw, th, color);
                blit(g, tex, ow - c, c, u2, v1, c, midH, ck, k1, tw, th, color);
                if (midW > 0) {
                    blit(g, tex, c, c, u1, v1, midW, midH, k1, k1, tw, th, color);
                }
            }
        }
        pose.popMatrix();
    }

    /** A white disc tinted {@code color}, centred on {@code cx, cy}. */
    public static void disc(GuiGraphicsExtractor g, int diameter, float cx, float cy, int color) {
        int c = color(color);
        if (Theme.alpha(c) == 0) {
            return;
        }
        AtlasPacker.Region r = Shapes.ready() ? Shapes.region(ShapeSet.discKey(diameter)) : null;
        float x = cx - diameter / 2f;
        float y = cy - diameter / 2f;
        if (r == null) {
            fill(g, x, y, x + diameter, y + diameter, c);
            return;
        }
        Matrix3x2fStack pose = g.pose();
        pose.pushMatrix();
        pose.translate(x, y);
        blit(g, Shapes.texture(), 0, 0, r.x(), r.y(), diameter, diameter, r.width(), r.height(),
            Shapes.textureWidth(), Shapes.textureHeight(), c);
        pose.popMatrix();
    }

    /** An icon {@code size} GUI pixels square (one of {@link Icon#SIZES}) tinted {@code color}. */
    public static void icon(GuiGraphicsExtractor g, Icon icon, int size, float x, float y, int color) {
        int c = color(color);
        if (Theme.alpha(c) == 0) {
            return;
        }
        AtlasPacker.Region r = Shapes.ready() ? Shapes.region(icon.key(size)) : null;
        if (r == null) {
            return;
        }
        Matrix3x2fStack pose = g.pose();
        pose.pushMatrix();
        pose.translate(x, y);
        blit(g, Shapes.texture(), 0, 0, r.x(), r.y(), size, size, r.width(), r.height(), Shapes.textureWidth(),
            Shapes.textureHeight(), c);
        pose.popMatrix();
    }

    /**
     * A baked white square of {@code size} GUI pixels by its atlas name, tinted {@code color} (a crew mark's frame,
     * light, edge and emblem). Nothing until the atlas is baked.
     */
    public static void mask(GuiGraphicsExtractor g, String key, float x, float y, int size, int color) {
        int c = color(color);
        if (key == null || Theme.alpha(c) == 0) {
            return;
        }
        AtlasPacker.Region r = Shapes.ready() ? Shapes.region(key) : null;
        if (r == null) {
            return;
        }
        Matrix3x2fStack pose = g.pose();
        pose.pushMatrix();
        pose.translate(x, y);
        blit(g, Shapes.texture(), 0, 0, r.x(), r.y(), size, size, r.width(), r.height(), Shapes.textureWidth(),
            Shapes.textureHeight(), c);
        pose.popMatrix();
    }

    /** A baked horizontal strip stretched over {@code w x h} (shadows, row fades). */
    public static void strip(GuiGraphicsExtractor g, String key, float x, float y, int w, int h) {
        int c = color(0xFFFFFFFF);
        AtlasPacker.Region r = Shapes.ready() ? Shapes.region(key) : null;
        if (r == null || Theme.alpha(c) == 0) {
            return;
        }
        Matrix3x2fStack pose = g.pose();
        pose.pushMatrix();
        pose.translate(x, y);
        // Every row of a strip is the same: one texel row stretched over the height.
        blit(g, Shapes.texture(), 0, 0, r.x(), r.y(), w, h, r.width(), 1, Shapes.textureWidth(),
            Shapes.textureHeight(), c);
        pose.popMatrix();
    }

    /** A baked vertical strip stretched over {@code w x h} (a list's fades at its top and bottom). */
    public static void column(GuiGraphicsExtractor g, String key, float x, float y, int w, int h) {
        int c = color(0xFFFFFFFF);
        AtlasPacker.Region r = Shapes.ready() ? Shapes.region(key) : null;
        if (r == null || Theme.alpha(c) == 0) {
            return;
        }
        Matrix3x2fStack pose = g.pose();
        pose.pushMatrix();
        pose.translate(x, y);
        // Every column of it is the same: one texel column stretched over the width.
        blit(g, Shapes.texture(), 0, 0, r.x(), r.y(), w, h, 1, r.height(), Shapes.textureWidth(),
            Shapes.textureHeight(), c);
        pose.popMatrix();
    }

    /** A filled rectangle, edges on screen pixels. */
    public static void fill(GuiGraphicsExtractor g, float x0, float y0, float x1, float y1, int argb) {
        int c = color(argb);
        if (Theme.alpha(c) == 0 || x1 <= x0 || y1 <= y0) {
            return;
        }
        int k = scale();
        Matrix3x2fStack pose = g.pose();
        pose.pushMatrix();
        pose.scale(1f / k, 1f / k);
        g.fill(Math.round(x0 * k), Math.round(y0 * k), Math.round(x1 * k), Math.round(y1 * k), c);
        pose.popMatrix();
    }

    /** A horizontal hairline one screen pixel thick. */
    public static void hairline(GuiGraphicsExtractor g, float x0, float x1, float y, int argb) {
        int c = color(argb);
        if (Theme.alpha(c) == 0) {
            return;
        }
        int k = scale();
        Matrix3x2fStack pose = g.pose();
        pose.pushMatrix();
        pose.scale(1f / k, 1f / k);
        int py = Math.round(y * k);
        g.fill(Math.round(x0 * k), py, Math.round(x1 * k), py + 1, c);
        pose.popMatrix();
    }

    /** A vertical hairline one screen pixel thick. */
    public static void vhairline(GuiGraphicsExtractor g, float x, float y0, float y1, int argb) {
        int c = color(argb);
        if (Theme.alpha(c) == 0) {
            return;
        }
        int k = scale();
        Matrix3x2fStack pose = g.pose();
        pose.pushMatrix();
        pose.scale(1f / k, 1f / k);
        int px = Math.round(x * k);
        g.fill(px, Math.round(y0 * k), px + 1, Math.round(y1 * k), c);
        pose.popMatrix();
    }

    public static void text(GuiGraphicsExtractor g, Font font, FormattedCharSequence text, float x, float y,
        int argb) {
        int c = color(argb);
        if (Theme.alpha(c) == 0) {
            return;
        }
        Matrix3x2fStack pose = g.pose();
        pose.pushMatrix();
        pose.translate(x, y);
        g.text(font, text, 0, 0, c, false);
        pose.popMatrix();
    }

    public static void text(GuiGraphicsExtractor g, Font font, String text, float x, float y, int argb) {
        int c = color(argb);
        if (Theme.alpha(c) == 0 || text.isEmpty()) {
            return;
        }
        Matrix3x2fStack pose = g.pose();
        pose.pushMatrix();
        pose.translate(x, y);
        g.text(font, text, 0, 0, c, false);
        pose.popMatrix();
    }

    /**
     * A region of a texture ({@code uw x vh} texels at {@code u, v} of a {@code tw x th} texture) over the box
     * {@code x, y, w, h}, tinted {@code tint} and faded with the current opacity (stickers).
     */
    public static void texture(GuiGraphicsExtractor g, Identifier tex, float x, float y, int w, int h, float u,
        float v, int uw, int vh, int tw, int th, int tint) {
        int c = color(tint);
        if (Theme.alpha(c) == 0) {
            return;
        }
        Matrix3x2fStack pose = g.pose();
        pose.pushMatrix();
        pose.translate(x, y);
        blit(g, tex, 0, 0, u, v, w, h, uw, vh, tw, th, c);
        pose.popMatrix();
    }

    private static void blit(GuiGraphicsExtractor g, Identifier tex, int x, int y, float u, float v, int w, int h,
        int uw, int vh, int tw, int th, int color) {
        g.blit(RenderPipelines.GUI_TEXTURED, tex, x, y, u, v, w, h, uw, vh, tw, th, color);
    }

    /** Before the first bake: the frame's fill as a plain rectangle, so nothing is missing. */
    private static void fallbackFill(GuiGraphicsExtractor g, Frames frame, float x, float y, float w, float h,
        int tint) {
        int fill = frame.spec.fillTop() != 0 ? frame.spec.fillTop() : frame.spec.border();
        if (fill == 0) {
            return;
        }
        float height = frame.spec.fixedHeight() > 0 ? frame.spec.fixedHeight() : h;
        float width = frame.spec.fixedWidth() > 0 ? frame.spec.fixedWidth() : w;
        g.fill(Math.round(x), Math.round(y), Math.round(x + width), Math.round(y + height),
            Theme.fade(fill, Theme.alpha(tint) / 255f));
    }
}
