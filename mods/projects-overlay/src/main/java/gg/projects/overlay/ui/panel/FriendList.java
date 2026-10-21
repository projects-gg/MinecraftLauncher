package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
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

/**
 * Friends, the ones in a game first, then those in the launcher, then the rest quietly: a face with the
 * presence dot (filled jade in a game, a jade ring in the launcher, none offline), the name and where
 * they are, and "Katıl" when the player can go to them. Arrow keys move through the rows, Enter joins.
 */
public final class FriendList extends PanelWidget {
    private static final int ROW_H = 24;
    private static final int HEADER_H = 16;
    private static final int FACE = 16;
    private static final int JOIN_H = 14;

    private static final class Row {
        String id;
        boolean header;
        String title;
        String name;
        String where;
        boolean online;
        boolean inGame;
        boolean joinable;
        String initial;
        float y;
        float joinWidth;
        Component narration;
    }

    private final Consumer<String> onJoin;
    private final List<Row> rows = new ArrayList<>();
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
    }

    /**
     * Lays the list out again when the friends changed.
     *
     * @param canJoin whether "Katıl" makes sense for a friend now (not already on their server, connected)
     */
    public void refresh(OverlayState state, Predicate<OverlayState.Friend> canJoin) {
        if (state.revision() == revision && getWidth() == laidOutWidth) {
            return;
        }
        revision = state.revision();
        laidOutWidth = getWidth();
        Font small = Fonts.small();
        joinLabel = Texts.str("friend.join");
        emptyText = Texts.str(state.signedIn() ? "friends.empty" : "friends.signed_out");
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
        y = section(y, "friends.in_game", game, canJoin, joinWidth);
        y = section(y, "friends.online", launcher, canJoin, joinWidth);
        y = section(y, "friends.offline", offline, canJoin, joinWidth);
        contentHeight = y;
        if (selected != null && rows.stream().noneMatch(r -> !r.header && r.id.equals(selected))) {
            selected = null;
        }
        scroll.snap(Math.max(0f, Math.min(maxScroll(), scroll.target())));
    }

    private float section(float y, String titleKey, List<OverlayState.Friend> friends,
        Predicate<OverlayState.Friend> canJoin, float joinWidth) {
        if (friends.isEmpty()) {
            return y;
        }
        Row header = new Row();
        header.header = true;
        header.title = Texts.str(titleKey, friends.size());
        header.y = y;
        rows.add(header);
        y += HEADER_H;
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        int nameRoom = getWidth() - FACE - 8 - (int) joinWidth - 6;
        for (OverlayState.Friend f : friends) {
            Row row = new Row();
            row.id = f.id();
            row.name = Texts.fit(strong, f.name().isEmpty() ? Texts.str("toast.someone") : f.name(), nameRoom);
            row.online = f.online();
            row.inGame = f.inGame();
            row.joinable = f.joinable() && canJoin.test(f);
            String where = Texts.str("friend.where." + whereKey(f.where()));
            if (f.where().equals("server") && f.host() != null) {
                where = Texts.str("friend.where.server_at", f.host());
            }
            row.where = Texts.fit(small, where, nameRoom);
            row.initial = Texts.initial(f.name());
            row.joinWidth = joinWidth;
            row.y = y;
            row.narration = Texts.tr("narration.friend", row.name, where);
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
            Draw.text(g, small, emptyText, x0 + (w - small.width(emptyText)) / 2f,
                Fonts.centeredY(Fonts.Face.SMALL, y0 + h / 2f), Theme.ASH_400);
            return;
        }
        g.enableScissor(x0 - 2, y0, x0 + w + 2, y0 + h);
        float top = scroll.value();
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
                Draw.frame(g, Frames.CHIP_HOVER, x0 - 2, y + 1, w + 4, ROW_H - 2, 0x99FFFFFF);
            }
            float cx = x0 + FACE / 2f + 1;
            float cy = y + ROW_H / 2f;
            Draw.pushAlpha(row.online ? 1f : 0.55f);
            Draw.disc(g, FACE, cx, cy, Theme.OBSIDIAN_600);
            Draw.text(g, strong, row.initial, cx - strong.width(row.initial) / 2f,
                Fonts.centeredY(Fonts.Face.STRONG, cy), Theme.BONE_100);
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
            Draw.text(g, strong, row.name, tx, Fonts.centeredY(Fonts.Face.STRONG, y + 8), row.online
                ? Theme.BONE_100 : Theme.ASH_400);
            Draw.text(g, small, row.where, tx, Fonts.centeredY(Fonts.Face.SMALL, y + 17), row.inGame
                ? Theme.ASH_300 : Theme.ASH_400);
            if (row.joinable) {
                float jx = x0 + w - row.joinWidth;
                float jy = y + (ROW_H - JOIN_H) / 2f;
                boolean joinLit = lit && mouseX >= jx;
                Draw.frame(g, joinLit ? Frames.CHIP_SELECTED : Frames.CHIP, jx, jy, row.joinWidth, JOIN_H);
                Draw.text(g, small, joinLabel, jx + 7, Fonts.centeredY(Fonts.Face.SMALL, jy + JOIN_H / 2f),
                    joinLit ? Theme.GOLD_200 : Theme.BONE_100);
            }
        }
        g.disableScissor();
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
