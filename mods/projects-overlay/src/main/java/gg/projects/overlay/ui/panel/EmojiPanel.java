package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.InputConstants;
import com.mojang.blaze3d.platform.cursor.CursorTypes;
import gg.projects.overlay.emoji.EmojiCatalog;
import gg.projects.overlay.emoji.EmojiFinder;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.ShapeSet;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.IntPredicate;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.util.FormattedCharSequence;
import org.joml.Matrix3x2fStack;

/**
 * The chat picker's emoji tab (OVL-SOC; the owner: "çıkartmalar var da emoji yok"; the launcher's picker panel,
 * {@code features/emoji/picker-panel.tsx}): over the message field, under the picker's tabs, "Emoji ara" (a search in
 * the overlay's language that finds English too, {@link EmojiFinder}), then "Son kullanılanlar" and the launcher's
 * groups in its order, a grid of the emoji the overlay draws as pictures (at chat size: the faces' 8-pixel pictures, one
 * device pixel each at every GUI scale), the group rail under it (it follows the scroll; a press jumps), and the name of
 * the emoji under the pointer or picked with the arrows at the bottom. A pick goes into the message where the caret is and
 * the picker stays open (emoji come in runs). The recent row is the one of the moment it opened, so a pick never shifts
 * the cells under the pointer.
 *
 * <p>Typed letters while it has the keys go to the search; the arrows pick, Enter writes the picked one, ↑ from the top
 * row reaches the tabs. It opens with the sticker picker's 150 ms rise (a fade only with reduced motion); nothing about
 * it runs while it is closed.
 */
public final class EmojiPanel extends PanelWidget {
    private static final int PAD = 5;
    private static final int SEARCH_H = 14;
    static final int CELL = 16;
    private static final int TITLE_H = 11;
    private static final int RAIL_H = 14;
    private static final int RAIL_ICON = 10;
    private static final int NAME_H = 9;
    private static final long OPEN_MS = 150;
    private static final int MAX_QUERY = 32;
    /** Recent emoji kept at the top (the launcher's 24: three rows of its eight). */
    public static final int RECENT = 24;

    /** What a pick does. */
    public interface Actions {
        /** Write {@code emoji} into the message; {@code keys}: Enter picked it (the keys stay here). */
        void pick(String emoji, boolean keys);

        /** The sticker tab was asked for ({@code keys}: by the arrows on the tabs, which keep the keys there). */
        void tab(int tab, boolean keys);
    }

    /** A section of the grid: its title ("Son kullanılanlar", a group's name; none for search results). */
    private record Section(String key, Icon icon, String title, List<EmojiCatalog.Entry> entries) {
    }

    private final Actions actions;
    private final List<Section> sections = new ArrayList<>();
    private final List<EmojiCatalog.Entry> cells = new ArrayList<>();
    /** Each cell's row and column, and each row's top in the grid's own content; sections' title tops. */
    private int[] cellRow = new int[0];
    private int[] cellCol = new int[0];
    private final List<Float> rowTop = new ArrayList<>();
    private final List<Float> titleTop = new ArrayList<>();
    /** Prepared lines kept while it is open: the cells' emoji by entry, the sections' titles, the search's hint. */
    private FormattedCharSequence[] glyphs = new FormattedCharSequence[0];
    private final Map<String, FormattedCharSequence> titles = new HashMap<>();
    private FormattedCharSequence hint;
    private int hintWidth = -1;
    /** When the search was last typed into (its caret stays lit while typing, as the message field's). */
    private long typedAt;
    private final Spring scroll = Spring.snappy(0f);
    private EmojiCatalog catalog;
    private IntPredicate shows = cp -> true;
    private List<String> recent = List.of();
    private String query = "";
    private boolean tabs;
    /** The keys are on the tabs (↑ from the grid's first row). */
    private boolean onTabs;
    private int focused;
    private int hovered = -1;
    private int columns = 10;
    private float contentHeight;
    private int laidOutWidth = -1;
    private long openedAt;
    private long lastFrame;
    private boolean reducedMotion;
    private boolean turkish = true;
    private int namedIndex = -2;
    private FormattedCharSequence name;

    public EmojiPanel(Actions actions) {
        super(0, 0, 100, 100, Texts.tr("chat.emoji"));
        this.actions = actions;
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    /**
     * Opens with {@code catalog} ({@code shows}: whether the font of the moment draws a code point; at GUI scale 1 the
     * game's own font does) and the player's recent emoji, the last first. {@code tabs}: the sticker tab is there;
     * {@code swap}: it comes in place of the sticker tab (a fade, no rise).
     */
    public void open(EmojiCatalog catalog, IntPredicate shows, List<String> recentEmoji, boolean tabs, boolean swap,
        boolean keysOnTabs, long now) {
        this.catalog = catalog;
        this.shows = shows;
        this.recent = List.copyOf(recentEmoji);
        this.tabs = tabs;
        turkish = Texts.turkish();
        query = "";
        onTabs = tabs && keysOnTabs;
        glyphs = new FormattedCharSequence[catalog.entries().size()];
        focused = 0;
        hovered = -1;
        namedIndex = -2;
        openedAt = swap ? now - OPEN_MS / 2 : now;
        scroll.snap(0f);
        lastFrame = 0;
        laidOutWidth = -1;
        visible = true;
        build();
    }

    public void close() {
        visible = false;
        query = "";
        // The cells' prepared lines go with it (the catalog itself stays for the game, EmojiCatalog).
        glyphs = new FormattedCharSequence[0];
        titles.clear();
        hint = null;
        hintWidth = -1;
        cells.clear();
        sections.clear();
        laidOutWidth = -1;
    }

    public boolean isOpen() {
        return visible;
    }

    /** The sticker tab came or went while it is open (the service took or stopped taking stickers). */
    public void setTabs(boolean value) {
        if (tabs != value) {
            tabs = value;
            onTabs = false;
        }
    }

    public boolean hasTabs() {
        return tabs;
    }

    /** How far it has faded in (what it covers steps aside as much). */
    public float opacity(long now) {
        return visible ? Ease.OUT.at(Math.min(1f, (now - openedAt) / (float) OPEN_MS)) : 0f;
    }

    /** Its height at {@code width}: the tabs, the search, about five rows of the grid, the rail and the name. */
    public static int height(int width, boolean tabs) {
        return PAD + (tabs ? PickerTabs.HEIGHT + PickerTabs.BELOW : 0) + SEARCH_H + 4 + TITLE_H + 5 * CELL + 3
            + RAIL_H + 2 + NAME_H + PAD;
    }

    /** The emoji the picked cell writes (the scenario driver), or null. */
    public String focusedEmoji() {
        layoutGrid();
        return focused >= 0 && focused < cells.size() ? cells.get(focused).emoji() : null;
    }

    /** Types {@code text} into the search as keys would (the scenario driver). */
    public void search(String text) {
        query = text.length() > MAX_QUERY ? text.substring(0, MAX_QUERY) : text;
        rebuild();
    }

    /** The cells of the grid now, in order (the scenario driver and the narration). */
    public int count() {
        layoutGrid();
        return cells.size();
    }

    /** Writes the picked cell's emoji as Enter does (the scenario driver); false with nothing picked. */
    public boolean pickFocused() {
        layoutGrid();
        if (focused < 0 || focused >= cells.size()) {
            return false;
        }
        actions.pick(cells.get(focused).emoji(), true);
        return true;
    }

    // ---- Content ------------------------------------------------------------------------------------

    private void build() {
        sections.clear();
        if (catalog == null) {
            return;
        }
        if (!query.isBlank()) {
            List<EmojiCatalog.Entry> found = new ArrayList<>();
            for (EmojiCatalog.Entry e : EmojiFinder.search(catalog, query)) {
                if (shown(e)) {
                    found.add(e);
                }
            }
            sections.add(new Section("found", null, null, found));
            return;
        }
        List<EmojiCatalog.Entry> recents = new ArrayList<>();
        for (String r : recent) {
            EmojiCatalog.Entry e = catalog.find(r);
            if (e != null && shown(e) && !recents.contains(e) && recents.size() < RECENT) {
                recents.add(e);
            }
        }
        if (!recents.isEmpty()) {
            sections.add(new Section("recent", Icon.RECENT, Texts.str("chat.emoji.recent"), recents));
        }
        for (EmojiCatalog.Group group : catalog.groups()) {
            List<EmojiCatalog.Entry> of = new ArrayList<>();
            for (EmojiCatalog.Entry e : catalog.entries()) {
                if (e.group() == group && shown(e)) {
                    of.add(e);
                }
            }
            if (!of.isEmpty()) {
                sections.add(new Section(group.id, iconOf(group), Texts.str("chat.emoji.group." + group.id), of));
            }
        }
    }

    private boolean shown(EmojiCatalog.Entry e) {
        return shows.test(e.emoji().codePointAt(0));
    }

    private static Icon iconOf(EmojiCatalog.Group group) {
        return switch (group) {
            case SMILEYS -> Icon.SMILEY;
            case PEOPLE -> Icon.WAVE;
            case NATURE -> Icon.PAW;
            case FOOD -> Icon.FOOD;
            case TRAVEL -> Icon.TRAVEL;
            case ACTIVITIES -> Icon.BALL;
            case OBJECTS -> Icon.BULB;
            case SYMBOLS -> Icon.SHAPES;
            case FLAGS -> Icon.REPORT;
        };
    }

    private void rebuild() {
        build();
        laidOutWidth = -1;
        focused = 0;
        namedIndex = -2;
        scroll.snap(0f);
    }

    /** The grid's cells and rows for the width (made again only when the width or the content changes). */
    private void layoutGrid() {
        if (laidOutWidth == getWidth()) {
            return;
        }
        laidOutWidth = getWidth();
        columns = Math.max(4, (getWidth() - 2 * PAD) / CELL);
        cells.clear();
        rowTop.clear();
        titleTop.clear();
        List<Integer> rows = new ArrayList<>();
        List<Integer> cols = new ArrayList<>();
        float y = 0f;
        int row = 0;
        for (Section s : sections) {
            titleTop.add(y);
            if (s.title != null) {
                y += TITLE_H;
            }
            for (int i = 0; i < s.entries.size(); i++) {
                if (i % columns == 0) {
                    rowTop.add(y);
                    y += CELL;
                    row = rowTop.size() - 1;
                }
                cells.add(s.entries.get(i));
                rows.add(row);
                cols.add(i % columns);
            }
            y += 3;
        }
        contentHeight = Math.max(0f, y - 3);
        cellRow = rows.stream().mapToInt(Integer::intValue).toArray();
        cellCol = cols.stream().mapToInt(Integer::intValue).toArray();
        focused = Math.max(0, Math.min(focused, cells.size() - 1));
    }

    // ---- Geometry -----------------------------------------------------------------------------------

    private float tabsTop() {
        return getY() + PAD;
    }

    private float searchTop() {
        return tabsTop() + (tabs ? PickerTabs.HEIGHT + PickerTabs.BELOW : 0);
    }

    private float gridTop() {
        return searchTop() + SEARCH_H + 4;
    }

    private float nameTop() {
        return getY() + getHeight() - PAD - NAME_H;
    }

    private float railTop() {
        return nameTop() - 2 - RAIL_H;
    }

    private float gridBottom() {
        return railTop() - 3;
    }

    private float maxScroll() {
        return Math.max(0f, contentHeight - (gridBottom() - gridTop()));
    }

    /** The grid's left edge: the columns centred in the picker, on a whole pixel. */
    private float gridLeft() {
        return getX() + PAD + (getWidth() - 2 * PAD - columns * CELL) / 2;
    }

    private float cellX(int index) {
        return gridLeft() + cellCol[index] * CELL;
    }

    private float cellY(int index) {
        return gridTop() + rowTop.get(cellRow[index]) - scroll.value();
    }

    private int cellAt(double mx, double my) {
        if (my < gridTop() || my >= gridBottom() || mx < gridLeft()) {
            return -1;
        }
        int col = (int) ((mx - gridLeft()) / CELL);
        if (col >= columns) {
            return -1;
        }
        float local = (float) my - gridTop() + scroll.value();
        for (int r = 0; r < rowTop.size(); r++) {
            float top = rowTop.get(r);
            if (local >= top && local < top + CELL) {
                for (int i = 0; i < cells.size(); i++) {
                    if (cellRow[i] == r && cellCol[i] == col) {
                        return i;
                    }
                }
                return -1;
            }
        }
        return -1;
    }

    /** The section the top of the view is in (the rail lights it). */
    private int sectionInView() {
        float top = scroll.value() + 1f;
        int at = 0;
        for (int i = 0; i < titleTop.size(); i++) {
            if (titleTop.get(i) <= top) {
                at = i;
            }
        }
        return at;
    }

    /** The rail's icons: one a titled section, as wide as the rail allows. */
    private float railStep() {
        int n = Math.max(1, railCount());
        return Math.min(18f, (getWidth() - 2f * PAD) / n);
    }

    private int railCount() {
        int n = 0;
        for (Section s : sections) {
            if (s.icon != null) {
                n++;
            }
        }
        return n;
    }

    private int railAt(double mx, double my) {
        if (my < railTop() || my >= railTop() + RAIL_H || !query.isBlank()) {
            return -1;
        }
        int slot = (int) ((mx - getX() - PAD) / railStep());
        int n = 0;
        for (int i = 0; i < sections.size(); i++) {
            if (sections.get(i).icon == null) {
                continue;
            }
            if (n == slot) {
                return i;
            }
            n++;
        }
        return -1;
    }

    private void reveal(int index) {
        float top = rowTop.get(cellRow[index]);
        float view = gridBottom() - gridTop();
        float target = scroll.target();
        if (top < target) {
            target = top;
        } else if (top + CELL > target + view) {
            target = top + CELL - view;
        }
        scroll.snap(Math.max(0f, Math.min(maxScroll(), target)));
    }

    private void jumpTo(int section) {
        float target = Math.max(0f, Math.min(maxScroll(), titleTop.get(section)));
        if (reducedMotion) {
            scroll.snap(target);
        } else {
            scroll.setTarget(target);
        }
        // The keys follow: the section's first cell.
        int at = 0;
        for (int i = 0; i < section; i++) {
            at += sections.get(i).entries.size();
        }
        focused = Math.min(at, Math.max(0, cells.size() - 1));
    }

    // ---- Input --------------------------------------------------------------------------------------

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        layoutGrid();
        if (tabs) {
            int tab = PickerTabs.at(event.x(), event.y(), getX() + PAD, tabsTop(), getWidth() - 2 * PAD);
            if (tab == PickerTabs.STICKERS) {
                actions.tab(tab, false);
                return;
            }
            if (tab >= 0) {
                return;
            }
        }
        int rail = railAt(event.x(), event.y());
        if (rail >= 0) {
            jumpTo(rail);
            return;
        }
        int i = cellAt(event.x(), event.y());
        if (i >= 0) {
            focused = i;
            pressed();
            actions.pick(cells.get(i).emoji(), false);
        }
    }

    @Override
    public boolean charTyped(CharacterEvent event) {
        if (!visible) {
            return false;
        }
        int cp = event.codepoint();
        if (Character.isISOControl(cp) || query.codePointCount(0, query.length()) >= MAX_QUERY
            || query.isEmpty() && Character.isWhitespace(cp)) {
            return true;
        }
        PanelInput.usedKeyboard();
        onTabs = false;
        query += new String(Character.toChars(cp));
        typedAt = now();
        rebuild();
        return true;
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (!visible) {
            // Closed (a press elsewhere, the layout) while it still had the keys: it does nothing.
            return false;
        }
        layoutGrid();
        if (event.key() == InputConstants.KEY_BACKSPACE) {
            PanelInput.usedKeyboard();
            if (!query.isEmpty()) {
                int cut = query.offsetByCodePoints(query.length(), -1);
                query = query.substring(0, cut);
                typedAt = now();
                rebuild();
            }
            return true;
        }
        if (onTabs) {
            if (event.isRight()) {
                PanelInput.usedKeyboard();
                actions.tab(PickerTabs.STICKERS, true);
                return true;
            }
            if (event.isDown()) {
                PanelInput.usedKeyboard();
                onTabs = false;
                return true;
            }
            return event.isLeft() || event.isUp();
        }
        int n = cells.size();
        if (n == 0) {
            if (event.isUp() && tabs) {
                onTabs = true;
                return true;
            }
            return false;
        }
        int next = focused;
        if (event.isLeft()) {
            next = Math.max(0, focused - 1);
        } else if (event.isRight()) {
            next = Math.min(n - 1, focused + 1);
        } else if (event.isUp()) {
            if (cellRow[focused] == 0) {
                if (tabs) {
                    PanelInput.usedKeyboard();
                    onTabs = true;
                }
                return true;
            }
            next = inRow(cellRow[focused] - 1, cellCol[focused]);
        } else if (event.isDown()) {
            next = inRow(Math.min(rowTop.size() - 1, cellRow[focused] + 1), cellCol[focused]);
        } else if (event.isConfirmation()) {
            PanelInput.usedKeyboard();
            pressed();
            actions.pick(cells.get(focused).emoji(), true);
            return true;
        } else {
            return false;
        }
        PanelInput.usedKeyboard();
        focused = next;
        reveal(focused);
        return true;
    }

    /** The cell in {@code row} at {@code column}, or the row's last when it is shorter. */
    private int inRow(int row, int column) {
        int best = focused;
        for (int i = 0; i < cells.size(); i++) {
            if (cellRow[i] == row) {
                best = i;
                if (cellCol[i] >= column) {
                    return i;
                }
            }
        }
        return best;
    }

    @Override
    public boolean mouseScrolled(double mouseX, double mouseY, double scrollX, double scrollY) {
        if (!isMouseOver(mouseX, mouseY) || maxScroll() <= 0f) {
            return false;
        }
        float target = Math.max(0f, Math.min(maxScroll(), scroll.target() - (float) scrollY * CELL * 2));
        if (reducedMotion) {
            scroll.snap(target);
        } else {
            scroll.setTarget(target);
        }
        return true;
    }

    // ---- Drawing ------------------------------------------------------------------------------------

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        layoutGrid();
        float dt = lastFrame == 0 ? 0f : Math.min(0.05f, (now - lastFrame) / 1000f);
        lastFrame = now;
        scroll.step(dt);
        if (scroll.value() > maxScroll()) {
            scroll.snap(maxScroll());
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
        Draw.frame(g, Frames.POPOVER, x0, y0, w, getHeight());
        if (tabs) {
            PickerTabs.draw(g, x0 + PAD, tabsTop(), w - 2 * PAD, PickerTabs.EMOJI, isHovered() ? mouseX : -1,
                mouseY, onTabs && keyboardFocus());
        }
        drawSearch(g, x0, w, now);
        hovered = isHovered() ? cellAt(mouseX, mouseY) : -1;
        float top = gridTop();
        float bottom = gridBottom();
        g.enableScissor(x0, Math.round(top) - 1, x0 + w, Math.round(bottom) + 1);
        Font small = Fonts.small();
        Font ui = Fonts.ui();
        for (int s = 0; s < sections.size(); s++) {
            Section section = sections.get(s);
            if (section.title == null) {
                continue;
            }
            float ty = top + titleTop.get(s) - scroll.value();
            if (ty + TITLE_H >= top - 1 && ty <= bottom + 1) {
                Draw.text(g, small, titles.computeIfAbsent(section.key, k -> Texts.visual(section.title)), gridLeft() + 1,
                    Fonts.centeredY(Fonts.Face.SMALL, ty + TITLE_H / 2f), Theme.ASH_400);
            }
        }
        for (int i = 0; i < cells.size(); i++) {
            float cy = cellY(i);
            if (cy + CELL < top - 1 || cy > bottom + 1) {
                continue;
            }
            float cx = cellX(i);
            if (i == hovered) {
                Draw.frame(g, Frames.CELL_HOVER, cx, cy, CELL, CELL);
                g.requestCursor(CursorTypes.POINTING_HAND);
            }
            if (keyboardFocus() && !onTabs && i == focused) {
                Draw.frame(g, Frames.CELL_FOCUS, cx, cy, CELL, CELL);
            }
            EmojiCatalog.Entry e = cells.get(i);
            FormattedCharSequence glyph = glyphOf(e);
            // The picture's own colours (white is no tint), on whole pixels: crisp at every GUI scale.
            float gw = ui.width(glyph);
            Draw.text(g, ui, glyph, Draw.snap(cx + (CELL - gw) / 2f), Fonts.centeredY(Fonts.Face.UI, cy + CELL / 2f),
                Theme.WHITE);
        }
        if (cells.isEmpty() && !query.isBlank()) {
            String none = Texts.str("chat.emoji.none");
            Draw.text(g, small, none, x0 + (w - small.width(none)) / 2f,
                Fonts.centeredY(Fonts.Face.SMALL, top + Math.min(24f, (bottom - top) / 2f)), Theme.ASH_400);
        }
        g.disableScissor();
        float above = Math.min(1f, scroll.value() / ShapeSet.LIST_FADE);
        float below = Math.min(1f, (maxScroll() - scroll.value()) / ShapeSet.LIST_FADE);
        if (above > 0.01f) {
            Draw.pushAlpha(above);
            Draw.column(g, ShapeSet.POPOVER_FADE_TOP, x0 + 1, top - 1, w - 2, ShapeSet.LIST_FADE);
            Draw.popAlpha();
        }
        if (below > 0.01f) {
            Draw.pushAlpha(below);
            Draw.column(g, ShapeSet.POPOVER_FADE_BOTTOM, x0 + 1, bottom + 1 - ShapeSet.LIST_FADE, w - 2,
                ShapeSet.LIST_FADE);
            Draw.popAlpha();
        }
        drawRail(g, x0, mouseX, mouseY);
        drawName(g, x0);
        Draw.popAlpha();
        m.popMatrix();
    }

    private void drawSearch(GuiGraphicsExtractor g, int x0, int w, long now) {
        float sy = searchTop();
        // A lighter edge while it has the keys: the gold of focus belongs to the picked cell (one focus mark).
        Draw.frame(g, isFocused() ? Frames.FIELD_HOVER : Frames.FIELD, x0 + PAD, sy, w - 2 * PAD, SEARCH_H);
        Draw.icon(g, Icon.SEARCH, 8, x0 + PAD + 4, sy + (SEARCH_H - 8) / 2f, Theme.ASH_400);
        Font ui = Fonts.ui();
        float tx = x0 + PAD + 4 + 8 + 4;
        float ty = Fonts.centeredY(Fonts.Face.UI, sy + SEARCH_H / 2f);
        int room = Math.max(10, w - 2 * PAD - 4 - 8 - 4 - 6);
        if (query.isEmpty()) {
            if (hint == null || hintWidth != room) {
                hint = Texts.visual(Texts.fit(ui, Texts.str("chat.emoji.search"), room));
                hintWidth = room;
            }
            Draw.text(g, ui, hint, tx, ty, Theme.ASH_400);
        } else {
            String shown = query;
            while (ui.width(shown) > room && shown.length() > 1) {
                shown = shown.substring(shown.offsetByCodePoints(0, 1));
            }
            Draw.text(g, ui, shown, tx, ty, Theme.BONE_50);
            tx += ui.width(shown);
        }
        if (isFocused()) {
            // The search's caret, as the message field's: lit while typing, a slow blink when still.
            int k = Draw.scale();
            float cw = Math.max(1, k / 2) / (float) k;
            if (now - typedAt < 530 || (now / 530) % 2 == 0) {
                Draw.fill(g, tx + (query.isEmpty() ? -1 : 0), sy + 3, tx + cw + (query.isEmpty() ? -1 : 0),
                    sy + SEARCH_H - 3, Theme.GOLD_300);
            }
        }
    }

    private void drawRail(GuiGraphicsExtractor g, int x0, int mouseX, int mouseY) {
        if (!query.isBlank() || sections.isEmpty()) {
            return;
        }
        float ry = railTop();
        Draw.hairline(g, x0 + PAD, x0 + getWidth() - PAD, ry - 1, Theme.withAlpha(0xFFFFFF, 0.07f));
        float step = railStep();
        int inView = sectionInView();
        int lit = isHovered() ? railAt(mouseX, mouseY) : -1;
        int n = 0;
        for (int i = 0; i < sections.size(); i++) {
            Section s = sections.get(i);
            if (s.icon == null) {
                continue;
            }
            float ix = x0 + PAD + n * step + (step - RAIL_ICON) / 2f;
            float iy = ry + (RAIL_H - RAIL_ICON) / 2f;
            int ink = i == inView ? Theme.BONE_50 : i == lit ? Theme.BONE_100 : Theme.ASH_400;
            if (i == lit) {
                g.requestCursor(CursorTypes.POINTING_HAND);
            }
            Draw.icon(g, s.icon, RAIL_ICON, Draw.snap(ix), Draw.snap(iy), ink);
            n++;
        }
    }

    private void drawName(GuiGraphicsExtractor g, int x0) {
        int shownIndex = hovered >= 0 ? hovered : keyboardFocus() && !onTabs ? focused : -1;
        if (shownIndex >= cells.size()) {
            shownIndex = -1;
        }
        if (shownIndex != namedIndex) {
            namedIndex = shownIndex;
            Font small = Fonts.small();
            name = shownIndex < 0 ? null : Texts.visual(Texts.fit(small,
                EmojiCatalog.displayName(cells.get(shownIndex).name(), turkish), getWidth() - 2 * PAD));
        }
        float ny = nameTop();
        if (name != null) {
            Draw.text(g, Fonts.small(), name, x0 + PAD + 1, Fonts.centeredY(Fonts.Face.SMALL, ny + NAME_H / 2f),
                Theme.BONE_100);
        }
    }

    /** An emoji's prepared line, made the first time its cell shows and kept while the picker is open. */
    private FormattedCharSequence glyphOf(EmojiCatalog.Entry e) {
        int i = e.index();
        if (i < 0 || i >= glyphs.length) {
            return Texts.visual(e.emoji());
        }
        FormattedCharSequence made = glyphs[i];
        if (made == null) {
            made = Texts.visual(e.emoji());
            glyphs[i] = made;
        }
        return made;
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, Texts.tr("chat.emoji"));
        if (focused >= 0 && focused < cells.size()) {
            output.add(NarratedElementType.HINT, Texts.tr("chat.emoji.label",
                EmojiCatalog.displayName(cells.get(focused).name(), turkish)));
        } else if (!query.isBlank()) {
            output.add(NarratedElementType.HINT, Texts.tr("chat.emoji.none"));
        }
    }
}
