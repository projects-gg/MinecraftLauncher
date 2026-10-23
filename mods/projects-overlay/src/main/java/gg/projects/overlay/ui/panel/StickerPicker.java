package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.cursor.CursorTypes;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.stickers.StickerArt;
import gg.projects.overlay.stickers.StickerSet;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.util.FormattedCharSequence;
import org.joml.Matrix3x2fStack;

/**
 * The sticker picker over the message field (CHAT2 §8.5): the recently sent ones on top, then the set's four
 * themes, eight to a row; the name of the one under the pointer (or picked with the arrows) at the bottom.
 * A press or Enter sends it at once and the picker closes (as a reply when one is being written). It opens
 * from the sticker button with a short rise; nothing about it runs while it is closed.
 */
public final class StickerPicker extends PanelWidget {
    private static final int PAD = 5;
    private static final int SECTION_GAP = 5;
    private static final long OPEN_MS = 150;
    private static final long HOVER_MS = 110;

    private final Consumer<String> onPick;
    private final List<StickerSet.Sticker> recent = new ArrayList<>(8);
    private final List<StickerSet.Sticker> all = new ArrayList<>(32);
    private final Tween lift = new Tween(1f);
    private int columns = 8;
    private int cell = 26;
    private int art = 22;
    private int hovered = -1;
    private int focused;
    private int lastHovered = -1;
    private int pressedIndex = -1;
    private long openedAt;
    private boolean reducedMotion;
    private boolean turkish = true;
    private int namedIndex = -2;
    private FormattedCharSequence name;
    private FormattedCharSequence title;

    public StickerPicker(Consumer<String> onPick) {
        super(0, 0, 100, 100, Texts.tr("chat.stickers"));
        this.onPick = onPick;
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    /** Opens with the set and the player's recent stickers (references, the last sent first). */
    public void open(List<String> recentRefs, long now) {
        StickerSet set = StickerSet.get();
        all.clear();
        all.addAll(set.all());
        columns = Math.max(1, set.columns());
        recent.clear();
        for (String ref : recentRefs) {
            StickerSet.Sticker s = set.find(ref);
            if (s != null && recent.size() < columns) {
                recent.add(s);
            }
        }
        turkish = Texts.turkish();
        title = Texts.visual(Texts.str("chat.stickers"));
        hovered = -1;
        lastHovered = -1;
        focused = 0;
        namedIndex = -2;
        openedAt = now;
        visible = true;
    }

    public void close() {
        visible = false;
        pressedIndex = -1;
    }

    public boolean isOpen() {
        return visible;
    }

    /** The picker's height at {@code width}, with or without the recent row. */
    public static int height(int width, boolean withRecent) {
        StickerSet set = StickerSet.get();
        int columns = Math.max(1, set.columns());
        int rows = Math.max(1, (set.all().size() + columns - 1) / columns);
        int cell = cellSize(width, columns);
        return PAD + (withRecent ? cell + SECTION_GAP : 0) + rows * cell + 4 + 9 + PAD;
    }

    /** The art size the picker draws stickers at for this width (prepared before it opens). */
    public static int artSize(int width) {
        int columns = Math.max(1, StickerSet.get().columns());
        return Math.max(16, Math.min(30, cellSize(width, columns) - 4));
    }

    private static int cellSize(int width, int columns) {
        return Math.max(18, (width - 2 * PAD) / columns);
    }

    public boolean hasRecent() {
        return !recent.isEmpty();
    }

    private int count() {
        return recent.size() + all.size();
    }

    private StickerSet.Sticker at(int index) {
        return index < recent.size() ? recent.get(index) : all.get(index - recent.size());
    }

    /** The cell of item {@code index}: column and row, the recent row first. */
    private int columnOf(int index) {
        return index < recent.size() ? index : (index - recent.size()) % columns;
    }

    private int rowOf(int index) {
        return index < recent.size() ? 0 : (recent.isEmpty() ? 0 : 1) + (index - recent.size()) / columns;
    }

    private float cellX(int index) {
        return getX() + PAD + columnOf(index) * cell;
    }

    private float cellY(int index) {
        int row = rowOf(index);
        float y = getY() + PAD + row * cell;
        if (!recent.isEmpty() && row > 0) {
            y += SECTION_GAP;
        }
        return y;
    }

    private int indexAt(double mx, double my) {
        for (int i = 0; i < count(); i++) {
            float x = cellX(i);
            float y = cellY(i);
            if (mx >= x && mx < x + cell && my >= y && my < y + cell) {
                return i;
            }
        }
        return -1;
    }

    private void layoutCells() {
        cell = cellSize(getWidth(), columns);
        art = artSize(getWidth());
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        int i = indexAt(event.x(), event.y());
        if (i >= 0) {
            pick(i);
        }
    }

    private void pick(int index) {
        pressedIndex = index;
        pressed();
        onPick.accept(at(index).ref());
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        int n = count();
        if (n == 0) {
            return false;
        }
        int next = focused;
        if (event.isLeft()) {
            next = Math.max(0, focused - 1);
        } else if (event.isRight()) {
            next = Math.min(n - 1, focused + 1);
        } else if (event.isUp()) {
            next = moveVertically(-1);
        } else if (event.isDown()) {
            next = moveVertically(1);
        } else if (event.isSelection() || event.isConfirmation()) {
            PanelInput.usedKeyboard();
            pick(focused);
            return true;
        } else {
            return false;
        }
        PanelInput.usedKeyboard();
        focused = next;
        return true;
    }

    /** The cell above or below the focused one (the recent row and the grid line up by column). */
    private int moveVertically(int dir) {
        int column = columnOf(focused);
        int row = rowOf(focused);
        int target = row + dir;
        if (target < 0) {
            return focused;
        }
        for (int i = 0; i < count(); i++) {
            if (rowOf(i) == target && columnOf(i) == Math.min(column, lastColumnOf(target))) {
                return i;
            }
        }
        return focused;
    }

    private int lastColumnOf(int row) {
        int last = -1;
        for (int i = 0; i < count(); i++) {
            if (rowOf(i) == row) {
                last = Math.max(last, columnOf(i));
            }
        }
        return last;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        if (all.isEmpty()) {
            return;
        }
        layoutCells();
        hovered = isHovered() ? indexAt(mouseX, mouseY) : -1;
        if (hovered != lastHovered) {
            lift.snap(1f);
            if (hovered >= 0 && !reducedMotion) {
                lift.animateTo(1.08f, now, HOVER_MS, Ease.OUT);
            }
            lastHovered = hovered;
        }
        float t = Math.min(1f, (now - openedAt) / (float) OPEN_MS);
        float rise = reducedMotion ? 0f : (1f - Ease.OUT_EXPO.at(t)) * 4f;
        Matrix3x2fStack m = g.pose();
        m.pushMatrix();
        m.translate(0f, rise);
        Draw.pushAlpha(Ease.OUT.at(t));
        int x0 = getX();
        int y0 = getY();
        int w = getWidth();
        int h = getHeight();
        Draw.frame(g, Frames.POPOVER, x0, y0, w, h);
        if (!recent.isEmpty()) {
            float line = y0 + PAD + cell + SECTION_GAP / 2f;
            Draw.hairline(g, x0 + PAD, x0 + w - PAD, line, Theme.withAlpha(0xFFFFFF, 0.07f));
        }
        int shownIndex = hovered >= 0 ? hovered : keyboardFocus() ? focused : -1;
        for (int i = 0; i < count(); i++) {
            float cx = cellX(i);
            float cy = cellY(i);
            if (i == hovered) {
                Draw.frame(g, Frames.CELL_HOVER, cx, cy, cell, cell);
                g.requestCursor(CursorTypes.POINTING_HAND);
            }
            if (keyboardFocus() && i == focused) {
                Draw.frame(g, Frames.CELL_FOCUS, cx, cy, cell, cell);
            }
            float scale = i == hovered ? lift.value(now) : 1f;
            if (i == pressedIndex) {
                scale *= 1f - 0.08f * press.value(now);
            }
            float ax = cx + (cell - art) / 2f;
            float ay = cy + (cell - art) / 2f;
            if (scale != 1f) {
                m.pushMatrix();
                m.translate(ax + art / 2f, ay + art / 2f);
                m.scale(scale, scale);
                m.translate(-(ax + art / 2f), -(ay + art / 2f));
            }
            if (!StickerArt.draw(g, at(i), art, scale == 1f ? Draw.snap(ax) : ax, scale == 1f ? Draw.snap(ay) : ay)) {
                Draw.frame(g, Frames.STICKER_TILE, ax, ay, art, art);
            }
            if (scale != 1f) {
                m.popMatrix();
            }
        }
        if (shownIndex != namedIndex) {
            namedIndex = shownIndex;
            name = shownIndex < 0 ? null : Texts.visual(at(shownIndex).name(turkish));
        }
        Font small = Fonts.small();
        float nameY = y0 + h - PAD - 9;
        if (name != null) {
            Draw.text(g, small, name, x0 + PAD + 1, Fonts.centeredY(Fonts.Face.SMALL, nameY + 4.5f), Theme.BONE_100);
        } else if (title != null) {
            Draw.text(g, small, title, x0 + PAD + 1, Fonts.centeredY(Fonts.Face.SMALL, nameY + 4.5f), Theme.ASH_400);
        }
        Draw.popAlpha();
        m.popMatrix();
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, Texts.tr("chat.stickers"));
        if (count() > 0) {
            output.add(NarratedElementType.HINT, Texts.tr("chat.stickers.label",
                at(Math.min(focused, count() - 1)).name(turkish)));
        }
    }
}
