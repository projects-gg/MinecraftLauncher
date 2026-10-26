package gg.projects.overlay.chat;

import com.mojang.blaze3d.platform.InputConstants;
import com.mojang.blaze3d.platform.cursor.CursorTypes;
import gg.projects.overlay.emoji.Emoji;
import gg.projects.overlay.emoji.EmojiKind;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.ShapeSet;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;
import org.joml.Matrix3x2fStack;

/**
 * The chat's emoji picker (CHAT-EMOJI): a popover over the chat line, right above its button, in the overlay's surfaces
 * (the sticker picker's {@code POPOVER}). Groups top to bottom: what the player wrote last, symbols, the server's own
 * pictures, expressions and, on Projects, the codes the server fills in (a row each, with what they write). Every emoji
 * shows as chat will show it ({@link Previews}). The line at the bottom names the one under the pointer or picked with
 * the arrows: its code, the rank the server asks for (gold) and the servers it works on; on other servers its name.
 *
 * <p>It never takes the chat line's focus: letters still go to the line (and close the picker, the player is writing
 * again), so Enter is the picker's only after the arrows picked something. A press on the button opens it with a 150 ms
 * fade and a 4 px rise (a fade only with reduced motion); Ctrl+E, used many times a day, opens it at once. Nothing about
 * it runs while it is closed. Sizes are GUI pixels, so it is as legible at GUI 1 as at 4, scrolling when the screen is
 * short.
 */
final class EmojiPicker {
    static final int PAD = 5;
    static final int TILE = 16;
    static final int GAP = 2;
    static final int CHIP_PAD = 4;
    static final int TITLE_H = 11;
    static final int SECTION_GAP = 4;
    static final int SHORTCUT_H = 14;
    static final int FOOTER_H = 14;
    static final int MAX_W = 180;
    static final int MAX_CONTENT_H = 150;
    static final long OPEN_MS = 150;
    /** How long "the line is full" stays in the name line. */
    static final long NOTE_MS = 2_200;

    /** The groups, in the picker's order. */
    enum Section {
        RECENT("chat_emoji.recent"),
        SYMBOLS("chat_emoji.symbols"),
        PACK("chat_emoji.pack"),
        TEXT("chat_emoji.text"),
        SHORTCUTS("chat_emoji.shortcuts");

        final String key;

        Section(String key) {
            this.key = key;
        }
    }

    /** What a key did. */
    enum KeyResult {
        /** Not the picker's: the chat line has it. */
        PASS,
        /** The picker took it (moved, scrolled). */
        TAKEN,
        /** Enter on the picked emoji: write it. */
        PICK,
        /** Escape: close the picker only. */
        CLOSE
    }

    /** One emoji laid out: its box in the content (before scrolling), its preview and its width. */
    record Item(Emoji emoji, Section section, float x, float y, int w, int h, Component preview, int previewWidth,
        boolean picture) {
    }

    private record Title(FormattedCharSequence text, float y) {
    }

    private final Spring scroll = Spring.snappy(0f);
    private final List<Item> items = new ArrayList<>();
    private final List<Title> titles = new ArrayList<>();
    private boolean open;
    private long openedAt;
    private long lastFrame;
    /** The server's list (codes, ranks, servers in the name line) or the picker's unicode set (names). */
    private boolean projects;
    private float contentHeight;
    private int x0;
    private int y0;
    private int width;
    private int height;
    private int hovered = -1;
    private int focused = -1;
    private boolean keyboard;
    private int shortcutCodeWidth;
    private FormattedCharSequence note;
    private long noteUntil;

    boolean isOpen() {
        return open;
    }

    /** Whether the arrows picked an emoji since it opened (Enter writes it). */
    boolean keyboardPicked() {
        return open && keyboard && focused >= 0;
    }

    /**
     * Opens with {@code list} (in the picker's order) and the recently written ones that are in it; the arrows' pick starts
     * on the first emoji when it opened from the keyboard.
     */
    void open(boolean projects, List<Emoji> list, List<String> recent, boolean byKeyboard, long now) {
        this.projects = projects;
        build(list, recent);
        open = true;
        // A shortcut used many times a day opens at once; the pointer's press gets the short rise.
        openedAt = byKeyboard ? now - OPEN_MS : now;
        lastFrame = 0;
        scroll.snap(0f);
        hovered = -1;
        keyboard = byKeyboard;
        focused = byKeyboard && !items.isEmpty() ? 0 : -1;
        note = null;
    }

    void close() {
        open = false;
        hovered = -1;
    }

    /** The list changed while it is open (the launcher's new list, the pack loading): laid out again, scrolled to top. */
    void refresh(boolean projects, List<Emoji> list, List<String> recent) {
        if (!open) {
            return;
        }
        this.projects = projects;
        build(list, recent);
        scroll.snap(0f);
        focused = keyboard && !items.isEmpty() ? 0 : -1;
    }

    /** Says that the chat line has no room for the pick, in the name line for a moment. */
    void lineFull(long now) {
        note = Texts.visual(Texts.str("chat_emoji.full"));
        noteUntil = now + NOTE_MS;
    }

    // ---- Layout -------------------------------------------------------------------------------------------

    /** Lays the groups out for the box's content width (sections without emojis are left out). */
    private void build(List<Emoji> list, List<String> recent) {
        items.clear();
        titles.clear();
        Minecraft mc = Minecraft.getInstance();
        Font font = mc.font;
        int contentWidth = Math.max(TILE, width - 2 * PAD);
        List<List<Emoji>> sections = new ArrayList<>();
        for (int i = 0; i < Section.values().length; i++) {
            sections.add(new ArrayList<>());
        }
        for (String code : recent) {
            for (Emoji e : list) {
                if (e.code().equals(code) && e.kind() != EmojiKind.SHORTCUT) {
                    sections.get(Section.RECENT.ordinal()).add(e);
                    break;
                }
            }
        }
        for (Emoji e : list) {
            Section s = switch (e.kind()) {
                case TEXT -> Section.TEXT;
                case PACK -> Section.PACK;
                case SHORTCUT -> Section.SHORTCUTS;
                default -> Section.SYMBOLS;
            };
            sections.get(s.ordinal()).add(e);
        }
        int shown = 0;
        for (List<Emoji> s : sections) {
            shown += s.isEmpty() ? 0 : 1;
        }
        shortcutCodeWidth = 0;
        for (Emoji e : sections.get(Section.SHORTCUTS.ordinal())) {
            shortcutCodeWidth = Math.max(shortcutCodeWidth, font.width(e.code()));
        }
        Font small = Fonts.small();
        float y = 0;
        for (Section section : Section.values()) {
            List<Emoji> entries = sections.get(section.ordinal());
            if (section == Section.RECENT) {
                entries = firstRow(entries, contentWidth, font);
            }
            if (entries.isEmpty()) {
                continue;
            }
            if (!items.isEmpty() || !titles.isEmpty()) {
                y += SECTION_GAP;
            }
            // A title only where there is more than one group to tell apart.
            if (shown > 1) {
                titles.add(new Title(Texts.visual(Texts.fit(small, Texts.str(section.key), contentWidth)), y));
                y += TITLE_H;
            }
            float x = 0;
            float rowTop = y;
            for (Emoji e : entries) {
                if (section == Section.SHORTCUTS) {
                    items.add(new Item(e, section, 0, rowTop, contentWidth, SHORTCUT_H, Component.literal(e.code()),
                        font.width(e.code()), false));
                    rowTop += SHORTCUT_H;
                    continue;
                }
                Component preview = Previews.of(e);
                boolean picture = Previews.picture(e);
                int previewWidth = font.width(preview);
                int w = cellWidth(e, previewWidth, picture, contentWidth);
                if (x > 0 && x + w > contentWidth) {
                    x = 0;
                    rowTop += TILE + GAP;
                }
                items.add(new Item(e, section, x, rowTop, w, TILE, preview, previewWidth, picture));
                x += w + GAP;
            }
            y = section == Section.SHORTCUTS ? rowTop : rowTop + TILE;
        }
        contentHeight = y;
    }

    /** A symbol (or the pack's picture) in a square cell, an expression as wide as its words (one row at most). */
    private static int cellWidth(Emoji e, int previewWidth, boolean picture, int contentWidth) {
        boolean square = picture || e.kind() == EmojiKind.SYMBOL || previewWidth <= TILE - 4;
        int w = square ? Math.max(TILE, previewWidth + 4) : previewWidth + 2 * CHIP_PAD;
        return Math.min(contentWidth, Math.max(TILE, w));
    }

    /** The recent ones that fit one row. */
    private static List<Emoji> firstRow(List<Emoji> recent, int contentWidth, Font font) {
        List<Emoji> row = new ArrayList<>();
        float x = 0;
        for (Emoji e : recent) {
            Component preview = Previews.of(e);
            int w = cellWidth(e, font.width(preview), Previews.picture(e), contentWidth);
            if (x > 0 && x + w > contentWidth) {
                break;
            }
            row.add(e);
            x += w + GAP;
        }
        return row;
    }

    /**
     * Places the box: its right edge at {@code right}, its bottom at {@code bottom}, as wide as the screen allows and
     * as tall as its groups (scrolling past {@link #MAX_CONTENT_H} or what is left above).
     */
    void place(int right, int bottom, int screenWidth, List<Emoji> list, List<String> recent) {
        int w = Math.max(TILE * 4 + 2 * PAD, Math.min(MAX_W, screenWidth - 8));
        if (w != width) {
            width = w;
            if (open) {
                build(list, recent);
            }
        }
        int available = Math.max(TILE * 2, bottom - 4);
        int content = (int) Math.ceil(Math.min(contentHeight, MAX_CONTENT_H));
        height = Math.min(available, PAD + content + 4 + FOOTER_H);
        x0 = Math.max(4, right - width);
        y0 = bottom - height;
        if (scroll.value() > maxScroll()) {
            scroll.snap(maxScroll());
        }
    }

    boolean contains(double mx, double my) {
        return open && mx >= x0 && mx < x0 + width && my >= y0 && my < y0 + height;
    }

    private float gridTop() {
        return y0 + PAD;
    }

    private float gridBottom() {
        return y0 + height - FOOTER_H - 4;
    }

    private float maxScroll() {
        return Math.max(0f, contentHeight - (gridBottom() - gridTop()));
    }

    private float itemX(Item item) {
        return x0 + PAD + item.x();
    }

    private float itemY(Item item) {
        return gridTop() + item.y() - scroll.value();
    }

    private int itemAt(double mx, double my) {
        if (!contains(mx, my) || my < gridTop() || my >= gridBottom()) {
            return -1;
        }
        for (int i = 0; i < items.size(); i++) {
            Item item = items.get(i);
            float x = itemX(item);
            float y = itemY(item);
            if (mx >= x && mx < x + item.w() && my >= y && my < y + item.h()) {
                return i;
            }
        }
        return -1;
    }

    /** Where the emoji with this code is on screen now (x, y, w, h), or null (the scenario driver points at it). */
    float[] boxOf(String code) {
        for (Item item : items) {
            if (item.emoji().code().equals(code)) {
                return new float[] {itemX(item), itemY(item), item.w(), item.h()};
            }
        }
        return null;
    }

    /** The emoji under the pointer at {@code mx, my}, or null. */
    Emoji emojiAt(double mx, double my) {
        int i = itemAt(mx, my);
        return i < 0 ? null : items.get(i).emoji();
    }

    /** The emoji the arrows picked, or null. */
    Emoji focusedEmoji() {
        return focused >= 0 && focused < items.size() ? items.get(focused).emoji() : null;
    }

    /** The wheel over the picker scrolls it (and nothing under it). */
    boolean scrolled(double mx, double my, double amount) {
        if (!contains(mx, my)) {
            return false;
        }
        if (maxScroll() > 0f) {
            scroll.setTarget(Math.max(0f, Math.min(maxScroll(), scroll.target() - (float) amount * (TILE + GAP))));
        }
        return true;
    }

    /** A key while it is open: the arrows, Home and End, Page Up and Down move the pick; Enter writes it; Escape closes. */
    KeyResult key(KeyEvent event) {
        if (event.isEscape()) {
            return KeyResult.CLOSE;
        }
        int n = items.size();
        if (event.isConfirmation()) {
            return keyboardPicked() ? KeyResult.PICK : KeyResult.PASS;
        }
        int next;
        if (event.isLeft()) {
            next = focused < 0 ? 0 : Math.max(0, focused - 1);
        } else if (event.isRight()) {
            next = focused < 0 ? 0 : Math.min(n - 1, focused + 1);
        } else if (event.isUp()) {
            next = focused < 0 ? 0 : vertical(-1);
        } else if (event.isDown()) {
            next = focused < 0 ? 0 : vertical(1);
        } else if (event.key() == InputConstants.KEY_HOME) {
            next = 0;
        } else if (event.key() == InputConstants.KEY_END) {
            next = n - 1;
        } else if (event.key() == InputConstants.KEY_PAGEUP || event.key() == InputConstants.KEY_PAGEDOWN) {
            next = focused < 0 ? 0 : page(event.key() == InputConstants.KEY_PAGEUP ? -1 : 1);
        } else {
            return KeyResult.PASS;
        }
        if (n == 0) {
            return KeyResult.TAKEN;
        }
        keyboard = true;
        focused = Math.max(0, Math.min(n - 1, next));
        reveal(items.get(focused));
        return KeyResult.TAKEN;
    }

    /** The emoji in the row above or below whose middle is nearest the picked one's. */
    private int vertical(int dir) {
        Item from = items.get(focused);
        float middle = from.x() + from.w() / 2f;
        float rowY = from.y();
        float target = Float.NaN;
        for (int i = dir > 0 ? focused + 1 : focused - 1; i >= 0 && i < items.size(); i += dir) {
            float y = items.get(i).y();
            if (y != rowY) {
                target = y;
                break;
            }
        }
        if (Float.isNaN(target)) {
            return focused;
        }
        int best = focused;
        float bestDistance = Float.MAX_VALUE;
        for (int i = 0; i < items.size(); i++) {
            Item item = items.get(i);
            if (item.y() != target) {
                continue;
            }
            float d = Math.abs(item.x() + item.w() / 2f - middle);
            if (d < bestDistance) {
                bestDistance = d;
                best = i;
            }
        }
        return best;
    }

    /** About a view's height up or down. */
    private int page(int dir) {
        float view = gridBottom() - gridTop();
        float wanted = items.get(focused).y() + dir * view;
        int best = focused;
        float bestDistance = Float.MAX_VALUE;
        for (int i = 0; i < items.size(); i++) {
            float d = Math.abs(items.get(i).y() - wanted);
            if (d < bestDistance) {
                bestDistance = d;
                best = i;
            }
        }
        return best;
    }

    /** Scrolls the picked one into view (at once: the keys repeat). */
    private void reveal(Item item) {
        float view = gridBottom() - gridTop();
        float target = scroll.target();
        // Its group's title comes along when the pick is the group's first row.
        float top = item.y();
        for (Title title : titles) {
            if (title.y() <= item.y() && item.y() - title.y() <= TITLE_H + 0.5f) {
                top = title.y();
            }
        }
        if (top < target) {
            target = top;
        } else if (item.y() + item.h() > target + view) {
            target = item.y() + item.h() - view;
        }
        scroll.snap(Math.max(0f, Math.min(maxScroll(), target)));
    }

    // ---- Drawing ------------------------------------------------------------------------------------------

    void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now, boolean reducedMotion, boolean turkish) {
        if (!open) {
            return;
        }
        float dt = lastFrame == 0 ? 0f : Math.min(0.05f, (now - lastFrame) / 1000f);
        lastFrame = now;
        if (reducedMotion) {
            scroll.snap(scroll.target());
        } else {
            scroll.step(dt);
        }
        if (scroll.value() > maxScroll()) {
            scroll.snap(maxScroll());
        }
        hovered = itemAt(mouseX, mouseY);
        if (hovered >= 0 && keyboard && mouseMovedSinceKeys(mouseX, mouseY)) {
            keyboard = false;
        }
        float t = Math.min(1f, (now - openedAt) / (float) OPEN_MS);
        float rise = reducedMotion ? 0f : (1f - Ease.OUT_EXPO.at(t)) * 4f;
        Matrix3x2fStack m = g.pose();
        m.pushMatrix();
        m.translate(0f, rise);
        Draw.pushAlpha(Ease.OUT.at(t));
        Draw.frame(g, Frames.POPOVER, x0, y0, width, height);
        float top = gridTop();
        float bottom = gridBottom();
        boolean scrolls = maxScroll() > 0f;
        g.enableScissor(x0, Math.round(top) - 1, x0 + width, Math.round(bottom) + 1);
        Font small = Fonts.small();
        for (Title title : titles) {
            float ty = top + title.y() - scroll.value();
            if (ty + TITLE_H < top - 1 || ty > bottom + 1) {
                continue;
            }
            Draw.text(g, small, title.text(), x0 + PAD + 1, Fonts.centeredY(Fonts.Face.SMALL, ty + 4.5f), Theme.ASH_400);
        }
        Font font = Minecraft.getInstance().font;
        int shownIndex = hovered >= 0 ? hovered : keyboard ? focused : -1;
        for (int i = 0; i < items.size(); i++) {
            Item item = items.get(i);
            float x = itemX(item);
            float y = itemY(item);
            if (y + item.h() < top - 1 || y > bottom + 1) {
                continue;
            }
            if (i == hovered) {
                Draw.frame(g, Frames.CELL_HOVER, x, y, item.w(), item.h());
                g.requestCursor(CursorTypes.POINTING_HAND);
            }
            if (keyboard && i == focused) {
                Draw.frame(g, Frames.CELL_FOCUS, x, y, item.w(), item.h());
            }
            if (item.section() == Section.SHORTCUTS) {
                drawShortcut(g, font, small, item, x, y, turkish);
                continue;
            }
            float px = Draw.snap(x + (item.w() - item.previewWidth()) / 2f);
            float py = Draw.snap(y + (item.picture() ? 5f : 4f));
            Draw.text(g, font, item.preview().getVisualOrderText(), px, py, Theme.BONE_100);
        }
        g.disableScissor();
        if (scrolls) {
            float above = Math.min(1f, scroll.value() / ShapeSet.LIST_FADE);
            float below = Math.min(1f, (maxScroll() - scroll.value()) / ShapeSet.LIST_FADE);
            if (above > 0.01f) {
                Draw.pushAlpha(above);
                Draw.column(g, ShapeSet.POPOVER_FADE_TOP, x0 + 1, top - 1, width - 2, ShapeSet.LIST_FADE);
                Draw.popAlpha();
            }
            if (below > 0.01f) {
                Draw.pushAlpha(below);
                Draw.column(g, ShapeSet.POPOVER_FADE_BOTTOM, x0 + 1, bottom + 1 - ShapeSet.LIST_FADE, width - 2,
                    ShapeSet.LIST_FADE);
                Draw.popAlpha();
            }
        }
        drawNameLine(g, small, shownIndex, now, turkish);
        Draw.popAlpha();
        m.popMatrix();
    }

    private int lastMouseX = Integer.MIN_VALUE;
    private int lastMouseY = Integer.MIN_VALUE;

    /** The pointer moved since the keys took over: its emoji is the one named again. */
    private boolean mouseMovedSinceKeys(int mouseX, int mouseY) {
        boolean moved = lastMouseX != Integer.MIN_VALUE && (mouseX != lastMouseX || mouseY != lastMouseY);
        lastMouseX = mouseX;
        lastMouseY = mouseY;
        return moved;
    }

    /** A shortcut's row: its code as it goes into chat, then what the server writes for it. */
    private void drawShortcut(GuiGraphicsExtractor g, Font font, Font small, Item item, float x, float y,
        boolean turkish) {
        float codeY = Draw.snap(y + 3f);
        Draw.text(g, font, item.preview().getVisualOrderText(), Draw.snap(x + 3), codeY, Theme.BONE_100);
        String meaning = item.emoji().name(turkish);
        if (meaning == null) {
            return;
        }
        float mx = x + 3 + shortcutCodeWidth + 8;
        int room = Math.round(x + item.w() - 3 - mx);
        if (room > 12) {
            Draw.text(g, small, Texts.visual(Texts.fit(small, meaning, room)), mx,
                Fonts.centeredY(Fonts.Face.SMALL, y + item.h() / 2f), Theme.ASH_300);
        }
    }

    /**
     * The name line: the emoji under the pointer (or picked): its code (or name), the servers it works on and, in gold at
     * the right, the rank it needs; "the line is full" for a moment after a pick that did not fit; the picker's name else.
     */
    private void drawNameLine(GuiGraphicsExtractor g, Font small, int shownIndex, long now, boolean turkish) {
        float lineY = y0 + height - FOOTER_H - 2;
        Draw.hairline(g, x0 + PAD, x0 + width - PAD, lineY, Theme.withAlpha(0xFFFFFF, 0.07f));
        float textY = Fonts.centeredY(Fonts.Face.SMALL, lineY + 1 + FOOTER_H / 2f);
        float left = x0 + PAD + 1;
        float right = x0 + width - PAD - 1;
        if (note != null && now < noteUntil) {
            Draw.text(g, small, note, left, textY, Theme.FLARE_300);
            return;
        }
        if (shownIndex < 0 || shownIndex >= items.size()) {
            Draw.text(g, small, Texts.visual(Texts.str("chat_emoji.title")), left, textY, Theme.ASH_400);
            return;
        }
        Emoji e = items.get(shownIndex).emoji();
        if (!projects) {
            String name = e.name(turkish);
            Draw.text(g, small, Texts.visual(Texts.fit(small, name == null ? e.code() : name, Math.round(right - left))),
                left, textY, Theme.BONE_100);
            return;
        }
        float rankWidth = 0;
        if (e.rank() != null) {
            rankWidth = small.width(e.rank());
            Draw.text(g, small, Texts.visual(e.rank()), right - rankWidth, textY, Theme.GOLD_300);
            rankWidth += 6;
        }
        int room = Math.round(right - rankWidth - left);
        String code = Texts.fit(small, e.code(), room);
        Draw.text(g, small, Texts.visual(code), left, textY, Theme.BONE_100);
        if (!e.servers().isEmpty()) {
            float sx = left + small.width(code) + 6;
            int rest = Math.round(right - rankWidth - sx);
            if (rest > 16) {
                String where = Texts.str("chat_emoji.only", String.join(", ", e.servers()));
                Draw.text(g, small, Texts.visual(Texts.fit(small, where, rest)), sx, textY, Theme.ASH_400);
            }
        }
    }
}
