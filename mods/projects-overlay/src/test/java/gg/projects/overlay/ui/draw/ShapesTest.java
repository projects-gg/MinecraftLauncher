package gg.projects.overlay.ui.draw;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.ui.Theme;
import java.io.IOException;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;

class ShapesTest {
    private static int alpha(ShapeBaker.Image image, int x, int y) {
        return (image.argb()[y * image.width() + x] >>> 24) & 0xFF;
    }

    @Test
    void pathsBecomeClosedOutlines() {
        SvgPath square = SvgPath.parse("M10,10 L20,10 20,20 H10 Z m5,5 l1,0 0,1 z", 0.1f);
        assertEquals(2, square.contours().size());
        float[] first = square.contours().get(0);
        assertEquals(10f, first[0]);
        assertEquals(10f, first[1]);
        assertEquals(10f, first[first.length - 2]);
        assertEquals(20f, first[first.length - 1]);
        float[] second = square.contours().get(1);
        assertEquals(15f, second[0]);
        assertEquals(15f, second[1]);
    }

    @Test
    void curvesAndArcsStayOnTheirShape() {
        // A circle of radius 50 around (128, 128) drawn with two arcs.
        SvgPath circle = SvgPath.parse("M78,128a50,50,0,1,0,100,0A50,50,0,1,0,78,128Z", 0.05f);
        float[] c = circle.contours().get(0);
        for (int i = 0; i < c.length; i += 2) {
            double r = Math.hypot(c[i] - 128, c[i + 1] - 128);
            assertEquals(50.0, r, 0.1, "point off the circle");
        }
        SvgPath curve = SvgPath.parse("M0,0C0,10,10,10,10,0S20,-10,20,0Q25,5,30,0T40,0Z", 0.05f);
        float[] q = curve.contours().get(0);
        assertEquals(0f, q[q.length - 1], 1e-4f);
        assertEquals(40f, q[q.length - 2], 1e-4f);
    }

    @Test
    void badPathsAreRefused() {
        assertThrows(IllegalArgumentException.class, () -> SvgPath.parse("10,10 L 5", 0.1f));
        assertThrows(IllegalArgumentException.class, () -> SvgPath.parse("M0,0 A5,5,0,2,0,10,10", 0.1f));
    }

    @Test
    void everyIconOfTheOverlayParsesAndFills() throws IOException {
        Map<Icon, String> paths = Icon.loadPaths();
        assertEquals(Icon.values().length, paths.size());
        for (Map.Entry<Icon, String> e : paths.entrySet()) {
            List<float[]> contours = Icon.contours(e.getValue(), 20);
            assertFalse(contours.isEmpty(), e.getKey().name());
            ShapeBaker.Image image = ShapeBaker.icon(contours, Icon.VIEW_BOX, 10, 2);
            int covered = 0;
            for (int px : image.argb()) {
                if ((px >>> 24) > 128) {
                    covered++;
                }
            }
            assertTrue(covered > 10, e.getKey().name() + " drew almost nothing");
            assertTrue(covered < image.argb().length * 0.9, e.getKey().name() + " filled the whole square");
        }
    }

    @Test
    void theNonZeroRuleKeepsHolesOpen() {
        // A ring: outer square clockwise, inner square counter-clockwise.
        List<float[]> ring = SvgPath.parse("M0,0H256V256H0Z M64,64V192H192V64Z", 0.1f).contours();
        ShapeBaker.Image image = ShapeBaker.icon(ring, 256, 16, 1);
        assertEquals(255, alpha(image, 1, 1));
        assertEquals(0, alpha(image, 8, 8));
    }

    @Test
    void discsAreRoundAndSoftAtTheEdge() {
        ShapeBaker.Image disc = ShapeBaker.disc(8, 2);
        assertEquals(16, disc.width());
        assertEquals(255, alpha(disc, 8, 8));
        assertEquals(0, alpha(disc, 0, 0));
        assertTrue(alpha(disc, 8, 0) > 0, "the top of the disc is drawn");
        int rim = alpha(disc, 1, 3);
        assertTrue(rim > 0 && rim < 255, "the rim is anti-aliased: " + rim);
    }

    @Test
    void framesAreSlicedAroundAOnePixelMiddle() {
        ShapeBaker.FrameSpec spec = Frames.CARD.spec;
        ShapeBaker.Image image = ShapeBaker.frame(spec, 3);
        int c = spec.corner();
        assertEquals((2 * c + 1) * 3, image.width());
        assertEquals(image.width(), image.height());
        // The middle (the stretched part) is the fill, the same across its pixel.
        int mid = c * 3 + 1;
        int center = image.argb()[mid * image.width() + mid];
        assertEquals(Theme.withAlpha(Theme.OBSIDIAN_800, 0.86f), center);
    }

    @Test
    void theToastIsBakedWholeAcross() {
        ShapeBaker.FrameSpec spec = Frames.TOAST.spec;
        ShapeBaker.Image image = ShapeBaker.frame(spec, 3);
        int c = spec.corner();
        int m = spec.margin();
        assertEquals((Frames.TOAST_WIDTH + 2 * m) * 3, image.width());
        assertEquals((2 * c + 1) * 3, image.height());
        // The middle row (the stretched part) is the translucent fill.
        int row = c * 3 + 1;
        int center = image.argb()[row * image.width() + image.width() / 2];
        assertEquals(Theme.withAlpha(Theme.OBSIDIAN_900, Frames.Alphas.TOAST), center);
        // The shadow falls outside the box and fades away at the margin.
        assertTrue(alpha(image, image.width() / 2, image.height() - 1) < 16);
        assertTrue(alpha(image, 0, row) < 16);
    }

    @Test
    void fixedHeightFramesKeepTheirHeight() {
        ShapeBaker.FrameSpec spec = Frames.BUTTON_PRIMARY.spec;
        ShapeBaker.Image image = ShapeBaker.frame(spec, 2);
        assertEquals((spec.fixedHeight() + 2 * spec.margin()) * 2, image.height());
        // A chamfer: the top-left corner is cut, the top-right is square.
        assertEquals(0, alpha(image, 0, 0));
        assertEquals(255, alpha(image, image.width() - 1, 0));
    }

    @Test
    void theAtlasHoldsEveryShapeWithoutOverlap() throws IOException {
        AtlasPacker.Atlas atlas = ShapeSet.bake(2, Icon.loadPaths());
        assertTrue(Integer.bitCount(atlas.width()) == 1 && Integer.bitCount(atlas.height()) == 1);
        List<AtlasPacker.Region> regions = List.copyOf(atlas.regions().values());
        for (int i = 0; i < regions.size(); i++) {
            AtlasPacker.Region a = regions.get(i);
            assertTrue(a.x() + a.width() <= atlas.width() && a.y() + a.height() <= atlas.height());
            for (int j = i + 1; j < regions.size(); j++) {
                AtlasPacker.Region b = regions.get(j);
                boolean apart = a.x() + a.width() <= b.x() || b.x() + b.width() <= a.x()
                    || a.y() + a.height() <= b.y() || b.y() + b.height() <= a.y();
                assertTrue(apart, "regions overlap");
            }
        }
        for (Frames f : Frames.values()) {
            assertTrue(atlas.regions().containsKey(f.key()), f.name());
        }
        assertTrue(atlas.regions().containsKey(Icon.SEND.key(10)));
    }

    @Test
    void theListFadesRunFromThePanelToClear() throws IOException {
        int k = 3;
        ShapeBaker.Image top = ShapeSet.column(ShapeSet.LIST_FADE * k, k, ShapeSet.PANEL_FILL, true, 1.6f);
        ShapeBaker.Image bottom = ShapeSet.column(ShapeSet.LIST_FADE * k, k, ShapeSet.PANEL_FILL, false, 1.6f);
        assertEquals(k, top.width());
        assertEquals(ShapeSet.LIST_FADE * k, top.height());
        int panel = Theme.alpha(ShapeSet.PANEL_FILL);
        // Nearly the panel at the edge, clear at the far end, mostly clear in the middle (words still read).
        assertTrue(alpha(top, 0, 0) > panel - 12 && alpha(top, 0, 0) <= panel);
        assertTrue(alpha(top, k - 1, top.height() - 1) < 6);
        assertTrue(alpha(top, 1, top.height() / 2) < panel / 2);
        assertEquals(alpha(top, 0, 2), alpha(bottom, 0, bottom.height() - 3));
        assertEquals(ShapeSet.PANEL_FILL & 0xFFFFFF, top.argb()[0] & 0xFFFFFF);
        AtlasPacker.Atlas atlas = ShapeSet.bake(2, Icon.loadPaths());
        assertTrue(atlas.regions().containsKey(ShapeSet.FADE_TOP));
        assertTrue(atlas.regions().containsKey(ShapeSet.FADE_BOTTOM));
    }

    @Test
    void aStrokeFollowsTheOutlineOnBothSidesOfIt() {
        // A 40-unit square's outline from 10 to 30, a 3-pixel stroke at 40 pixels: on the edge, gone inside and out.
        List<float[]> square = List.of(new float[] {10, 10, 30, 10, 30, 30, 10, 30});
        ShapeBaker.Image stroke = ShapeBaker.stroke(square, 40f, 40, 1, 3f);
        assertEquals(255, alpha(stroke, 20, 10), "on the top edge");
        assertEquals(255, alpha(stroke, 10, 20), "on the left edge");
        assertEquals(255, alpha(stroke, 20, 9), "half of it outside");
        assertEquals(0, alpha(stroke, 20, 20), "the middle stays open");
        assertEquals(0, alpha(stroke, 2, 2), "nothing far outside");
        assertTrue(alpha(stroke, 20, 11) > 0 && alpha(stroke, 20, 11) < 255, "a soft edge");
        // The mark's light: the fill's shape, strongest at the top, gone at the bottom.
        ShapeBaker.Image fill = ShapeBaker.icon(square, 40f, 40, 1);
        ShapeBaker.Image light = Emblems.light(fill);
        assertTrue(alpha(light, 20, 11) > alpha(light, 20, 28));
        assertEquals(0, alpha(light, 2, 2));
        assertTrue(alpha(light, 20, 11) <= Math.round(Emblems.LIGHT * 255));
    }

    @Test
    void colorMath() {
        assertEquals(0x80FFFFFF, Theme.fade(0xFFFFFFFF, 128 / 255f));
        assertEquals(0xFF0A090C, Theme.mix(0xFF0A090C, 0xFFFFFFFF, 0f));
        assertEquals(0xFFFFFFFF, Theme.over(0xFFFFFFFF, 0xFF000000));
        assertEquals(0xFF000000, Theme.over(0x00FFFFFF, 0xFF000000));
        assertEquals(0, Theme.over(0, 0));
    }
}
