package gg.projects.overlay.ui.draw;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.ui.Theme;
import java.util.List;
import org.junit.jupiter.api.Test;

/** The profile badges' pixel art (BADGES-UI): every look of the contract, put together as the game draws it. */
class MarkSpritesTest {
    private static final int S = MarkSprites.SIZE;
    private static final int F = MarkSprites.FOOT;

    @Test
    void everyLookOfTheContractIsDrawnAndNothingElse() {
        assertEquals(Protocol.MARK_SHAPES, List.copyOf(MarkSprites.shapes()));
        assertEquals(Protocol.MARK_GLYPHS, List.copyOf(MarkSprites.glyphs()));
        for (String color : Protocol.MARK_COLORS) {
            assertEquals(0xFF, Theme.alpha(MarkSprites.color(color)), color);
        }
        assertEquals(0, MarkSprites.color("gold"));
        assertEquals(0, MarkSprites.color(null));
        assertNull(MarkSprites.compose("hexagon", "check", "red"));
        assertNull(MarkSprites.compose("seal", "skull", "red"));
        assertNull(MarkSprites.compose("seal", "check", "gold"));
    }

    @Test
    void theColoursAreTheLaunchersPalette() {
        // packages/ui styles.css --color-mark-<name> (docs/tasks/badges.md, "Renk paleti").
        int[] hex = {0x2A8AF0, 0xE0434A, 0x9B5EEB, 0x219453, 0xD95F1A, 0x138D89, 0x6673E8, 0xDF378B, 0x7F7A9F};
        for (int i = 0; i < hex.length; i++) {
            assertEquals(0xFF000000 | hex[i], MarkSprites.color(Protocol.MARK_COLORS.get(i)),
                Protocol.MARK_COLORS.get(i));
        }
    }

    @Test
    void everyShapeFillsItsNineByNineSquareEvenly() {
        for (String shape : MarkSprites.shapes()) {
            boolean top = false;
            boolean bottom = false;
            boolean left = false;
            boolean right = false;
            for (int i = 0; i < S; i++) {
                top |= MarkSprites.covers(shape, i, 0);
                bottom |= MarkSprites.covers(shape, i, S - 1);
                left |= MarkSprites.covers(shape, 0, i);
                right |= MarkSprites.covers(shape, S - 1, i);
                for (int j = 0; j < S; j++) {
                    assertEquals(MarkSprites.covers(shape, i, j), MarkSprites.covers(shape, S - 1 - i, j),
                        shape + " is the same both sides");
                }
            }
            assertTrue(top && bottom && left && right, shape + " reaches every edge of its square");
            assertTrue(MarkSprites.covers(shape, S / 2, S / 2), shape + " has a middle");
            assertTrue(!MarkSprites.covers(shape, -1, 4) && !MarkSprites.covers(shape, S, 4), "nothing outside");
        }
        // The seal points a pixel out on each side; the round badge is flat there; the shield is flat on top with
        // square shoulders and pointed below.
        assertTrue(MarkSprites.covers("seal", 4, 0) && !MarkSprites.covers("seal", 3, 0));
        assertTrue(MarkSprites.covers("round", 3, 0) && MarkSprites.covers("round", 5, 0));
        assertTrue(MarkSprites.covers("shield", 0, 0) && MarkSprites.covers("shield", 8, 0));
        assertTrue(MarkSprites.covers("shield", 4, 8) && !MarkSprites.covers("shield", 3, 8));
    }

    @Test
    void everyGlyphSitsInsideItsBadgeWithColourAllAroundIt() {
        for (String shape : MarkSprites.shapes()) {
            for (String glyph : MarkSprites.glyphs()) {
                int[] at = MarkSprites.glyphAt(shape, glyph);
                assertNotNull(at);
                int pixels = 0;
                for (int y = 0; y < MarkSprites.GLYPH; y++) {
                    for (int x = 0; x < MarkSprites.GLYPH; x++) {
                        if (!MarkSprites.inGlyph(glyph, x, y)) {
                            continue;
                        }
                        pixels++;
                        int bx = at[0] + x;
                        int by = at[1] + y;
                        String where = glyph + " in " + shape + " at " + bx + "," + by;
                        assertTrue(MarkSprites.covers(shape, bx, by), where);
                        // Its four neighbours are glyph or colour: it never touches the badge's edge.
                        assertTrue(MarkSprites.covers(shape, bx - 1, by) && MarkSprites.covers(shape, bx + 1, by)
                            && MarkSprites.covers(shape, bx, by - 1) && MarkSprites.covers(shape, bx, by + 1), where);
                    }
                }
                assertTrue(pixels >= 7, glyph + " draws something");
            }
        }
    }

    @Test
    void glyphsSitInTheMiddleAndAPixelHigherInTheShield() {
        for (String glyph : MarkSprites.glyphs()) {
            for (String shape : MarkSprites.shapes()) {
                double[] c = middle(shape, glyph);
                double middle = (S - 1) / 2.0;
                String where = glyph + " in " + shape + ": " + c[0] + "," + c[1];
                assertEquals(middle, c[0], 0.5, where);
                // The shield's point leaves less room below: its glyph a pixel higher, as the launcher's.
                assertEquals(shape.equals("shield") ? middle - 1 : middle, c[1], 0.5, where);
            }
        }
    }

    /** The middle of the box around a glyph's pixels in its badge (a crown's thin prongs count as much as its band). */
    private static double[] middle(String shape, String glyph) {
        int[] at = MarkSprites.glyphAt(shape, glyph);
        int minX = MarkSprites.GLYPH;
        int maxX = -1;
        int minY = MarkSprites.GLYPH;
        int maxY = -1;
        for (int y = 0; y < MarkSprites.GLYPH; y++) {
            for (int x = 0; x < MarkSprites.GLYPH; x++) {
                if (MarkSprites.inGlyph(glyph, x, y)) {
                    minX = Math.min(minX, x);
                    maxX = Math.max(maxX, x);
                    minY = Math.min(minY, y);
                    maxY = Math.max(maxY, y);
                }
            }
        }
        return new double[] {at[0] + (minX + maxX) / 2.0, at[1] + (minY + maxY) / 2.0};
    }

    @Test
    void theChecksLegsRunAtFortyFiveDegrees() {
        // Its long leg from the top right and its short one from the left meet at the bottom, each pixel a step left
        // or right of the one above (the launcher's check, SOC-VERIFY).
        int[][] longLeg = {{4, 0}, {3, 1}, {2, 2}, {1, 3}};
        int[][] shortLeg = {{0, 2}, {1, 3}};
        for (int[] p : longLeg) {
            assertTrue(MarkSprites.inGlyph("check", p[0], p[1]), p[0] + "," + p[1]);
        }
        for (int[] p : shortLeg) {
            assertTrue(MarkSprites.inGlyph("check", p[0], p[1]), p[0] + "," + p[1]);
        }
        assertTrue(MarkSprites.inGlyph("check", 1, 4), "the point at the bottom");
        for (int x = 0; x < MarkSprites.GLYPH; x++) {
            assertEquals(x == 1, MarkSprites.inGlyph("check", x, 4), "one pixel at the bottom");
        }
    }

    @Test
    void aBadgeIsItsShapeInItsColourTheGlyphInWhiteAndTheShadowOutside() {
        for (String shape : MarkSprites.shapes()) {
            for (String glyph : MarkSprites.glyphs()) {
                for (String color : Protocol.MARK_COLORS) {
                    int[] px = MarkSprites.compose(shape, glyph, color);
                    int fill = MarkSprites.color(color);
                    int shadow = MarkSprites.shadow(fill);
                    int[] at = MarkSprites.glyphAt(shape, glyph);
                    assertEquals(F * F, px.length);
                    int shadows = 0;
                    for (int y = 0; y < F; y++) {
                        for (int x = 0; x < F; x++) {
                            int want;
                            if (MarkSprites.covers(shape, x, y)) {
                                want = MarkSprites.inGlyph(glyph, x - at[0], y - at[1]) ? Theme.WHITE : fill;
                            } else if (MarkSprites.covers(shape, x - 1, y - 1)) {
                                want = shadow;
                                shadows++;
                            } else {
                                want = 0;
                            }
                            assertEquals(want, px[y * F + x], shape + "/" + glyph + "/" + color + " at " + x + "," + y);
                        }
                    }
                    // A pixel right and down: never along the top or the left.
                    assertTrue(shadows >= S, "a shadow under " + shape);
                    for (int i = 0; i < F; i++) {
                        assertTrue(px[i] != shadow && px[i * F] != shadow, "no shadow over or left of " + shape);
                    }
                }
            }
        }
    }

    @Test
    void theShadowIsTheGamesTextShadowOfTheColour() {
        // A quarter of each channel, as the game shadows its text.
        assertEquals(0xFF381012, MarkSprites.shadow(0xFFE0434A));
        assertEquals(0xFF0A223C, MarkSprites.shadow(0xFF2A8AF0));
        for (String color : Protocol.MARK_COLORS) {
            int fill = MarkSprites.color(color);
            int shadow = MarkSprites.shadow(fill);
            assertEquals(0xFF, Theme.alpha(shadow));
            assertTrue(luminance(shadow) < luminance(fill) / 3, color + " has a dark shadow");
        }
    }

    private static double luminance(int argb) {
        return 0.2126 * ((argb >> 16) & 0xFF) + 0.7152 * ((argb >> 8) & 0xFF) + 0.0722 * (argb & 0xFF);
    }

    @Test
    void thePiecesAreBakedAsWholeBlocksAtEveryScale() {
        for (int k = 1; k <= 4; k++) {
            AtlasPacker packer = new AtlasPacker();
            MarkSprites.bake(packer, k);
            AtlasPacker.Atlas atlas = packer.pack();
            for (String shape : MarkSprites.shapes()) {
                AtlasPacker.Region fill = atlas.regions().get("mark:" + shape);
                AtlasPacker.Region shadow = atlas.regions().get("mark:" + shape + ":shadow");
                assertEquals(S * k, fill.width());
                assertEquals(S * k, fill.height());
                assertEquals(F * k, shadow.width());
                for (int y = 0; y < S * k; y++) {
                    for (int x = 0; x < S * k; x++) {
                        int texel = atlas.argb()[(fill.y() + y) * atlas.width() + fill.x() + x];
                        assertEquals(MarkSprites.covers(shape, x / k, y / k) ? 0xFFFFFFFF : 0, texel,
                            shape + " at x" + k);
                    }
                }
            }
            for (String glyph : MarkSprites.glyphs()) {
                AtlasPacker.Region r = atlas.regions().get("mark:glyph:" + glyph);
                assertEquals(MarkSprites.GLYPH * k, r.width());
                int x = MarkSprites.GLYPH * k - 1;
                int y = 2 * k;
                int texel = atlas.argb()[(r.y() + y) * atlas.width() + r.x() + x];
                assertEquals(MarkSprites.inGlyph(glyph, x / k, y / k) ? 0xFFFFFFFF : 0, texel, glyph);
            }
        }
    }

    @Test
    void theShapesAtlasCarriesThem() {
        AtlasPacker.Atlas atlas = ShapeSet.bake(2, null);
        for (String shape : MarkSprites.shapes()) {
            assertTrue(atlas.regions().containsKey("mark:" + shape));
            assertTrue(atlas.regions().containsKey("mark:" + shape + ":shadow"));
        }
        for (String glyph : MarkSprites.glyphs()) {
            assertTrue(atlas.regions().containsKey("mark:glyph:" + glyph));
        }
    }
}
