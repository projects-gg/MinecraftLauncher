package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.TagChip;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.ShapeSet;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.function.Predicate;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.input.MouseButtonInfo;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * Friends, the ones in a game first, then those in the launcher, then the rest quietly: a face with the presence dot
 * (filled jade in a game, a jade ring in the launcher, none offline), the name and where they are, "Katıl" when the
 * player can go to them, and a "…" at the row's end.
 *
 * <p>The row opens the friend's conversation (a click, Enter; OVL-UX, the owner: "sol tık mesajı açsın"); where there is
 * none to open (chat off) it opens the friend's menu instead, as the launcher's row opens the friend's card. Their menu
 * (the launcher's person menu, FIX2-D) opens at the pointer for a right click, and under the "…" for its click, Shift+F10
 * or the menu key. The "…" shows on the row under the pointer, the row picked with the keys and the row whose menu is
 * open, as on the launcher's rows; its place is kept on every row, so nothing moves when it comes.
 *
 * <p>A friend who wears a crew tag has its chip before the name ("[GK] Ayşe", TEAM6): the name gives way, the chip
 * never does. The pointer resting on a chip, or a row with one picked with the keys, opens the tag's bubble. How a menu
 * action went is said for a moment at the list's bottom.
 */
public final class FriendList extends PanelWidget {
    private static final int ROW_H = 24;
    private static final int HEADER_H = 16;
    private static final int FACE = 16;
    private static final int JOIN_H = 14;
    /** The "…": its square, and the gap before it. */
    private static final int DOTS = 14;
    private static final int DOTS_GAP = 3;
    private static final int NOTE_H = 14;
    private static final long DOTS_IN_MS = 120;
    private static final long DOTS_OUT_MS = 80;
    /** The name line's middle in a row. */
    private static final float NAME_MID = 8f;

    /** What a row does. */
    public interface Actions {
        /** "Katıl" on a friend. */
        void join(String friendId);

        /** The row itself (a click, Enter): their conversation, or their menu under {@code dots} when none opens. */
        void primary(String friendId, PersonMenu.Anchor dots, boolean keys);

        /** Their menu: at the pointer (a right click) or under the "…" ({@code anchor}). */
        void menu(String friendId, PersonMenu.Anchor anchor, boolean keys);
    }

    private static final class Row {
        String id;
        boolean header;
        FormattedCharSequence title;
        FormattedCharSequence name;
        FormattedCharSequence where;
        boolean online;
        boolean inGame;
        boolean joinable;
        String initial;
        FormattedCharSequence initialLine;
        float initialWidth;
        float y;
        float joinWidth;
        Component narration;
        /** Their conversation can be opened (the row's own press opens it). */
        boolean message;
        /** TEAM6: the tag they wear, or null. */
        TagChip chip;
    }

    private final Actions actions;
    private final BadgeBubble bubble = new BadgeBubble();
    private final List<Row> rows = new ArrayList<>();
    private FormattedCharSequence joinLine = FormattedCharSequence.EMPTY;
    private FormattedCharSequence emptyLine = FormattedCharSequence.EMPTY;
    private float emptyWidth;
    private int badgeRevision = -1;
    private final Spring scroll = Spring.snappy(0f);
    private int revision = -1;
    private int laidOutWidth = -1;
    private float contentHeight;
    private String selected;
    private boolean reducedMotion;
    private long lastFrame;
    private String joinLabel = "";
    private String emptyText = "";
    // The "…": on which row it shows, the one it is leaving, and the row whose menu is open.
    private String dotsRow;
    private String dotsLeaving;
    private final Tween dotsIn = new Tween(0f);
    private final Tween dotsOut = new Tween(0f);
    private String menuFor;
    // The menu came from the "…" (it looks pressed then); from a right click it only stays on its row.
    private boolean menuFromDots;
    // The note at the bottom (how a menu action went).
    private OverlayState.PersonNote note;
    private FormattedCharSequence noteText;
    private float noteWidth;
    private boolean noteError;
    private boolean noteComing;
    private final Tween noteAlpha = new Tween(0f);

    public FriendList(Actions actions) {
        super(0, 0, 100, 100, Texts.tr("friends"));
        this.actions = actions;
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
        bubble.setReducedMotion(value);
    }

    /** Closes the tag's bubble (the panel shows another page). */
    public void dismissBubble() {
        bubble.dismiss(now());
    }

    /** Esc: closes the tag's bubble if one is open; whether it was. */
    public boolean closeBubble() {
        return bubble.close(now());
    }

    /** The menu closed. */
    public void setMenuFor(String friendId) {
        setMenuFor(friendId, false);
    }

    /**
     * The friend whose menu is open (their row stays lit and keeps its "…"), or null; {@code fromDots}: it opened under
     * the "…" (a click on it, or the keys), which then looks pressed.
     */
    public void setMenuFor(String friendId, boolean fromDots) {
        menuFromDots = friendId != null && fromDots;
        if (!Objects.equals(menuFor, friendId)) {
            menuFor = friendId;
            if (friendId != null) {
                bubble.dismiss(now());
            }
        }
    }

    /**
     * Lays the list out again when the friends changed.
     *
     * @param canJoin whether "Katıl" makes sense for a friend now (not already on their server, connected)
     */
    public void refresh(OverlayState state, Predicate<OverlayState.Friend> canJoin) {
        if (state.revision() == revision && getWidth() == laidOutWidth && state.badgeRevision() == badgeRevision) {
            return;
        }
        revision = state.revision();
        badgeRevision = state.badgeRevision();
        laidOutWidth = getWidth();
        Font small = Fonts.small();
        joinLabel = Texts.str("friend.join");
        joinLine = Texts.visual(joinLabel);
        emptyText = Texts.str(state.signedIn() ? "friends.empty" : "friends.signed_out");
        emptyLine = Texts.visual(emptyText);
        emptyWidth = small.width(emptyText);
        float joinWidth = small.width(joinLabel) + 14;
        rows.clear();
        List<OverlayState.Friend> game = new ArrayList<>();
        List<OverlayState.Friend> launcher = new ArrayList<>();
        List<OverlayState.Friend> offline = new ArrayList<>();
        for (OverlayState.Friend f : state.friends()) {
            if (f.inGame()) {
                game.add(f);
            } else if (f.online()) {
                launcher.add(f);
            } else {
                offline.add(f);
            }
        }
        float y = 0f;
        y = section(state, y, "friends.in_game", game, canJoin, joinWidth);
        y = section(state, y, "friends.online", launcher, canJoin, joinWidth);
        y = section(state, y, "friends.offline", offline, canJoin, joinWidth);
        contentHeight = y;
        if (selected != null && rows.stream().noneMatch(r -> !r.header && r.id.equals(selected))) {
            selected = null;
        }
        scroll.snap(Math.max(0f, Math.min(maxScroll(), scroll.target())));
    }

    private float section(OverlayState state, float y, String titleKey, List<OverlayState.Friend> friends,
        Predicate<OverlayState.Friend> canJoin, float joinWidth) {
        if (friends.isEmpty()) {
            return y;
        }
        Row header = new Row();
        header.header = true;
        header.title = Texts.visual(Texts.str(titleKey, friends.size()));
        header.y = y;
        rows.add(header);
        y += HEADER_H;
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        // The "…" keeps its place on every row (OVL-UX): names and places end before it and "Katıl".
        int nameRoom = getWidth() - FACE - 8 - (int) joinWidth - 6 - DOTS - DOTS_GAP;
        for (OverlayState.Friend f : friends) {
            Row row = new Row();
            row.id = f.id();
            OverlayState.Badge badge = state.badge(f.badge());
            row.chip = TagChip.of(badge, TagChip.ROW);
            String name = Texts.fit(strong, f.name().isEmpty() ? Texts.str("toast.someone") : f.name(),
                Math.round(nameRoom - TagChip.room(row.chip)));
            row.name = Texts.visual(name);
            row.online = f.online();
            row.inGame = f.inGame();
            row.joinable = f.joinable() && canJoin.test(f);
            row.message = state.dm(f.id()) != null;
            String where = Texts.str("friend.where." + whereKey(f.where()));
            if (f.where().equals("server") && f.host() != null) {
                where = Texts.str("friend.where.server_at", f.host());
            }
            row.where = Texts.visual(Texts.fit(small, where, nameRoom));
            row.initial = Texts.initial(f.name());
            row.initialLine = Texts.visual(row.initial);
            row.initialWidth = strong.width(row.initial);
            row.joinWidth = joinWidth;
            row.y = y;
            // "Ayşe, etiket GK, Projects'te": the chip is drawn, the row says it.
            row.narration = Texts.tr("narration.friend", badge == null ? name : Texts.str("row.badge", name,
                badge.tag()), where);
            rows.add(row);
            y += ROW_H;
        }
        return y + 4;
    }

    private static String whereKey(String where) {
        return switch (where) {
            case "projects", "launcher", "single", "server", "game", "offline" -> where;
            default -> "game";
        };
    }

    private float maxScroll() {
        return Math.max(0f, contentHeight - getHeight());
    }

    private int selectedIndex() {
        for (int i = 0; i < rows.size(); i++) {
            if (!rows.get(i).header && rows.get(i).id.equals(selected)) {
                return i;
            }
        }
        return -1;
    }

    private void move(int direction) {
        int i = selectedIndex();
        int next = i;
        do {
            next += direction;
        } while (next >= 0 && next < rows.size() && rows.get(next).header);
        if (next < 0 || next >= rows.size()) {
            if (i < 0) {
                for (Row r : rows) {
                    if (!r.header) {
                        selected = r.id;
                        reveal(r);
                        return;
                    }
                }
            }
            return;
        }
        selected = rows.get(next).id;
        reveal(rows.get(next));
    }

    private void reveal(Row row) {
        float target = scroll.target();
        if (row.y < target) {
            target = row.y - HEADER_H;
        } else if (row.y + ROW_H > target + getHeight()) {
            target = row.y + ROW_H - getHeight();
        }
        target = Math.max(0f, Math.min(maxScroll(), target));
        if (reducedMotion) {
            scroll.snap(target);
        } else {
            scroll.setTarget(target);
        }
    }

    /** The "…" of a row on screen (scrolled), as a menu's anchor. */
    private PersonMenu.Anchor dotsAnchor(Row row) {
        float y = getY() + row.y - scroll.value();
        return new PersonMenu.Anchor(dotsX(), y + (ROW_H - DOTS) / 2f, DOTS, DOTS, false);
    }

    private float dotsX() {
        return getX() + getWidth() - DOTS;
    }

    private float joinX(Row row) {
        return dotsX() - DOTS_GAP - row.joinWidth;
    }

    /** The "…" of {@code friendId}'s row as a menu's anchor (the scenario driver opens it as a click would), or null. */
    public PersonMenu.Anchor dotsOf(String friendId) {
        for (Row row : rows) {
            if (!row.header && row.id.equals(friendId)) {
                return dotsAnchor(row);
            }
        }
        return null;
    }

    /** The middle of a friend's row on screen (the scenario driver points at it), or null. */
    public float[] rowCenter(String friendId) {
        for (Row row : rows) {
            if (!row.header && row.id.equals(friendId)) {
                float y = getY() + row.y - scroll.value();
                return new float[] {getX() + FACE + 8 + 30, y + ROW_H / 2f};
            }
        }
        return null;
    }

    /** Puts the keyboard's pick on a friend's row (the scenario driver uses it). */
    public void select(String friendId) {
        for (Row row : rows) {
            if (!row.header && row.id.equals(friendId)) {
                selected = row.id;
                reveal(row);
                return;
            }
        }
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (event.isUp()) {
            PanelInput.usedKeyboard();
            move(-1);
            return true;
        }
        if (event.isDown()) {
            PanelInput.usedKeyboard();
            move(1);
            return true;
        }
        int i = selectedIndex();
        Row row = i < 0 ? null : rows.get(i);
        if (row != null && (event.key() == Compat.KEY_MENU || (event.key() == InputConstants.KEY_F10
            && event.hasShiftDown()))) {
            // Shift+F10 or the menu key: the row's menu, its first row lit.
            PanelInput.usedKeyboard();
            actions.menu(row.id, dotsAnchor(row), true);
            return true;
        }
        if (row != null && (event.isSelection() || event.isConfirmation())) {
            PanelInput.usedKeyboard();
            actions.primary(row.id, dotsAnchor(row), true);
            return true;
        }
        if (event.key() == InputConstants.KEY_HOME || event.key() == InputConstants.KEY_END) {
            PanelInput.usedKeyboard();
            selected = null;
            move(event.key() == InputConstants.KEY_HOME ? 1 : -1);
            return true;
        }
        return false;
    }

    @Override
    public boolean mouseScrolled(double mouseX, double mouseY, double scrollX, double scrollY) {
        if (!isMouseOver(mouseX, mouseY) || maxScroll() <= 0f) {
            return false;
        }
        float target = Math.max(0f, Math.min(maxScroll(), scroll.target() - (float) scrollY * 24f));
        if (reducedMotion) {
            scroll.snap(target);
        } else {
            scroll.setTarget(target);
        }
        return true;
    }

    /** A right click opens a row's menu too. */
    @Override
    protected boolean isValidClickButton(MouseButtonInfo info) {
        return info.button() == InputConstants.MOUSE_BUTTON_LEFT || info.button() == InputConstants.MOUSE_BUTTON_RIGHT;
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        float cy = (float) event.y() - getY() + scroll.value();
        for (Row row : rows) {
            if (row.header || cy < row.y || cy >= row.y + ROW_H) {
                continue;
            }
            selected = row.id;
            if (event.button() == InputConstants.MOUSE_BUTTON_RIGHT) {
                actions.menu(row.id, PersonMenu.Anchor.at(event.x(), event.y()), false);
            } else if (event.x() >= dotsX() - 1) {
                actions.menu(row.id, dotsAnchor(row), false);
            } else if (row.joinable && event.x() >= joinX(row)) {
                actions.join(row.id);
            } else if (!doubleClick) {
                actions.primary(row.id, dotsAnchor(row), false);
            }
            return;
        }
    }

    /** How a friend menu action went, said under the list for a moment (null: nothing to say). */
    public void setNote(OverlayState.PersonNote next, long now) {
        if (next == note) {
            return;
        }
        note = next;
        if (next != null) {
            String words = next.text() != null ? next.text()
                : next.name().isEmpty() && next.key().equals("invite_queued") ? Texts.str("people.note.invited")
                : Texts.str("people.note." + next.key(), next.name());
            Font small = Fonts.small();
            String fitted = Texts.fit(small, words, Math.max(40, getWidth() - 12));
            noteText = Texts.visual(fitted);
            noteWidth = small.width(fitted) + 12;
            noteError = next.error();
            noteComing = noteAlpha.value(now) < 0.5f;
            noteAlpha.animateTo(1f, now, 160, Ease.OUT);
        } else {
            noteComing = false;
            noteAlpha.animateTo(0f, now, 110, Ease.OUT);
        }
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float dt = lastFrame == 0 ? 0f : Math.min(0.05f, (now - lastFrame) / 1000f);
        lastFrame = now;
        scroll.step(dt);
        int x0 = getX();
        int y0 = getY();
        int w = getWidth();
        int h = getHeight();
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        if (rows.isEmpty()) {
            Draw.text(g, small, emptyLine, x0 + (w - emptyWidth) / 2f, Fonts.centeredY(Fonts.Face.SMALL, y0 + h / 2f),
                Theme.ASH_400);
            bubble.track(null, false, now);
            drawNote(g, now);
            return;
        }
        float top = scroll.value();
        trackBubble(mouseX, mouseY, top, now);
        trackDots(mouseX, mouseY, top, now);
        float dotsX = dotsX();
        g.enableScissor(x0 - 2, y0, x0 + w + 2, y0 + h);
        for (Row row : rows) {
            float y = y0 + row.y - top;
            if (y > y0 + h || y + ROW_H < y0) {
                continue;
            }
            if (row.header) {
                Draw.text(g, small, row.title, x0 + 1, Fonts.centeredY(Fonts.Face.SMALL, y + HEADER_H / 2f + 1),
                    Theme.ASH_400);
                continue;
            }
            boolean lit = isHovered() && mouseY >= y && mouseY < y + ROW_H && menuFor == null;
            boolean chosen = row.id.equals(selected) && keyboardFocus();
            boolean open = row.id.equals(menuFor);
            if (lit || chosen || open) {
                Draw.frame(g, Frames.ROW_HOVER, x0 - 2, y + 1, w + 4, ROW_H - 2, 0x99FFFFFF);
            }
            if (chosen) {
                Draw.frame(g, Frames.FOCUS_ROW, x0 - 2, y + 1, w + 4, ROW_H - 2);
            }
            float cx = x0 + FACE / 2f + 1;
            float cy = y + ROW_H / 2f;
            Draw.pushAlpha(row.online ? 1f : 0.55f);
            Draw.disc(g, FACE, cx, cy, Theme.OBSIDIAN_600);
            Draw.text(g, strong, row.initialLine, cx - row.initialWidth / 2f, Fonts.centeredY(Fonts.Face.STRONG, cy),
                Theme.BONE_100);
            Draw.popAlpha();
            if (row.online) {
                // Filled jade in a game, a jade ring in the launcher (docs/design.md, Friends).
                Draw.disc(g, 8, cx + 6, cy + 6, Theme.OBSIDIAN_900);
                Draw.disc(g, 5, cx + 6, cy + 6, Theme.JADE_400);
                if (!row.inGame) {
                    Draw.disc(g, 3, cx + 6, cy + 6, Theme.OBSIDIAN_900);
                }
            }
            float tx = x0 + FACE + 8;
            if (row.chip != null) {
                // An offline friend's chip rests quieter, as their name does.
                Draw.pushAlpha(row.online ? 1f : 0.7f);
                row.chip.draw(g, tx, y + NAME_MID);
                Draw.popAlpha();
            }
            Draw.text(g, strong, row.name, tx + TagChip.room(row.chip), Fonts.centeredY(Fonts.Face.STRONG, y + NAME_MID),
                row.online ? Theme.BONE_100 : Theme.ASH_400);
            Draw.text(g, small, row.where, tx, Fonts.centeredY(Fonts.Face.SMALL, y + 17), row.inGame
                ? Theme.ASH_300 : Theme.ASH_400);
            if (row.joinable) {
                float jx = joinX(row);
                float jy = y + (ROW_H - JOIN_H) / 2f;
                boolean joinLit = lit && mouseX >= jx && mouseX < jx + row.joinWidth;
                Draw.frame(g, joinLit ? Frames.CHIP_SELECTED : Frames.CHIP, jx, jy, row.joinWidth, JOIN_H);
                Draw.text(g, small, joinLine, jx + 7, Fonts.centeredY(Fonts.Face.SMALL, jy + JOIN_H / 2f),
                    joinLit ? Theme.GOLD_200 : Theme.BONE_100);
            }
            float dots = row.id.equals(dotsRow) ? dotsIn.value(now) : row.id.equals(dotsLeaving) ? dotsOut.value(now)
                : 0f;
            if (dots > 0.01f) {
                float dy = y + (ROW_H - DOTS) / 2f;
                boolean pressed = open && menuFromDots;
                boolean dotsLit = pressed || (lit && mouseX >= dotsX - 1);
                Draw.pushAlpha(dots);
                if (dotsLit) {
                    Draw.frame(g, pressed ? Frames.CHIP_SELECTED : Frames.CHIP_HOVER, dotsX, dy, DOTS, DOTS);
                }
                Draw.icon(g, Icon.DOTS, 10, dotsX + (DOTS - 10) / 2f, dy + (DOTS - 10) / 2f, dotsLit ? Theme.BONE_100
                    : Theme.ASH_300);
                Draw.popAlpha();
            }
        }
        // The edges fade into the panel while there is more beyond them (the strength follows the scroll): no row is
        // cut through its letters at the list's bottom.
        float above = Math.min(1f, Math.max(0f, top / ShapeSet.LIST_FADE));
        float below = Math.min(1f, Math.max(0f, (maxScroll() - top) / ShapeSet.LIST_FADE));
        if (above > 0.01f) {
            Draw.pushAlpha(above);
            Draw.column(g, ShapeSet.FADE_TOP, x0 - 2, y0, w + 4, ShapeSet.LIST_FADE);
            Draw.popAlpha();
        }
        if (below > 0.01f) {
            Draw.pushAlpha(below);
            Draw.column(g, ShapeSet.FADE_BOTTOM, x0 - 2, y0 + h - ShapeSet.LIST_FADE, w + 4, ShapeSet.LIST_FADE);
            Draw.popAlpha();
        }
        g.disableScissor();
        drawNote(g, now);
    }

    /** The "…" comes on the row under the pointer, the one picked with the keys and the one whose menu is open. */
    private void trackDots(int mouseX, int mouseY, float top, long now) {
        String want = menuFor;
        if (want == null && isHovered()) {
            for (Row row : rows) {
                float y = getY() + row.y - top;
                if (!row.header && mouseY >= y && mouseY < y + ROW_H) {
                    want = row.id;
                    break;
                }
            }
        }
        if (want == null && keyboardFocus()) {
            want = selected;
        }
        if (Objects.equals(want, dotsRow)) {
            return;
        }
        if (dotsRow != null) {
            dotsLeaving = dotsRow;
            dotsOut.snap(dotsIn.value(now));
            dotsOut.animateTo(0f, now, reducedMotion ? 0 : DOTS_OUT_MS, Ease.OUT);
        }
        dotsRow = want;
        if (want != null) {
            if (want.equals(dotsLeaving)) {
                dotsLeaving = null;
            }
            dotsIn.snap(0f);
            dotsIn.animateTo(1f, now, DOTS_IN_MS, Ease.OUT);
        }
    }

    /** How a menu action went: a plate over the list's bottom for a moment (it floats; nothing moves). */
    private void drawNote(GuiGraphicsExtractor g, long now) {
        float a = noteAlpha.value(now);
        if (a <= 0.01f || noteText == null) {
            return;
        }
        float rise = noteComing && !reducedMotion ? (1f - a) * 3f : 0f;
        float x = getX();
        float y = getY() + getHeight() - NOTE_H - 2 + rise;
        Draw.pushAlpha(a);
        Draw.frame(g, Frames.POPOVER, x, y, noteWidth, NOTE_H);
        Draw.text(g, Fonts.small(), noteText, x + 6, Fonts.centeredY(Fonts.Face.SMALL, y + NOTE_H / 2f),
            noteError ? Theme.FLARE_300 : Theme.BONE_100);
        Draw.popAlpha();
    }

    /**
     * The chip under the pointer opens its bubble; with none there, the row picked with the keys does (TEAM6 §6.3).
     * Only chips whose row is in view count, and none while a friend's menu is open (one popover at a time).
     */
    private void trackBubble(int mouseX, int mouseY, float top, long now) {
        BadgeBubble.Target target = null;
        float tx = getX() + FACE + 8;
        if (isHovered() && menuFor == null) {
            for (Row row : rows) {
                if (row.header || row.chip == null) {
                    continue;
                }
                float y = getY() + row.y - top;
                float cy = row.chip.top(y + NAME_MID);
                if (mouseX >= tx && mouseX < tx + row.chip.width && mouseY >= cy && mouseY < cy + row.chip.height
                    && cy >= getY() && cy + row.chip.height <= getY() + getHeight()) {
                    target = new BadgeBubble.Target(row.id, row.chip.badge, tx, cy, row.chip.width, row.chip.height,
                        y, y + ROW_H, false);
                    break;
                }
            }
        }
        if (target == null && keyboardFocus() && menuFor == null) {
            int i = selectedIndex();
            Row row = i < 0 ? null : rows.get(i);
            if (row != null && row.chip != null) {
                float y = getY() + row.y - top;
                if (y >= getY() - 2 && y + ROW_H <= getY() + getHeight() + 2) {
                    float cy = row.chip.top(y + NAME_MID);
                    target = new BadgeBubble.Target(row.id, row.chip.badge, tx, cy, row.chip.width, row.chip.height,
                        y, y + ROW_H, true);
                }
            }
        }
        bubble.track(target, bubble.contains(mouseX, mouseY), now);
    }

    /** The middle of a friend's tag chip on screen, or null (the scenario driver points at it). */
    public float[] chipCenter(String friendId) {
        for (Row row : rows) {
            if (!row.header && row.chip != null && row.id.equals(friendId)) {
                float y = getY() + row.y - scroll.value();
                return new float[] {getX() + FACE + 8 + row.chip.width / 2f, y + NAME_MID};
            }
        }
        return null;
    }

    /** Draws the tag's bubble beside the panel (or inside {@code left..right} and {@code top..bottom}). */
    public void drawBubble(GuiGraphicsExtractor g, long now, float panelLeft, float left, float top, float right,
        float bottom) {
        if (visible) {
            bubble.draw(g, now, panelLeft, left, top, right, bottom);
        }
    }

    /** Whether {@code (mx, my)} is on the tag's bubble (a press there stays in the panel). */
    public boolean bubbleContains(double mx, double my) {
        return visible && bubble.contains(mx, my);
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, getMessage());
        int i = selectedIndex();
        if (i >= 0) {
            output.add(NarratedElementType.HINT, rows.get(i).narration);
            output.add(NarratedElementType.USAGE, Texts.tr(rows.get(i).message ? "narration.friend.usage"
                : "narration.friend.usage.menu"));
        }
    }
}
