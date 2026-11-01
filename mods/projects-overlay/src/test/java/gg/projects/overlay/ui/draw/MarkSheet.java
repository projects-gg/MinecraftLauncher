package gg.projects.overlay.ui.draw;

import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.ui.Theme;
import java.awt.Font;
import java.awt.Graphics2D;
import java.awt.RenderingHints;
import java.awt.image.BufferedImage;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.util.List;
import javax.imageio.ImageIO;

/**
 * BADGES-UI's review sheet, a development tool and no test: every shape and glyph in each colour at GUI scale 1, 2 and
 * 3 on the panel's surface, put together as {@link MarkSprites#draw} puts them on the screen (three tinted layers), and
 * below them a friend row (online and offline), a crew chat's sender, a toast and the tooltips at GUI 2 and 3, with the
 * mod's own baked frames and its bundled Figtree. The words there are drawn by java.awt, not by the game: close, not
 * the same. Run it with the test classpath and {@code src/main/resources}: {@code java -cp ... MarkSheet <out.png>}.
 */
public final class MarkSheet {
    private static final int LABEL_W = 112;
    private static final int[] SCALES = {1, 2, 3};
    private static final int CELL_GAP = 4;
    private static final int COLOR_GAP = 14;
    private static final int ROW_H = 3 * MarkSprites.FOOT + 8;
    private static final int PANEL = Theme.OBSIDIAN_900;
    /** A daytime sky behind the toasts, which float over the world. */
    private static final int SKY = 0xFF7FA5C9;
    /** The scales of the mocks, and how tall and wide one scale's mocks are in GUI pixels. */
    private static final int[] MOCK_SCALES = {2, 3};
    private static final int MOCK_H = 136;
    private static final int MOCK_W = 378;

    private final BufferedImage image;
    private final Font strong;
    private final Font medium;

    private MarkSheet(BufferedImage image) throws IOException {
        this.image = image;
        this.strong = font("figtree-semibold.ttf");
        this.medium = font("figtree-medium.ttf");
    }

    public static void main(String[] args) throws Exception {
        if (args.length != 1) {
            System.err.println("usage: MarkSheet <out.png>");
            System.exit(2);
        }
        List<String> colors = Protocol.MARK_COLORS;
        int cellW = 0;
        for (int k : SCALES) {
            cellW += MarkSprites.FOOT * k + CELL_GAP;
        }
        int sheetH = 22 + MarkSprites.shapes().size() * MarkSprites.glyphs().size() * ROW_H;
        int width = LABEL_W + colors.size() * (cellW + COLOR_GAP);
        int mockH = 26;
        for (int k : MOCK_SCALES) {
            width = Math.max(width, 12 + MOCK_W * k);
            mockH += MOCK_H * k + 10;
        }
        BufferedImage image = new BufferedImage(width, sheetH + mockH, BufferedImage.TYPE_INT_ARGB);
        MarkSheet sheet = new MarkSheet(image);
        sheet.fill(0, 0, width, sheetH + mockH, PANEL);
        sheet.grid(colors, cellW);
        sheet.mocks(sheetH);
        BufferedImage out = new BufferedImage(width, sheetH + mockH, BufferedImage.TYPE_INT_RGB);
        out.getGraphics().drawImage(image, 0, 0, null);
        File file = new File(args[0]);
        ImageIO.write(out, "png", file);
        System.out.println(file + " " + width + "x" + (sheetH + mockH));
    }

    // ---- The grid ---------------------------------------------------------------------------------------------

    private void grid(List<String> colors, int cellW) {
        Graphics2D g = graphics();
        g.setFont(new Font(Font.SANS_SERIF, Font.PLAIN, 11));
        g.setColor(new java.awt.Color(Theme.ASH_400));
        for (int c = 0; c < colors.size(); c++) {
            g.drawString(colors.get(c) + "  x1 x2 x3", LABEL_W + c * (cellW + COLOR_GAP), 14);
        }
        int y = 22;
        for (String shape : MarkSprites.shapes()) {
            for (String glyph : MarkSprites.glyphs()) {
                g.drawString(shape + " · " + glyph, 6, y + ROW_H / 2 + 4);
                for (int c = 0; c < colors.size(); c++) {
                    int x = LABEL_W + c * (cellW + COLOR_GAP);
                    for (int k : SCALES) {
                        // Bottom-aligned, as badges of one row would be.
                        badge(shape, glyph, colors.get(c), x / (float) k, (y + 3 * MarkSprites.FOOT
                            - MarkSprites.FOOT * k) / (float) k, k, 1f);
                        x += MarkSprites.FOOT * k + CELL_GAP;
                    }
                }
                y += ROW_H;
            }
        }
        g.dispose();
    }

    /** A badge with its top-left pixel at GUI {@code x, y} at scale {@code k}, the three layers the game draws. */
    private void badge(String shape, String glyph, String color, float x, float y, int k, float alpha) {
        int fill = MarkSprites.color(color);
        int shadow = MarkSprites.shadow(fill);
        int[] at = MarkSprites.glyphAt(shape, glyph);
        int x0 = Math.round(x * k);
        int y0 = Math.round(y * k);
        for (int py = 0; py < MarkSprites.FOOT; py++) {
            for (int px = 0; px < MarkSprites.FOOT; px++) {
                if (MarkSprites.inShadow(shape, px, py)) {
                    block(x0 + px * k, y0 + py * k, k, Theme.fade(shadow, alpha));
                }
            }
        }
        for (int py = 0; py < MarkSprites.SIZE; py++) {
            for (int px = 0; px < MarkSprites.SIZE; px++) {
                if (MarkSprites.covers(shape, px, py)) {
                    block(x0 + px * k, y0 + py * k, k, Theme.fade(fill, alpha));
                }
            }
        }
        for (int py = 0; py < MarkSprites.GLYPH; py++) {
            for (int px = 0; px < MarkSprites.GLYPH; px++) {
                if (MarkSprites.inGlyph(glyph, px, py)) {
                    block(x0 + (at[0] + px) * k, y0 + (at[1] + py) * k, k, Theme.fade(Theme.WHITE, alpha));
                }
            }
        }
    }

    // ---- The mocks --------------------------------------------------------------------------------------------

    private void mocks(int top) {
        Graphics2D g = graphics();
        g.setFont(new Font(Font.SANS_SERIF, Font.PLAIN, 11));
        g.setColor(new java.awt.Color(Theme.ASH_400));
        g.drawString("GUI 2 and 3 (words: java.awt with the bundled Figtree, not the game's renderer)", 6, top + 16);
        g.dispose();
        int y = top + 26;
        for (int k : MOCK_SCALES) {
            mockRow(k, 6, y);
            y += MOCK_H * k + 10;
        }
    }

    /** At GUI {@code k}: two friend rows, a crew chat's sender over a message, a toast, and both tooltips. */
    private void mockRow(int k, int left, int top) {
        float gx = left / (float) k;
        float gy = top / (float) k;
        // The friends list, 150 GUI pixels wide.
        friend(k, gx, gy, "Kuzey_77", "Projects'te", true, "red", "check", "seal");
        friend(k, gx, gy + 24, "NightOwl_42", "Dün görüldü", false, "blue", "check", "seal");
        // Crew chat: the name over the group, the bubble under it.
        float cx = gx;
        float cy = gy + 52;
        float nameW = width(medium, 7.5f, k, "PixelPanda_9");
        text(medium, 7.5f, k, "PixelPanda_9", cx + 1, cy + 4.5f, Theme.ASH_300);
        badgeBeside(k, cx + 1 + nameW, cy + 4.5f, "purple", "check", "seal", 1f);
        frame(Frames.BUBBLE_THEIRS, k, cx, cy + 10, 82, 18);
        text(medium, 8.5f, k, "Akşam bedwars?", cx + 6, cy + 19, Theme.BONE_300);
        // A toast over the world.
        float tx = gx + 175;
        fill(Math.round((tx - 6) * k), Math.round((gy - 6) * k), Math.round(196 * k), Math.round(64 * k), SKY);
        frame(Frames.TOAST, k, tx, gy, Frames.TOAST_WIDTH, 44);
        disc(k, tx + 8 + 9, gy + 7 + 9, 9, Theme.withAlpha(Theme.GOLD_700, 0.4f));
        float titleW = width(strong, 8.5f, k, "Kuzey_77");
        text(strong, 8.5f, k, "Kuzey_77", tx + 33, gy + 12, Theme.BONE_50);
        badgeBeside(k, tx + 33 + titleW, gy + 12, "orange", "star", "seal", 1f);
        text(medium, 8.5f, k, "Akşam bedwars?", tx + 33, gy + 23, Theme.BONE_300);
        text(medium, 7.5f, k, "Açmak için F7 tuşuna bas", tx + 33, gy + 36, Theme.ASH_300);
        // The tooltips: one badge, then several.
        float px = gx + 175;
        float py = gy + 64;
        tooltip(k, px, py, new String[][] {{"Projects yöneticisi", "red", "check", "seal"}});
        tooltip(k, px + 90, py, new String[][] {
            {"Projects yöneticisi", "red", "check", "seal"},
            {"Premium Club üyesi", "orange", "star", "seal"},
            {"Bulut AFK kullanıcısı", "teal", "cloud", "round"},
            {"Ar-Ge ekibi", "pink", "flask", "round"},
            {"İçerik üreticisi", "slate", "play", "shield"}});
    }

    /** A friend row as {@code FriendList} lays it out (its face, the name with its badge, where they are). */
    private void friend(int k, float x, float y, String name, String where, boolean online, String color,
        String glyph, String shape) {
        float face = 16;
        float fx = x + face / 2f + 1;
        float fy = y + 12;
        frame(Frames.FACE, k, fx - face / 2f, fy - face / 2f, face, face);
        text(strong, 7.5f, k, name.substring(0, 1), fx - 2.5f, fy, online ? Theme.BONE_100 : Theme.ASH_400);
        if (online) {
            disc(k, fx + 6, fy + 6, 4, Theme.OBSIDIAN_900);
            disc(k, fx + 6, fy + 6, 2.5f, Theme.JADE_400);
        }
        float tx = x + face + 8;
        float nameW = width(strong, 8.5f, k, name);
        text(strong, 8.5f, k, name, tx, y + 8, online ? Theme.BONE_100 : Theme.ASH_400);
        badgeBeside(k, tx + nameW, y + 8, color, glyph, shape, online ? 1f : 0.7f);
        text(medium, 7.5f, k, where, tx, y + 17, online ? Theme.ASH_300 : Theme.ASH_400);
    }

    /** The badge right of a name ending at {@code nameEnd}, centred on {@code centerY}, as {@code Marks.draw} puts it. */
    private void badgeBeside(int k, float nameEnd, float centerY, String color, String glyph, String shape, float a) {
        badge(shape, glyph, color, snap(nameEnd + 3f, k), snap(centerY - MarkSprites.SIZE / 2f, k), k, a);
    }

    /** {@code MarkTip}'s tooltip at {@code x, y}: one name, or a line each with its badge. */
    private void tooltip(int k, float x, float y, String[][] marks) {
        boolean one = marks.length == 1;
        float widest = 0;
        for (String[] m : marks) {
            widest = Math.max(widest, width(medium, 7.5f, k, m[0]));
        }
        float w = 12 + (one ? 0 : MarkSprites.SIZE + 4) + widest;
        float h = 6 + (one ? 10 : marks.length * 12);
        frame(Frames.TOOLTIP, k, x, y, w, h);
        for (int i = 0; i < marks.length; i++) {
            if (one) {
                text(medium, 7.5f, k, marks[i][0], x + 6, y + 3 + 5, Theme.BONE_100);
            } else {
                float mid = y + 3 + i * 12 + 6;
                badge(marks[i][3], marks[i][2], marks[i][1], x + 6, snap(mid - MarkSprites.SIZE / 2f, k), k, 1f);
                text(medium, 7.5f, k, marks[i][0], x + 6 + MarkSprites.SIZE + 4, mid, Theme.BONE_100);
            }
        }
    }

    // ---- Drawing helpers --------------------------------------------------------------------------------------

    /** {@code Draw.snap}: the nearest screen pixel, in GUI pixels. */
    private static float snap(float v, int k) {
        return Math.round(v * k) / (float) k;
    }

    private static Font font(String file) throws IOException {
        try (InputStream in = MarkSheet.class.getResourceAsStream("/assets/projects_overlay/font/" + file)) {
            if (in == null) {
                throw new IOException("put src/main/resources on the classpath: no " + file);
            }
            return Font.createFont(Font.TRUETYPE_FONT, in);
        } catch (java.awt.FontFormatException e) {
            throw new IOException(e);
        }
    }

    private Graphics2D graphics() {
        Graphics2D g = image.createGraphics();
        g.setRenderingHint(RenderingHints.KEY_TEXT_ANTIALIASING, RenderingHints.VALUE_TEXT_ANTIALIAS_ON);
        g.setRenderingHint(RenderingHints.KEY_FRACTIONALMETRICS, RenderingHints.VALUE_FRACTIONALMETRICS_ON);
        return g;
    }

    /** A name's width in GUI pixels, rounded up as the game's font measures. */
    private float width(Font face, float size, int k, String text) {
        Graphics2D g = graphics();
        g.setFont(face.deriveFont(size * k));
        float w = (float) g.getFontMetrics().getStringBounds(text, g).getWidth();
        g.dispose();
        return (float) Math.ceil(w / k);
    }

    /** Words whose capitals sit centred on GUI {@code centerY} ({@code Fonts.centeredY}: the baseline 7 below the top). */
    private void text(Font face, float size, int k, String text, float x, float centerY, int argb) {
        Graphics2D g = graphics();
        g.setFont(face.deriveFont(size * k));
        g.setColor(new java.awt.Color(argb, true));
        float baseline = centerY + size * 0.7f / 2f;
        g.drawString(text, x * k, baseline * k);
        g.dispose();
    }

    private void disc(int k, float cx, float cy, float r, int argb) {
        for (int y = Math.round((cy - r) * k); y < Math.round((cy + r) * k); y++) {
            for (int x = Math.round((cx - r) * k); x < Math.round((cx + r) * k); x++) {
                float dx = (x + 0.5f) / k - cx;
                float dy = (y + 0.5f) / k - cy;
                float cover = Math.max(0f, Math.min(1f, (r - (float) Math.sqrt(dx * dx + dy * dy)) * k + 0.5f));
                over(x, y, Theme.fade(argb, cover));
            }
        }
    }

    /** One of the mod's frames, baked for GUI {@code k} and sliced as {@code Draw.frame} slices it. */
    private void frame(Frames f, int k, float x, float y, float w, float h) {
        ShapeBaker.FrameSpec spec = f.spec;
        ShapeBaker.Image baked = ShapeBaker.frame(spec, k);
        int c = spec.corner();
        int m = spec.margin();
        int iw = spec.fixedWidth() > 0 ? spec.fixedWidth() : Math.round(w);
        int ih = spec.fixedHeight() > 0 ? spec.fixedHeight() : Math.round(h);
        int ow = (iw + 2 * m) * k;
        int oh = (ih + 2 * m) * k;
        int ox = Math.round((x - m) * k);
        int oy = Math.round((y - m) * k);
        int ck = c * k;
        if (spec.fixedWidth() > 0) {
            int bw = baked.width();
            blit(baked, 0, 0, bw, ck, ox, oy, ow, ck);
            blit(baked, 0, ck, bw, k, ox, oy + ck, ow, oh - 2 * ck);
            blit(baked, 0, ck + k, bw, ck, ox, oy + oh - ck, ow, ck);
        } else if (spec.fixedHeight() > 0) {
            int bh = baked.height();
            blit(baked, 0, 0, ck, bh, ox, oy, ck, oh);
            blit(baked, ck, 0, k, bh, ox + ck, oy, ow - 2 * ck, oh);
            blit(baked, ck + k, 0, ck, bh, ox + ow - ck, oy, ck, oh);
        } else {
            int[] su = {0, ck, ck + k};
            int[] sw = {ck, k, ck};
            int[] du = {0, ck, ow - ck};
            int[] dw = {ck, ow - 2 * ck, ck};
            int[] dv = {0, ck, oh - ck};
            int[] dh = {ck, oh - 2 * ck, ck};
            for (int j = 0; j < 3; j++) {
                for (int i = 0; i < 3; i++) {
                    blit(baked, su[i], su[j], sw[i], sw[j], ox + du[i], oy + dv[j], dw[i], dh[j]);
                }
            }
        }
    }

    /** A region of a baked image stretched (nearest) over a box of screen pixels, source over. */
    private void blit(ShapeBaker.Image src, int sx, int sy, int sw, int sh, int dx, int dy, int dw, int dh) {
        if (dw <= 0 || dh <= 0) {
            return;
        }
        for (int y = 0; y < dh; y++) {
            int ty = sy + Math.min(sh - 1, y * sh / dh);
            for (int x = 0; x < dw; x++) {
                int tx = sx + Math.min(sw - 1, x * sw / dw);
                over(dx + x, dy + y, src.argb()[ty * src.width() + tx]);
            }
        }
    }

    private void block(int x, int y, int k, int argb) {
        for (int j = 0; j < k; j++) {
            for (int i = 0; i < k; i++) {
                over(x + i, y + j, argb);
            }
        }
    }

    private void fill(int x, int y, int w, int h, int argb) {
        for (int j = y; j < y + h; j++) {
            for (int i = x; i < x + w; i++) {
                over(i, j, argb);
            }
        }
    }

    private void over(int x, int y, int argb) {
        if (x < 0 || y < 0 || x >= image.getWidth() || y >= image.getHeight() || Theme.alpha(argb) == 0) {
            return;
        }
        image.setRGB(x, y, Theme.over(argb, image.getRGB(x, y)));
    }
}
