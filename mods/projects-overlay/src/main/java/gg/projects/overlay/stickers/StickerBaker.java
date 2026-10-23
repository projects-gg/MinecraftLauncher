package gg.projects.overlay.stickers;

/**
 * Bakes the sticker atlas at the size it is drawn, in screen pixels (like the overlay's shapes,
 * {@code ShapeBaker}): the game then draws each sticker one texel to one pixel, sharp at every GUI scale.
 * Smaller sizes average the covered source area, larger ones blend the four nearest; both on premultiplied
 * alpha, so the white cut edge never gets a dark fringe. Pure: images are plain ARGB arrays.
 */
public final class StickerBaker {
    /**
     * Every sticker of an atlas at {@code px} pixels: the atlas's grid again, each cell {@code px + 2} wide with
     * a clear pixel around the sticker (no neighbour bleeds in under filtering).
     */
    public record Sheet(int px, int columns, int rows, int width, int height, int[] argb) {
        public int cellX(int column) {
            return column * (px + 2) + 1;
        }

        public int cellY(int row) {
            return row * (px + 2) + 1;
        }
    }

    private StickerBaker() {
    }

    /** The atlas {@code src} ({@code srcW} wide, tiles of {@code tile}) baked at {@code px} pixels a sticker. */
    public static Sheet bake(int[] src, int srcW, int tile, int columns, int rows, int px) {
        int cell = px + 2;
        int width = columns * cell;
        int height = rows * cell;
        int[] out = new int[width * height];
        for (int row = 0; row < rows; row++) {
            for (int column = 0; column < columns; column++) {
                resample(src, srcW, column * tile, row * tile, tile, out, width, column * cell + 1, row * cell + 1,
                    px);
            }
        }
        return new Sheet(px, columns, rows, width, height, out);
    }

    /** One {@code tile} square of {@code src} at ({@code sx}, {@code sy}) into {@code px} pixels of {@code dst}. */
    static void resample(int[] src, int srcW, int sx, int sy, int tile, int[] dst, int dstW, int dx, int dy,
        int px) {
        if (px >= tile) {
            bilinear(src, srcW, sx, sy, tile, dst, dstW, dx, dy, px);
            return;
        }
        double scale = tile / (double) px;
        // The source columns (and rows) under each destination column, and how much of each it covers.
        int[][] spans = new int[px][];
        double[][] weights = new double[px][];
        for (int i = 0; i < px; i++) {
            double from = i * scale;
            double to = from + scale;
            int first = (int) Math.floor(from);
            int last = Math.min(tile - 1, (int) Math.ceil(to) - 1);
            int n = last - first + 1;
            spans[i] = new int[n];
            weights[i] = new double[n];
            for (int k = 0; k < n; k++) {
                int s = first + k;
                double w = Math.min(to, s + 1) - Math.max(from, s);
                spans[i][k] = s;
                weights[i][k] = Math.max(0, w);
            }
        }
        double area = scale * scale;
        for (int y = 0; y < px; y++) {
            int[] ys = spans[y];
            double[] wy = weights[y];
            for (int x = 0; x < px; x++) {
                int[] xs = spans[x];
                double[] wx = weights[x];
                double a = 0;
                double r = 0;
                double g = 0;
                double b = 0;
                for (int j = 0; j < ys.length; j++) {
                    int rowStart = (sy + ys[j]) * srcW + sx;
                    for (int i = 0; i < xs.length; i++) {
                        double w = wx[i] * wy[j];
                        int c = src[rowStart + xs[i]];
                        double ca = ((c >>> 24) & 0xFF) * w;
                        a += ca;
                        r += ((c >>> 16) & 0xFF) * ca;
                        g += ((c >>> 8) & 0xFF) * ca;
                        b += (c & 0xFF) * ca;
                    }
                }
                dst[(dy + y) * dstW + dx + x] = pack(a / area, r, g, b, a);
            }
        }
    }

    private static void bilinear(int[] src, int srcW, int sx, int sy, int tile, int[] dst, int dstW, int dx, int dy,
        int px) {
        double scale = tile / (double) px;
        for (int y = 0; y < px; y++) {
            double fy = Math.max(0, Math.min(tile - 1, (y + 0.5) * scale - 0.5));
            int y0 = (int) Math.floor(fy);
            int y1 = Math.min(tile - 1, y0 + 1);
            double ty = fy - y0;
            for (int x = 0; x < px; x++) {
                double fx = Math.max(0, Math.min(tile - 1, (x + 0.5) * scale - 0.5));
                int x0 = (int) Math.floor(fx);
                int x1 = Math.min(tile - 1, x0 + 1);
                double tx = fx - x0;
                double a = 0;
                double r = 0;
                double g = 0;
                double b = 0;
                int[] xsx = {x0, x1, x0, x1};
                int[] ysy = {y0, y0, y1, y1};
                double[] w = {(1 - tx) * (1 - ty), tx * (1 - ty), (1 - tx) * ty, tx * ty};
                for (int k = 0; k < 4; k++) {
                    int c = src[(sy + ysy[k]) * srcW + sx + xsx[k]];
                    double ca = ((c >>> 24) & 0xFF) * w[k];
                    a += ca;
                    r += ((c >>> 16) & 0xFF) * ca;
                    g += ((c >>> 8) & 0xFF) * ca;
                    b += (c & 0xFF) * ca;
                }
                dst[(dy + y) * dstW + dx + x] = pack(a, r, g, b, a);
            }
        }
    }

    /** Straight-alpha ARGB from an alpha and colour sums premultiplied by {@code weight}. */
    private static int pack(double alpha, double r, double g, double b, double weight) {
        int a = (int) Math.round(Math.max(0, Math.min(255, alpha)));
        if (a == 0 || weight <= 0) {
            return 0;
        }
        int rr = (int) Math.round(Math.max(0, Math.min(255, r / weight)));
        int gg = (int) Math.round(Math.max(0, Math.min(255, g / weight)));
        int bb = (int) Math.round(Math.max(0, Math.min(255, b / weight)));
        return (a << 24) | (rr << 16) | (gg << 8) | bb;
    }
}
