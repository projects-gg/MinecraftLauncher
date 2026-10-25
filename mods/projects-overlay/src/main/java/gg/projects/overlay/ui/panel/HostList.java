package gg.projects.overlay.ui.panel;

import gg.projects.overlay.host.HostController;
import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.host.Names;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Spring;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.Face;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.TagChip;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import gg.projects.overlay.ui.draw.ShapeSet;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Consumer;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;

/**
 * The people of world sharing (HOST1, host1.md §4.8). Picking: the friends online now, the ones in a game first, each
 * with a box to tick, up to the world's room. Guests: the friends in the session, "Dünyada" / "Bağlanıyor" /
 * "Çağrıldı", the game name when it is not their own, and on the row under the pointer (or picked with the keys) the
 * sign that sends them off. A row whose state changes fades in place and the rows below glide (the sliding markers'
 * spring); under reduced motion only the fades stay. Arrow keys move, Space or Enter ticks or opens "Dünyadan çıkar".
 */
public final class HostList extends PanelWidget {
    public enum Mode { PICK, GUESTS }

    private static final int ROW_H = 24;
    private static final int FACE = 16;
    private static final int CHECK = 10;
    private static final int REMOVE = 16;
    private static final float NAME_MID = 8f;
    private static final long FADE_IN_MS = 160;
    private static final long FADE_OUT_MS = 110;

    /** One person's row. */
    public static final class Row {
        public final String id;
        /** The guest id once their launcher connected (guests only), else empty. */
        public String guest = "";
        public String label = "";
        FormattedCharSequence name;
        FormattedCharSequence sub;
        /** Their face: the launcher's player head with their first letter (OVL-UX2). */
        Face face;
        boolean online;
        boolean inGame;
        boolean dim;
        boolean removable;
        TagChip chip;
        Component narration;
        final Spring y = Spring.snappy(0f);
        final Tween alpha = new Tween(0f);
        boolean leaving;
        long leftAt;
        float target;

        Row(String id) {
            this.id = id;
        }
    }

    private final Consumer<Row> onRemove;
    private final Runnable onPicksChanged;
    private final List<Row> rows = new ArrayList<>();
    private final Set<String> picked = new LinkedHashSet<>();
    private final Spring scroll = Spring.snappy(0f);
    private Mode mode = Mode.PICK;
    private int max = 7;
    private float contentHeight;
    private String cursor;
    private String emptyText = "";
    private FormattedCharSequence emptyLine = FormattedCharSequence.EMPTY;
    private float emptyWidth;
    private boolean reducedMotion;
    private long lastFrame;
    private boolean laidOut;

    public HostList(Consumer<Row> onRemove, Runnable onPicksChanged) {
        super(0, 0, 100, 100, Texts.tr("host.friends"));
        this.onRemove = onRemove;
        this.onPicksChanged = onPicksChanged;
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    public Mode mode() {
        return mode;
    }

    /** The friends ticked, in the order they were ticked. */
    public List<String> picked() {
        return List.copyOf(picked);
    }

    public int max() {
        return max;
    }

    /**
     * Ticks these friends again when nothing is ticked (a failed opening's, so "Tekrar dene" works); those not on the
     * list drop out when it is shown.
     */
    public void restorePicks(List<String> ids) {
        if (!picked.isEmpty() || ids.isEmpty()) {
            return;
        }
        for (String id : ids) {
            if (picked.size() < HostController.MAX_INVITE) {
                picked.add(id);
            }
        }
        onPicksChanged.run();
    }

    public void clearPicks() {
        if (!picked.isEmpty()) {
            picked.clear();
            onPicksChanged.run();
        }
    }

    /** Whether any row is on screen (an empty list says so in one line instead). */
    public boolean isEmpty() {
        return rows.stream().noneMatch(r -> !r.leaving);
    }

    /**
     * The friends online now (in a game first, then the launcher), but not {@code except} (already in the session),
     * with room for {@code room} picks. Picks of friends who went away are dropped.
     */
    public void showPick(OverlayState state, Set<String> except, int room, String emptyKey) {
        boolean modeChanged = mode != Mode.PICK;
        mode = Mode.PICK;
        max = Math.max(0, room);
        setMessage(Texts.tr("host.friends"));
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        int nameRoom = getWidth() - CHECK - 6 - FACE - 8;
        List<Row> next = new ArrayList<>();
        List<OverlayState.Friend> game = new ArrayList<>();
        List<OverlayState.Friend> launcher = new ArrayList<>();
        for (OverlayState.Friend f : state.friends()) {
            if (except.contains(f.id()) || !f.online()) {
                continue;
            }
            (f.inGame() ? game : launcher).add(f);
        }
        game.addAll(launcher);
        Map<String, Row> old = byId();
        for (OverlayState.Friend f : game) {
            Row row = modeChanged ? null : old.get(f.id());
            row = row == null ? new Row(f.id()) : row;
            row.label = f.name().isEmpty() ? Texts.str("toast.someone") : f.name();
            OverlayState.Badge badge = state.badge(f.badge());
            row.chip = TagChip.of(badge, TagChip.ROW);
            row.name = Texts.visual(Texts.fit(strong, row.label, Math.round(nameRoom - TagChip.room(row.chip))));
            String where = Texts.str("friend.where." + whereKey(f.where()));
            if (f.where().equals("server") && f.host() != null) {
                where = Texts.str("friend.where.server_at", f.host());
            }
            row.sub = Texts.visual(Texts.fit(small, where, nameRoom));
            row.online = true;
            row.inGame = f.inGame();
            row.dim = false;
            row.removable = false;
            row.guest = "";
            row.face = Face.of(row.label);
            row.narration = Texts.tr("narration.friend", badge == null ? row.label
                : Texts.str("row.badge", row.label, badge.tag()), where);
            next.add(row);
        }
        Set<String> ids = new java.util.HashSet<>();
        for (Row r : next) {
            ids.add(r.id);
        }
        boolean dropped = picked.removeIf(id -> !ids.contains(id));
        while (picked.size() > max) {
            Iterator<String> it = picked.iterator();
            String last = null;
            while (it.hasNext()) {
                last = it.next();
            }
            picked.remove(last);
            dropped = true;
        }
        setEmpty(emptyKey);
        apply(next, modeChanged);
        if (dropped) {
            onPicksChanged.run();
        }
    }

    /** The session's guests as the launcher mirrors them. */
    public void showGuests(OverlayState.HostView view) {
        boolean modeChanged = mode != Mode.GUESTS;
        mode = Mode.GUESTS;
        setMessage(Texts.tr("host.guests"));
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        int nameRoom = getWidth() - FACE - 8 - REMOVE - 4;
        Map<String, Row> old = byId();
        List<Row> next = new ArrayList<>();
        // In the world first, then on the way, then invited: who is here reads first.
        List<OverlayState.HostGuest> ordered = new ArrayList<>(view.guests());
        ordered.sort((a, b) -> Integer.compare(rank(a), rank(b)));
        for (OverlayState.HostGuest g : ordered) {
            Row row = modeChanged ? null : old.get(g.friend());
            row = row == null ? new Row(g.friend()) : row;
            row.guest = g.guest();
            row.label = g.label().isEmpty() ? Texts.str("toast.someone") : g.label();
            row.chip = null;
            row.name = Texts.visual(Texts.fit(strong, row.label, nameRoom));
            String state;
            if (g.in()) {
                boolean own = g.name().isEmpty() || Names.same(g.name(), row.label);
                state = own ? Texts.str("host.guest.in") : Texts.str("host.guest.in_as", g.name());
            } else if (g.joining()) {
                state = Texts.str("host.guest.joining");
            } else {
                state = Texts.str("host.guest.invited");
            }
            row.sub = Texts.visual(Texts.fit(small, state, nameRoom));
            row.online = g.in() || g.joining();
            row.inGame = g.in();
            row.dim = !row.online;
            row.removable = !g.guest().isEmpty();
            row.face = Face.of(row.label);
            row.narration = Texts.tr("narration.friend", row.label, state);
            next.add(row);
        }
        picked.clear();
        setEmpty("host.none");
        apply(next, modeChanged);
    }

    private static int rank(OverlayState.HostGuest g) {
        return g.in() ? 0 : g.joining() ? 1 : 2;
    }

    private void setEmpty(String key) {
        emptyText = Texts.str(key);
        emptyLine = Texts.visual(Texts.fit(Fonts.small(), emptyText, getWidth()));
        emptyWidth = Fonts.small().width(emptyText);
    }

    private Map<String, Row> byId() {
        Map<String, Row> map = new HashMap<>();
        for (Row r : rows) {
            if (!r.leaving) {
                map.put(r.id, r);
            }
        }
        return map;
    }

    /** New rows fade in where they land, rows that stay glide to their place, rows that went fade out where they were. */
    private void apply(List<Row> next, boolean fresh) {
        long now = now();
        Set<Row> kept = new java.util.HashSet<>(next);
        for (Row r : rows) {
            if (!kept.contains(r) && !r.leaving) {
                r.leaving = true;
                r.leftAt = now;
                r.alpha.animateTo(0f, now, FADE_OUT_MS, Ease.OUT);
            }
        }
        float y = 0f;
        for (Row r : next) {
            r.target = y;
            boolean isNew = !rows.contains(r) || r.leaving;
            r.leaving = false;
            if (isNew || fresh || !laidOut || reducedMotion) {
                r.y.snap(y);
            } else {
                r.y.setTarget(y);
            }
            if (isNew) {
                r.alpha.snap(fresh || !laidOut ? 1f : 0f);
                r.alpha.animateTo(1f, now, FADE_IN_MS, Ease.OUT);
            }
            y += ROW_H;
        }
        List<Row> gone = new ArrayList<>();
        for (Row r : rows) {
            if (r.leaving && !fresh) {
                gone.add(r);
            }
        }
        rows.clear();
        rows.addAll(next);
        rows.addAll(gone);
        contentHeight = y;
        laidOut = true;
        // A row picked with the keys meant something on the other list ("tick Mert"), not here.
        if (fresh || (cursor != null && next.stream().noneMatch(r -> r.id.equals(cursor)))) {
            cursor = null;
        }
        scroll.snap(Math.max(0f, Math.min(maxScroll(), scroll.target())));
    }

    private float maxScroll() {
        return Math.max(0f, contentHeight - getHeight());
    }

    private List<Row> live() {
        List<Row> out = new ArrayList<>();
        for (Row r : rows) {
            if (!r.leaving) {
                out.add(r);
            }
        }
        return out;
    }

    private void toggle(Row row) {
        if (mode != Mode.PICK || !active) {
            return;
        }
        if (picked.remove(row.id)) {
            onPicksChanged.run();
        } else if (picked.size() < max) {
            picked.add(row.id);
            onPicksChanged.run();
        }
    }

    private void move(int direction) {
        List<Row> live = live();
        if (live.isEmpty()) {
            return;
        }
        int i = -1;
        for (int k = 0; k < live.size(); k++) {
            if (live.get(k).id.equals(cursor)) {
                i = k;
            }
        }
        int next = i < 0 ? (direction > 0 ? 0 : live.size() - 1) : Math.max(0, Math.min(live.size() - 1, i + direction));
        cursor = live.get(next).id;
        reveal(live.get(next));
    }

    private void reveal(Row row) {
        float target = scroll.target();
        if (row.target < target) {
            target = row.target;
        } else if (row.target + ROW_H > target + getHeight()) {
            target = row.target + ROW_H - getHeight();
        }
        target = Math.max(0f, Math.min(maxScroll(), target));
        if (reducedMotion) {
            scroll.snap(target);
        } else {
            scroll.setTarget(target);
        }
    }

    private Row cursorRow() {
        for (Row r : rows) {
            if (!r.leaving && r.id.equals(cursor)) {
                return r;
            }
        }
        return null;
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (event.isUp() || event.isDown()) {
            PanelInput.usedKeyboard();
            move(event.isUp() ? -1 : 1);
            return true;
        }
        Row row = cursorRow();
        if (row != null && (event.isSelection() || event.isConfirmation()
            || (mode == Mode.GUESTS && event.key() == InputConstants.KEY_DELETE))) {
            PanelInput.usedKeyboard();
            if (mode == Mode.PICK) {
                toggle(row);
            } else if (row.removable) {
                onRemove.accept(row);
            }
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
        for (Row row : live()) {
            if (cy < row.target || cy >= row.target + ROW_H) {
                continue;
            }
            cursor = row.id;
            if (mode == Mode.PICK) {
                toggle(row);
            } else if (row.removable && event.x() >= getX() + getWidth() - REMOVE - 2) {
                onRemove.accept(row);
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
        rows.removeIf(r -> r.leaving && now - r.leftAt > FADE_OUT_MS + 20);
        if (isEmpty()) {
            Draw.text(g, small, emptyLine, x0 + Math.max(0f, (w - emptyWidth) / 2f),
                Fonts.centeredY(Fonts.Face.SMALL, y0 + Math.min(h / 2f, 20f)), Theme.ASH_400);
            return;
        }
        float top = scroll.value();
        g.enableScissor(x0 - 2, y0, x0 + w + 2, y0 + h);
        boolean full = mode == Mode.PICK && picked.size() >= max;
        for (Row row : rows) {
            row.y.step(dt);
            float a = row.alpha.value(now);
            float y = y0 + row.y.value() - top;
            if (a <= 0.01f || y > y0 + h || y + ROW_H < y0) {
                continue;
            }
            boolean lit = !row.leaving && isHovered() && mouseY >= y && mouseY < y + ROW_H;
            boolean chosen = !row.leaving && row.id.equals(cursor) && keyboardFocus();
            boolean ticked = mode == Mode.PICK && picked.contains(row.id);
            boolean blocked = full && !ticked;
            Draw.pushAlpha(a * (blocked ? 0.45f : 1f));
            if ((lit && !blocked) || chosen) {
                Draw.frame(g, Frames.ROW_HOVER, x0 - 2, y + 1, w + 4, ROW_H - 2, 0x99FFFFFF);
            }
            if (chosen) {
                Draw.frame(g, Frames.FOCUS_ROW, x0 - 2, y + 1, w + 4, ROW_H - 2);
            }
            float fx = x0 + 1;
            if (mode == Mode.PICK) {
                float cy = y + (ROW_H - CHECK) / 2f;
                Draw.frame(g, ticked ? Frames.CHECK_ON : Frames.CHECK_OFF, x0 + 1, cy, CHECK, CHECK);
                if (ticked) {
                    Draw.icon(g, Icon.CHECK, 8, x0 + 2, cy + 1, Theme.GOLD_INK);
                }
                fx = x0 + 1 + CHECK + 6;
            }
            float cx = fx + FACE / 2f;
            float cy = y + ROW_H / 2f;
            Draw.pushAlpha(row.dim ? 0.55f : 1f);
            row.face.draw(g, FACE, cx, cy);
            Draw.popAlpha();
            if (row.online) {
                // Filled jade in a game (in the world), a jade ring in the launcher (on the way).
                Draw.disc(g, 8, cx + 6, cy + 6, Theme.OBSIDIAN_900);
                Draw.disc(g, 5, cx + 6, cy + 6, Theme.JADE_400);
                if (!row.inGame) {
                    Draw.disc(g, 3, cx + 6, cy + 6, Theme.OBSIDIAN_900);
                }
            }
            float tx = fx + FACE + 8;
            if (row.chip != null) {
                row.chip.draw(g, tx, y + NAME_MID);
            }
            Draw.text(g, strong, row.name, tx + TagChip.room(row.chip), Fonts.centeredY(Fonts.Face.STRONG, y + NAME_MID),
                row.dim ? Theme.ASH_300 : ticked ? Theme.BONE_50 : Theme.BONE_100);
            Draw.text(g, small, row.sub, tx, Fonts.centeredY(Fonts.Face.SMALL, y + 17),
                row.inGame ? Theme.ASH_300 : Theme.ASH_400);
            if (mode == Mode.GUESTS && row.removable && (lit || chosen)) {
                float rx = x0 + w - REMOVE;
                float ry = y + (ROW_H - REMOVE) / 2f;
                boolean over = lit && mouseX >= rx - 2;
                if (over) {
                    Draw.frame(g, Frames.CELL_HOVER, rx, ry, REMOVE, REMOVE);
                }
                Draw.icon(g, Icon.USER_MINUS, 10, rx + 3, ry + 3, over ? Theme.FLARE_300 : Theme.ASH_300);
            }
            Draw.popAlpha();
        }
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

    private static String whereKey(String where) {
        return switch (where) {
            case "projects", "launcher", "single", "server", "game", "offline" -> where;
            default -> "game";
        };
    }

    /**
     * Where a row is on screen (the scenario driver points at it): its middle, and the middle of its remove sign; null
     * when it is not in the list.
     */
    public float[] rowCenter(String friendId) {
        for (Row row : live()) {
            if (row.id.equals(friendId)) {
                float y = getY() + row.target - scroll.value() + ROW_H / 2f;
                return new float[] {getX() + getWidth() / 2f, y, getX() + getWidth() - REMOVE / 2f};
            }
        }
        return null;
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, getMessage());
        Row row = cursorRow();
        if (row != null) {
            output.add(NarratedElementType.HINT, mode == Mode.PICK
                ? Texts.tr(picked.contains(row.id) ? "host.narration.picked" : "host.narration.not_picked",
                    row.narration)
                : row.narration);
            output.add(NarratedElementType.USAGE, Texts.tr(mode == Mode.PICK ? "host.narration.pick_usage"
                : "host.narration.remove_usage"));
        }
    }
}
