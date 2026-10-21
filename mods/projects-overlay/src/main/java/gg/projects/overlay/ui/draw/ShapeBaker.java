package gg.projects.overlay.ui.draw;

import gg.projects.overlay.ui.Theme;
import java.util.Arrays;
import java.util.List;

/**
 * Draws the overlay's shapes into pixels at the screen's own resolution (GUI scale {@code k} pixels per
 * GUI pixel), anti-aliased: rounded and chamfered frames with a hairline border, an inner top highlight
 * and a soft shadow; discs; and vector icons. The game then only stretches and tints these (nine-slice),
 * so every edge stays sharp at any GUI scale. Pure: images are plain ARGB arrays.
 */
public final class ShapeBaker {
    /** A baked image. */
    public record Image(int width, int height, int[] argb) {
    }

    /** The outline of a frame. */
    public enum Outline { ROUNDED, CHAMFER }

    /**
     * What a frame looks like, in GUI pixels.
     *
     * @param radius corner radius (ROUNDED) or diagonal cut (CHAMFER)
     * @param fillTop fill at the top; {@code fillBottom} at the bottom (a vertical gradient when they differ)
     * @param border a hairline (one screen pixel) just inside the edge; 0 for none
     * @param highlight a second hairline under the top border, fading out along the sides; 0 for none
     * @param ring an outer ring of {@code ringWidth} GUI pixels around the shape (focus); 0 for none
     * @param shadow shadow color at its densest; 0 for none
     * @param shadowBlur shadow softness
     * @param shadowY how far down the shadow falls
     * @param fixedHeight 0 for a nine-slice frame; else the frame is baked at this height and only stretches
     *     sideways (fixed-height controls, so a gradient or a pill end keeps its shape)
     * @param fixedWidth 0, or the one width the frame is drawn at (the toast card): it is baked whole across
     *     and only its middle row stretches, three slices instead of nine
     */
    public record FrameSpec(Outline outline, float radius, int fillTop, int fillBottom, int border, int highlight,
        int ring, float ringWidth, int shadow, float shadowBlur, float shadowY, int fixedHeight, int fixedWidth) {

        public static FrameSpec rounded(float radius, int fill, int border) {
            return new FrameSpec(Outline.ROUNDED, radius, fill, fill, border, 0, 0, 0f, 0, 0f, 0f, 0, 0);
        }

        public FrameSpec withHighlight(int color) {
            return new FrameSpec(outline, radius, fillTop, fillBottom, border, color, ring, ringWidth, shadow,
                shadowBlur, shadowY, fixedHeight, fixedWidth);
        }

        public FrameSpec withShadow(int color, float blur, float y) {
            return new FrameSpec(outline, radius, fillTop, fillBottom, border, highlight, ring, ringWidth, color,
                blur, y, fixedHeight, fixedWidth);
        }

        public FrameSpec withRing(int color, float width) {
            return new FrameSpec(outline, radius, fillTop, fillBottom, border, highlight, color, width, shadow,
                shadowBlur, shadowY, fixedHeight, fixedWidth);
        }

        public FrameSpec withGradient(int top, int bottom) {
            return new FrameSpec(outline, radius, top, bottom, border, highlight, ring, ringWidth, shadow,
                shadowBlur, shadowY, fixedHeight, fixedWidth);
        }

        public FrameSpec fixed(int height) {
            return new FrameSpec(outline, radius, fillTop, fillBottom, border, highlight, ring, ringWidth, shadow,
                shadowBlur, shadowY, height, fixedWidth);
        }

        public FrameSpec fixedWidth(int width) {
            return new FrameSpec(outline, radius, fillTop, fillBottom, border, highlight, ring, ringWidth, shadow,
                shadowBlur, shadowY, fixedHeight, width);
        }

        public FrameSpec chamfer() {
            return new FrameSpec(Outline.CHAMFER, radius, fillTop, fillBottom, border, highlight, ring, ringWidth,
                shadow, shadowBlur, shadowY, fixedHeight, fixedWidth);
        }

        /** Room around the shape for the ring and the shadow, in whole GUI pixels. */
        public int margin() {
            float m = 0f;
            if (ring != 0) {
                m = Math.max(m, ringWidth + 1f);
            }
            if (shadow != 0) {
                m = Math.max(m, shadowBlur * 1.6f + Math.abs(shadowY));
            }
            return (int) Math.ceil(m);
        }

        /** Corner piece size in GUI pixels: margin plus the corner itself. */
        public int corner() {
            return margin() + (int) Math.ceil(radius);
        }
    }

    private ShapeBaker() {
    }

    /**
     * A frame for nine-slice drawing: corners of {@link FrameSpec#corner()} GUI pixels and a one GUI pixel
     * middle that stretches. With a fixed height only the middle column stretches, with a fixed width only
     * the middle row.
     */
    public static Image frame(FrameSpec spec, int k) {
        int c = spec.corner();
        int m = spec.margin();
        int square = 2 * c + 1 - 2 * m;
        int boxW = spec.fixedWidth() > 0 ? spec.fixedWidth() : square;
        int boxH = spec.fixedHeight() > 0 ? spec.fixedHeight() : square;
        int guiW = boxW + 2 * m;
        int guiH = boxH + 2 * m;
        int w = guiW * k;
        int h = guiH * k;
        int[] out = new int[w * h];
        float x0 = m;
        float y0 = m;
        float x1 = m + boxW;
        float y1 = m + boxH;
        float r = Math.min(spec.radius(), Math.min(boxW, boxH) / 2f);
        float sigma = Math.max(0.5f, spec.shadowBlur() / 2f);
        for (int py = 0; py < h; py++) {
            float v = (py + 0.5f) / k;
            for (int px = 0; px < w; px++) {
                float u = (px + 0.5f) / k;
                float d = distance(spec.outline(), u, v, x0, y0, x1, y1, r) * k;
                float inside = coverage(d);
                int color = 0;
                if (spec.shadow() != 0 && inside < 1f) {
                    float ds = Math.max(0f, distance(spec.outline(), u, v - spec.shadowY(), x0, y0, x1, y1, r));
                    float fall = (float) Math.exp(-(ds * ds) / (2f * sigma * sigma));
                    color = Theme.fade(spec.shadow(), fall * (1f - inside));
                }
                if (spec.ring() != 0) {
                    float rw = spec.ringWidth() * k;
                    float ring = coverage(d - rw) - inside;
                    if (ring > 0f) {
                        color = Theme.over(Theme.fade(spec.ring(), ring), color);
                    }
                }
                if (inside > 0f) {
                    float t = boxH <= 0 ? 0f : Math.max(0f, Math.min(1f, (v - y0) / boxH));
                    int fill = spec.fillTop() == spec.fillBottom() ? spec.fillTop()
                        : Theme.mix(spec.fillTop(), spec.fillBottom(), t);
                    int layer = Theme.fade(fill, inside);
                    if (spec.border() != 0) {
                        float edge = inside - coverage(d + 1f);
                        if (edge > 0f) {
                            layer = Theme.over(Theme.fade(spec.border(), edge), layer);
                        }
                    }
                    if (spec.highlight() != 0) {
                        float line = coverage(d + 1f) - coverage(d + 2f);
                        if (line > 0f) {
                            float top = topness(spec.outline(), u, v, x0, y0, x1, y1, r);
                            if (top > 0f) {
                                layer = Theme.over(Theme.fade(spec.highlight(), line * top), layer);
                            }
                        }
                    }
                    color = Theme.over(layer, color);
                }
                out[py * w + px] = color;
            }
        }
        return new Image(w, h, out);
    }

    /** A white disc of {@code diameter} GUI pixels, for tinting. */
    public static Image disc(float diameter, int k) {
        int size = (int) Math.ceil(diameter * k);
        int[] out = new int[size * size];
        float rr = diameter * k / 2f;
        float cx = size / 2f;
        for (int y = 0; y < size; y++) {
            for (int x = 0; x < size; x++) {
                float dx = x + 0.5f - cx;
                float dy = y + 0.5f - cx;
                float d = (float) Math.sqrt(dx * dx + dy * dy) - rr;
                out[y * size + x] = Theme.withAlpha(0xFFFFFF, coverage(d));
            }
        }
        return new Image(size, size, out);
    }

    /**
     * A white icon of {@code size} GUI pixels from path outlines drawn on a {@code viewBox} square, filled
     * with the non-zero rule: 4 sub-rows per pixel, exact coverage along each row.
     */
    public static Image icon(List<float[]> contours, float viewBox, float size, int k) {
        int px = Math.max(1, Math.round(size * k));
        float scale = px / viewBox;
        float[] cover = new float[px];
        float[] alpha = new float[px * px];
        int subRows = 4;
        float[] xs = new float[256];
        int[] dirs = new int[256];
        for (int row = 0; row < px; row++) {
            Arrays.fill(cover, 0f);
            for (int sub = 0; sub < subRows; sub++) {
                float sy = row + (sub + 0.5f) / subRows;
                int n = 0;
                for (float[] c : contours) {
                    int points = c.length / 2;
                    for (int i = 0; i < points; i++) {
                        float ax = c[2 * i] * scale;
                        float ay = c[2 * i + 1] * scale;
                        int j = (i + 1) % points;
                        float bx = c[2 * j] * scale;
                        float by = c[2 * j + 1] * scale;
                        if ((ay <= sy && by > sy) || (by <= sy && ay > sy)) {
                            if (n == xs.length) {
                                xs = Arrays.copyOf(xs, n * 2);
                                dirs = Arrays.copyOf(dirs, n * 2);
                            }
                            xs[n] = ax + (sy - ay) / (by - ay) * (bx - ax);
                            dirs[n] = by > ay ? 1 : -1;
                            n++;
                        }
                    }
                }
                sortCrossings(xs, dirs, n);
                int winding = 0;
                for (int i = 0; i < n - 1; i++) {
                    winding += dirs[i];
                    if (winding != 0) {
                        addSpan(cover, xs[i], xs[i + 1], 1f / subRows);
                    }
                }
            }
            System.arraycopy(cover, 0, alpha, row * px, px);
        }
        int[] out = new int[px * px];
        for (int i = 0; i < out.length; i++) {
            out[i] = Theme.withAlpha(0xFFFFFF, Math.min(1f, alpha[i]));
        }
        return new Image(px, px, out);
    }

    private static void sortCrossings(float[] xs, int[] dirs, int n) {
        for (int i = 1; i < n; i++) {
            float x = xs[i];
            int d = dirs[i];
            int j = i - 1;
            while (j >= 0 && xs[j] > x) {
                xs[j + 1] = xs[j];
                dirs[j + 1] = dirs[j];
                j--;
            }
            xs[j + 1] = x;
            dirs[j + 1] = d;
        }
    }

    /** Adds {@code weight} times the covered part of each pixel between {@code from} and {@code to}. */
    private static void addSpan(float[] cover, float from, float to, float weight) {
        float a = Math.max(0f, from);
        float b = Math.min(cover.length, to);
        if (b <= a) {
            return;
        }
        int first = (int) a;
        int last = (int) Math.ceil(b) - 1;
        if (first == last) {
            cover[first] += (b - a) * weight;
            return;
        }
        cover[first] += (first + 1 - a) * weight;
        for (int x = first + 1; x < last; x++) {
            cover[x] += weight;
        }
        cover[last] += (b - last) * weight;
    }

    /** Pixel coverage from a signed distance in pixels (negative inside). */
    static float coverage(float d) {
        return Math.max(0f, Math.min(1f, 0.5f - d));
    }

    /** Signed distance (GUI pixels) to the shape, negative inside. */
    static float distance(Outline outline, float u, float v, float x0, float y0, float x1, float y1, float r) {
        if (outline == Outline.CHAMFER) {
            float d = Math.max(Math.max(x0 - u, u - x1), Math.max(y0 - v, v - y1));
            float cut = r;
            float invSqrt2 = 0.70710677f;
            float tl = (cut - ((u - x0) + (v - y0))) * invSqrt2;
            float br = (cut - ((x1 - u) + (y1 - v))) * invSqrt2;
            return Math.max(d, Math.max(tl, br));
        }
        float cx = (x0 + x1) / 2f;
        float cy = (y0 + y1) / 2f;
        float bx = (x1 - x0) / 2f - r;
        float by = (y1 - y0) / 2f - r;
        float qx = Math.abs(u - cx) - bx;
        float qy = Math.abs(v - cy) - by;
        float ox = Math.max(qx, 0f);
        float oy = Math.max(qy, 0f);
        return (float) Math.sqrt(ox * ox + oy * oy) + Math.min(Math.max(qx, qy), 0f) - r;
    }

    /** How much a point on the edge faces up: 1 along the top, 0 on the sides and below. */
    private static float topness(Outline outline, float u, float v, float x0, float y0, float x1, float y1,
        float r) {
        float e = 0.25f;
        float dUp = distance(outline, u, v - e, x0, y0, x1, y1, r);
        float dDown = distance(outline, u, v + e, x0, y0, x1, y1, r);
        float dLeft = distance(outline, u - e, v, x0, y0, x1, y1, r);
        float dRight = distance(outline, u + e, v, x0, y0, x1, y1, r);
        float gx = dRight - dLeft;
        float gy = dDown - dUp;
        float len = (float) Math.sqrt(gx * gx + gy * gy);
        if (len <= 1e-6f) {
            return 0f;
        }
        float up = -gy / len;
        return up <= 0f ? 0f : up * up;
    }
}
