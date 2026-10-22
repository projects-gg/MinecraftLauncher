package gg.projects.overlay.ui;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.navigation.ScreenRectangle;
import net.minecraft.util.FormattedCharSequence;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;

class PreparedLineTest {
    private static final int BONE = 0xFFF3EBDD;
    private static final Object SOURCE = new Object();

    /** A layout with no glyphs (nothing to check against released textures). */
    private static Font.PreparedText layout() {
        return new Font.PreparedText() {
            @Override
            public void visit(Font.GlyphVisitor visitor) {
            }

            @Override
            public ScreenRectangle bounds() {
                return null;
            }
        };
    }

    private static PreparedLine line() {
        FormattedCharSequence text = sink -> true;
        return (PreparedLine) PreparedLine.of(text);
    }

    @AfterEach
    void restore() {
        PreparedLine.keepLayouts = true;
    }

    @Test
    void wrapsOnceAndLeavesTheEmptyLineAlone() {
        PreparedLine line = line();
        assertSame(line, PreparedLine.of(line));
        assertSame(FormattedCharSequence.EMPTY, PreparedLine.of(FormattedCharSequence.EMPTY));
        assertNull(PreparedLine.of(null));
    }

    @Test
    void passesTheCharactersThrough() {
        FormattedCharSequence text = FormattedCharSequence.forward("Ayşe", net.minecraft.network.chat.Style.EMPTY);
        StringBuilder seen = new StringBuilder();
        PreparedLine.of(text).accept((index, style, codepoint) -> {
            seen.appendCodePoint(codepoint);
            return true;
        });
        assertEquals("Ayşe", seen.toString());
    }

    @Test
    void keepsOnlyOpaqueTextWithoutABackground() {
        assertTrue(PreparedLine.keeps(BONE, 0));
        // A fading line changes colour on every frame: laid out each time, never kept.
        assertFalse(PreparedLine.keeps(0x80F3EBDD, 0));
        assertFalse(PreparedLine.keeps(BONE, 0x40000000));
        PreparedLine.keepLayouts = false;
        assertFalse(PreparedLine.keeps(BONE, 0));
    }

    @Test
    void drawsWithTheKeptLayoutWhileNothingChanges() {
        PreparedLine line = line();
        Font.PreparedText made = layout();
        assertNull(line.reuse(null, SOURCE, 0f, 0f, BONE, false, false, 0));
        line.keep(null, SOURCE, 0f, 0f, BONE, false, false, 0, made);
        assertSame(made, line.reuse(null, SOURCE, 0f, 0f, BONE, false, false, 0));
        assertSame(made, line.reuse(null, SOURCE, 0f, 0f, BONE, false, false, 0));
    }

    @Test
    void laysOutAgainWhenAnythingItDependsOnChanges() {
        PreparedLine line = line();
        line.keep(null, SOURCE, 0f, 0f, BONE, false, false, 0, layout());
        // Another glyph source: the GUI scale, "Force Unicode Font" or a resource reload changed it.
        assertNull(line.reuse(null, new Object(), 0f, 0f, BONE, false, false, 0));
        assertNull(line.reuse(null, SOURCE, 1f, 0f, BONE, false, false, 0));
        assertNull(line.reuse(null, SOURCE, 0f, 0.5f, BONE, false, false, 0));
        assertNull(line.reuse(null, SOURCE, 0f, 0f, 0xFFBDB6C3, false, false, 0));
        assertNull(line.reuse(null, SOURCE, 0f, 0f, BONE, true, false, 0));
        assertNull(line.reuse(null, SOURCE, 0f, 0f, BONE, false, true, 0));
        assertNull(line.reuse(null, SOURCE, 0f, 0f, BONE, false, false, 0x40000000));
    }

    @Test
    void forgetsADroppedLayout() {
        PreparedLine line = line();
        line.keep(null, SOURCE, 0f, 0f, BONE, false, false, 0, layout());
        line.drop();
        assertNull(line.reuse(null, SOURCE, 0f, 0f, BONE, false, false, 0));
    }
}
