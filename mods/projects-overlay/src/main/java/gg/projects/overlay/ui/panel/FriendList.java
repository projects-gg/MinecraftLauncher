package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.TagChip;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.ShapeSet;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;
import java.util.function.Predicate;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * Friends, the ones in a game first, then those in the launcher, then the rest quietly: a face with the
 * presence dot (filled jade in a game, a jade ring in the launcher, none offline), the name and where
 * they are, and "Katıl" when the player can go to them. Arrow keys move through the rows, Enter joins.
 *
 * <p>A friend who wears a crew tag has its chip before the name ("[GK] Ayşe", TEAM6): the name gives way, the chip
 * never does. The pointer resting on a chip, or a row with one picked with the keys, opens the tag's bubble.
 */
public final class FriendList extends PanelWidget {
    private static final int ROW_H = 24;
    private static final int HEADER_H = 16;
    private static final int FACE = 16;
    private static final int JOIN_H = 14;
    /** The name line's middle in a row. */
    private static final float NAME_MID = 8f;

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
        /** TEAM6: the tag they wear, or null. */
        TagChip chip;
    }

    private final Consumer<String> onJoin;
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

    public FriendList(Consumer<String> onJoin) {
        super(0, 0, 100, 100, Texts.tr("friends"));
        this.onJoin = onJoin;
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
        int nameRoom = getWidth() - FACE - 8 - (int) joinWidth - 6;
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
            String where = Texts.str("friend.where." + whereKey(f.where()));
            if (f.where().equals("server") && f.host() != null) {
                where = Texts.str("friend.where.server_at", f.host());
            }
            OverlayState.WorldMark world = state.world(f.id());
            if (world != null) {
                // They invited the player to their open world (H13): "Dünyasında · 2/8" (the host counts).
                where = Texts.str("friend.where.world", (world.guests() + 1) + "/" + (world.max() + 1));
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
        if (event.isSelection() || event.isConfirmation()) {
            int i = selectedIndex();
            if (i >= 0 && rows.get(i).joinable) {
                PanelInput.usedKeyboard();
                onJoin.accept(rows.get(i).id);
                return true;
            }
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

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        float cy = (float) event.y() - getY() + scroll.value();
        for (Row row : rows) {
            if (row.header || cy < row.y || cy >= row.y + ROW_H) {
                continue;
            }
            selected = row.id;
            float joinX = getX() + getWidth() - row.joinWidth;
            if (row.joinable && event.x() >= joinX) {
                onJoin.accept(row.id);
            }
            return;
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
            return;
        }
        float top = scroll.value();
        trackBubble(mouseX, mouseY, top, now);
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
            boolean lit = isHovered() && mouseY >= y && mouseY < y + ROW_H;
            boolean chosen = row.id.equals(selected) && keyboardFocus();
            if (lit || chosen) {
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
                float jx = x0 + w - row.joinWidth;
                float jy = y + (ROW_H - JOIN_H) / 2f;
                boolean joinLit = lit && mouseX >= jx;
                Draw.frame(g, joinLit ? Frames.CHIP_SELECTED : Frames.CHIP, jx, jy, row.joinWidth, JOIN_H);
                Draw.text(g, small, joinLine, jx + 7, Fonts.centeredY(Fonts.Face.SMALL, jy + JOIN_H / 2f),
                    joinLit ? Theme.GOLD_200 : Theme.BONE_100);
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
    }

    /**
     * The chip under the pointer opens its bubble; with none there, the row picked with the keys does (TEAM6 §6.3).
     * Only chips whose row is in view count.
     */
    private void trackBubble(int mouseX, int mouseY, float top, long now) {
        BadgeBubble.Target target = null;
        float tx = getX() + FACE + 8;
        if (isHovered()) {
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
        if (target == null && keyboardFocus()) {
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
            if (rows.get(i).joinable) {
                output.add(NarratedElementType.USAGE, Texts.tr("narration.friend.usage"));
            }
        }
    }
}
