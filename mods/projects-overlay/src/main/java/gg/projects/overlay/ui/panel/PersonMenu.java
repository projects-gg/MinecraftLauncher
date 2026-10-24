package gg.projects.overlay.ui.panel;

import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.people.FriendMenu;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
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
 * A friend's menu in the panel (OVL-UX; FIX2-D: the launcher's person menu, its rows and order from {@link FriendMenu}).
 * It opens at the pointer for a right click, under the row's "…" for its click, Shift+F10 or the menu key, and there
 * too for a click on a row with no conversation to open. Rows: a Phosphor sign and the words, soft rows dimmed, "Engelle"
 * in flare, groups apart by a hairline; the pointer and the arrow keys light the same row (as the launcher's menus).
 *
 * <p>A row that asks first ("Arkadaşlıktan çıkar", "Engelle") turns the popover into its question, one sentence (and
 * for a block what it means), "Vazgeç" and the action in the danger style; the keyboard starts on "Vazgeç". In: 120 ms,
 * fading in as it moves 3 px away from what opened it; out: 80 ms, fading. Under reduced motion it only fades. Opaque
 * ({@link Frames#POPOVER}): text never shows through it. Nothing runs while it is closed.
 */
public final class PersonMenu {
    /** What the panel does with a pick. */
    public interface Actions {
        /** A row was picked (one that asks: only after its question). */
        void picked(String friendId, FriendMenu.Action action);
    }

    /** What opened it: a point (the pointer) or a box (the "…"), in GUI pixels. */
    public record Anchor(float x, float y, float w, float h, boolean pointer) {
        public static Anchor at(double x, double y) {
            return new Anchor((float) x, (float) y, 0f, 0f, true);
        }
    }

    private static final int ROW_H = 14;
    private static final int SEP_H = 7;
    private static final int PAD = 3;
    private static final int ICON_X = 5;
    private static final int TEXT_X = 19;
    private static final int MIN_W = 116;
    private static final int EDGE = 4;
    private static final long IN_MS = 120;
    private static final long OUT_MS = 80;
    private static final int ASK_PAD = 8;
    private static final int ASK_MIN_W = 168;
    private static final int ASK_MAX_W = 224;
    private static final int BUTTON_H = ActionButton.HEIGHT;
    private static final int BUTTON_PAD = 7;
    private static final int BUTTON_GAP = 6;

    private record Line(FriendMenu.Row row, FormattedCharSequence label, float width, Icon icon) {
    }

    private final Actions actions;
    private final Tween alpha = new Tween(0f);
    private boolean reducedMotion;
    private String friendId;
    private String friendName = "";
    private Anchor anchor;
    private final List<Line> lines = new ArrayList<>();
    /** The lit row (pointer or keys), -1 for none. */
    private int focus = -1;
    private boolean keys;
    // The question, when a row asks first.
    private FriendMenu.Action asking;
    private final List<FormattedCharSequence> question = new ArrayList<>(2);
    private final List<FormattedCharSequence> body = new ArrayList<>(3);
    private String cancelLabel = "";
    private String confirmLabel = "";
    private float cancelW;
    private float confirmW;
    /** The question's lit button: 0 "Vazgeç", 1 the action. */
    private int button;
    private final Tween cancelHover = new Tween(0f);
    private final Tween confirmHover = new Tween(0f);
    private int litButton = -1;
    // Where it stands (whole pixels) and which way it came from.
    private float x;
    private float y;
    private float width;
    private float height;
    private boolean below;
    private long shownAt;
    private int lastMouseX = Integer.MIN_VALUE;
    private int lastMouseY = Integer.MIN_VALUE;

    public PersonMenu(Actions actions) {
        this.actions = actions;
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    /** Whether it is open (not fading out). */
    public boolean isOpen() {
        return friendId != null && alpha.target() > 0f;
    }

    /** The friend whose menu is open, or null. */
    public String friend() {
        return isOpen() ? friendId : null;
    }

    /** Whether its question shows now. */
    public boolean asking() {
        return isOpen() && asking != null;
    }

    /** Whether {@code (mx, my)} is on it. */
    public boolean contains(double mx, double my) {
        return isOpen() && mx >= x && mx < x + width && my >= y && my < y + height;
    }

    /** Opens {@code rows} for a friend; {@code keys}: from the keyboard (its first row lit). */
    public void open(String friend, String name, List<FriendMenu.Row> rows, Anchor at, boolean fromKeys, int screenW,
        int screenH, long now) {
        friendId = friend;
        friendName = name == null ? "" : name;
        anchor = at;
        keys = fromKeys;
        asking = null;
        Font ui = Fonts.ui();
        lines.clear();
        float widest = 0f;
        for (FriendMenu.Row row : rows) {
            if (row == null) {
                lines.add(null);
                continue;
            }
            String words = Texts.str(labelKey(row));
            float w = ui.width(words);
            widest = Math.max(widest, w);
            lines.add(new Line(row, Texts.visual(words), w, icon(row)));
        }
        width = Math.max(MIN_W, (float) Math.ceil(TEXT_X + widest + 10));
        width = Math.min(width, screenW - 2 * EDGE);
        height = PAD * 2;
        for (Line line : lines) {
            height += line == null ? SEP_H : ROW_H;
        }
        place(screenW, screenH);
        focus = fromKeys ? next(-1, 1) : -1;
        lastMouseX = Integer.MIN_VALUE;
        appear(now);
        int count = 0;
        for (Line line : lines) {
            if (line != null) {
                count++;
            }
        }
        Component said = Texts.tr("people.menu.narration", friendName, count);
        if (focus >= 0) {
            said = Component.empty().append(said).append(". ").append(Component.literal(Texts.str(labelKey(
                lines.get(focus).row()))));
        }
        Minecraft.getInstance().getNarrator().saySystemQueued(said);
    }

    /** Closes it (fading out); whether it was open. */
    public boolean close(long now) {
        if (!isOpen()) {
            return false;
        }
        alpha.animateTo(0f, now, OUT_MS, Ease.OUT);
        return true;
    }

    /** Closes it at once (the list changed under it, the panel went to another page). */
    public void dismiss() {
        alpha.snap(0f);
        friendId = null;
        asking = null;
    }

    private void appear(long now) {
        shownAt = now;
        alpha.snap(0f);
        alpha.animateTo(1f, now, IN_MS, Ease.OUT);
    }

    private void place(int screenW, int screenH) {
        if (anchor.pointer()) {
            x = anchor.x() + 1;
            if (x + width > screenW - EDGE) {
                x = anchor.x() - width - 1;
            }
            y = anchor.y() + 1;
            below = true;
            if (y + height > screenH - EDGE) {
                y = anchor.y() - height - 1;
                below = false;
            }
        } else {
            // Under the "…", its right edge on the button's.
            x = anchor.x() + anchor.w() - width;
            y = anchor.y() + anchor.h() + 2;
            below = true;
            if (y + height > screenH - EDGE) {
                y = anchor.y() - height - 2;
                below = false;
            }
        }
        x = Draw.snap(Math.max(EDGE, Math.min(screenW - EDGE - width, x)));
        y = Draw.snap(Math.max(EDGE, Math.min(screenH - EDGE - height, y)));
    }

    private static String labelKey(FriendMenu.Row row) {
        return switch (row.action()) {
            case JOIN -> "people.join";
            case INVITE -> row.done() ? "people.invited" : "people.invite";
            case MESSAGE -> "people.message";
            case DISCORD -> "people.discord";
            case REMOVE_FROM_WORLD -> "people.remove_from_world";
            case UNFRIEND -> "people.unfriend";
            case BLOCK -> "people.block";
            case REPORT -> "people.report";
        };
    }

    private static Icon icon(FriendMenu.Row row) {
        return switch (row.action()) {
            case JOIN -> Icon.JOIN;
            case INVITE -> row.done() ? Icon.CHECK : Icon.PLAY_INVITE;
            case MESSAGE -> Icon.MESSAGE;
            case DISCORD -> Icon.DISCORD;
            case REMOVE_FROM_WORLD -> Icon.SEND_OFF;
            case UNFRIEND -> Icon.UNFRIEND;
            case BLOCK -> Icon.BLOCK;
            case REPORT -> Icon.REPORT;
        };
    }

    /**
     * The next row that can be picked after {@code from} in {@code step}'s direction (from the top or the bottom when
     * {@code from} is -1), wrapping; -1 for none.
     */
    private int next(int from, int step) {
        int n = lines.size();
        int at = from >= 0 ? from : step > 0 ? -1 : n;
        for (int i = 0; i < n; i++) {
            at = Math.floorMod(at + step, n);
            Line line = lines.get(at);
            if (line != null && !line.row().disabled()) {
                return at;
            }
        }
        return -1;
    }

    // ---- The question ---------------------------------------------------------------------------------

    private void ask(FriendMenu.Action action, int screenW, int screenH, long now) {
        asking = action;
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        Font ui = Fonts.ui();
        cancelLabel = Texts.str("people.cancel");
        confirmLabel = Texts.str(action == FriendMenu.Action.BLOCK ? "people.block" : "people.unfriend");
        cancelW = ui.width(cancelLabel) + BUTTON_PAD * 2;
        confirmW = ui.width(confirmLabel) + BUTTON_PAD * 2;
        float buttons = cancelW + BUTTON_GAP + confirmW;
        float maxW = Math.min(ASK_MAX_W, screenW - 2 * EDGE);
        width = Math.min(maxW, Math.max(ASK_MIN_W, buttons + ASK_PAD * 2));
        int textW = Math.round(width - ASK_PAD * 2);
        String title = Texts.str(action == FriendMenu.Action.BLOCK ? "people.block.ask" : "people.unfriend.ask",
            friendName.isEmpty() ? Texts.str("toast.someone") : friendName);
        question.clear();
        question.addAll(Texts.wrap(strong, title, textW, 2));
        body.clear();
        if (action == FriendMenu.Action.BLOCK) {
            body.addAll(Texts.wrap(small, Texts.str("people.block.body"), textW, 3));
        }
        height = ASK_PAD + question.size() * 10 + (body.isEmpty() ? 0 : 3 + body.size() * 9) + 9 + BUTTON_H + ASK_PAD;
        place(screenW, screenH);
        // A destructive answer never comes from a reflex: the keys start on "Vazgeç".
        button = 0;
        litButton = -1;
        cancelHover.snap(0f);
        confirmHover.snap(0f);
        appear(now);
        String said = String.join(" ", question.stream().map(PersonMenu::plain).toList());
        if (!body.isEmpty()) {
            said += ". " + Texts.str("people.block.body");
        }
        Minecraft.getInstance().getNarrator().saySystemQueued(Component.literal(said));
    }

    private static String plain(FormattedCharSequence seq) {
        StringBuilder b = new StringBuilder();
        seq.accept((index, style, cp) -> {
            b.appendCodePoint(cp);
            return true;
        });
        return b.toString();
    }

    private float buttonsY() {
        return y + height - ASK_PAD - BUTTON_H;
    }

    private float confirmX() {
        return x + width - ASK_PAD - confirmW;
    }

    private float cancelX() {
        return confirmX() - BUTTON_GAP - cancelW;
    }

    /** The question's button under the pointer: 0, 1 or -1. */
    private int buttonAt(double mx, double my) {
        float by = buttonsY();
        if (my < by || my >= by + BUTTON_H) {
            return -1;
        }
        if (mx >= cancelX() && mx < cancelX() + cancelW) {
            return 0;
        }
        if (mx >= confirmX() && mx < confirmX() + confirmW) {
            return 1;
        }
        return -1;
    }

    // ---- Input ----------------------------------------------------------------------------------------

    /** The row under the pointer, or -1. */
    private int rowAt(double mx, double my) {
        if (!contains(mx, my) || asking != null) {
            return -1;
        }
        float ry = y + PAD;
        for (int i = 0; i < lines.size(); i++) {
            Line line = lines.get(i);
            float h = line == null ? SEP_H : ROW_H;
            if (my >= ry && my < ry + h) {
                return line == null ? -1 : i;
            }
            ry += h;
        }
        return -1;
    }

    /**
     * A press while it is open: on it, a row (or a question's button) does its thing; beside it, it closes. Always
     * taken (a press beside it closes only it).
     */
    public void mouseClicked(double mx, double my, int screenW, int screenH, long now) {
        if (!contains(mx, my)) {
            close(now);
            return;
        }
        PanelInput.usedMouse();
        if (asking != null) {
            int b = buttonAt(mx, my);
            if (b == 0) {
                close(now);
            } else if (b == 1) {
                confirm(now);
            }
            return;
        }
        int at = rowAt(mx, my);
        if (at >= 0 && !lines.get(at).row().disabled()) {
            pick(at, screenW, screenH, now);
        }
    }

    private void pick(int index, int screenW, int screenH, long now) {
        FriendMenu.Action action = lines.get(index).row().action();
        if (action.asks()) {
            ask(action, screenW, screenH, now);
            return;
        }
        String friend = friendId;
        close(now);
        actions.picked(friend, action);
    }

    /** Presses the row of {@code action} as a click would (the scenario driver); false when it is not there or soft. */
    public boolean press(FriendMenu.Action action, int screenW, int screenH, long now) {
        if (!isOpen() || asking != null) {
            return false;
        }
        for (int i = 0; i < lines.size(); i++) {
            Line line = lines.get(i);
            if (line != null && line.row().action() == action && !line.row().disabled()) {
                focus = i;
                pick(i, screenW, screenH, now);
                return true;
            }
        }
        return false;
    }

    /** Answers the question as a click would (the scenario driver): the action, or "Vazgeç". */
    public void answer(boolean yes, long now) {
        if (asking == null || !isOpen()) {
            return;
        }
        if (yes) {
            confirm(now);
        } else {
            close(now);
        }
    }

    private void confirm(long now) {
        String friend = friendId;
        FriendMenu.Action action = asking;
        close(now);
        if (action != null) {
            actions.picked(friend, action);
        }
    }

    /** Keys while it is open; whether it took the key. Esc closes it (the panel asks first). */
    public boolean keyPressed(KeyEvent event, int screenW, int screenH, long now) {
        if (!isOpen()) {
            return false;
        }
        if (event.isEscape()) {
            close(now);
            return true;
        }
        PanelInput.usedKeyboard();
        if (asking != null) {
            if (event.isLeft() || event.isRight() || event.isCycleFocus()) {
                button = 1 - button;
                return true;
            }
            if (event.isSelection() || event.isConfirmation()) {
                if (button == 0) {
                    close(now);
                } else {
                    confirm(now);
                }
                return true;
            }
            return true;
        }
        if (event.isUp() || event.isDown()) {
            focus = next(focus, event.isUp() ? -1 : 1);
            sayFocus();
            return true;
        }
        if (event.isCycleFocus()) {
            // As the launcher's menus: Tab leaves the menu.
            close(now);
            return false;
        }
        if (event.isSelection() || event.isConfirmation()) {
            if (focus < 0) {
                focus = next(-1, 1);
                sayFocus();
            } else {
                pick(focus, screenW, screenH, now);
            }
            return true;
        }
        // Nothing else reaches the panel under an open menu.
        return true;
    }

    private void sayFocus() {
        if (focus >= 0) {
            Minecraft.getInstance().getNarrator().saySystemQueued(Texts.tr(labelKey(lines.get(focus).row())));
        }
    }

    // ---- Drawing --------------------------------------------------------------------------------------

    /** Draws it (over everything in the panel). */
    public void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        if (friendId == null) {
            return;
        }
        float a = alpha.value(now);
        if (a <= 0.01f) {
            if (alpha.target() <= 0f) {
                friendId = null;
                asking = null;
            }
            return;
        }
        boolean moved = mouseX != lastMouseX || mouseY != lastMouseY;
        lastMouseX = mouseX;
        lastMouseY = mouseY;
        boolean coming = alpha.target() > 0f && !reducedMotion;
        float shift = coming ? (1f - Ease.OUT.at(Math.min(1f, (now - shownAt) / (float) IN_MS))) * 3f : 0f;
        Matrix3x2fStack m = g.pose();
        m.pushMatrix();
        m.translate(0f, below ? -shift : shift);
        Draw.pushAlpha(a);
        Draw.frame(g, Frames.POPOVER, x, y, width, height);
        if (asking == null) {
            if (moved && contains(mouseX, mouseY)) {
                // The pointer lights the row it is on, as the arrow keys do.
                int at = rowAt(mouseX, mouseY);
                if (at >= 0 && !lines.get(at).row().disabled()) {
                    if (at != focus) {
                        focus = at;
                    }
                } else {
                    focus = -1;
                }
            }
            drawRows(g);
        } else {
            drawQuestion(g, mouseX, mouseY, now);
        }
        Draw.popAlpha();
        m.popMatrix();
    }

    private void drawRows(GuiGraphicsExtractor g) {
        Font ui = Fonts.ui();
        float ry = y + PAD;
        for (int i = 0; i < lines.size(); i++) {
            Line line = lines.get(i);
            if (line == null) {
                Draw.hairline(g, x + PAD + 4, x + width - PAD - 4, ry + SEP_H / 2f, Theme.withAlpha(0xFFFFFF, 0.08f));
                ry += SEP_H;
                continue;
            }
            boolean soft = line.row().disabled();
            boolean lit = i == focus && !soft;
            if (lit) {
                Draw.frame(g, Frames.ROW_HOVER, x + PAD, ry, width - PAD * 2, ROW_H);
            }
            boolean danger = line.row().action().danger();
            int ink = danger ? Theme.FLARE_300 : Theme.BONE_100;
            int iconInk = danger ? Theme.FLARE_300 : lit ? Theme.BONE_100 : Theme.ASH_300;
            if (line.row().done()) {
                iconInk = Theme.JADE_400;
            }
            if (soft) {
                Draw.pushAlpha(0.45f);
            }
            Draw.icon(g, line.icon(), 10, x + ICON_X, ry + (ROW_H - 10) / 2f, iconInk);
            Draw.text(g, ui, line.label(), x + TEXT_X, Fonts.centeredY(Fonts.Face.UI, ry + ROW_H / 2f), ink);
            if (soft) {
                Draw.popAlpha();
            }
            ry += ROW_H;
        }
    }

    private void drawQuestion(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float tx = x + ASK_PAD;
        float ty = y + ASK_PAD;
        Font strong = Fonts.strong();
        Font small = Fonts.small();
        for (FormattedCharSequence line : question) {
            Draw.text(g, strong, line, tx, Fonts.centeredY(Fonts.Face.STRONG, ty + 5), Theme.BONE_50);
            ty += 10;
        }
        if (!body.isEmpty()) {
            ty += 3;
            for (FormattedCharSequence line : body) {
                Draw.text(g, small, line, tx, Fonts.centeredY(Fonts.Face.SMALL, ty + 4.5f), Theme.ASH_300);
                ty += 9;
            }
        }
        int lit = contains(mouseX, mouseY) ? buttonAt(mouseX, mouseY) : -1;
        if (lit != litButton) {
            cancelHover.animateTo(lit == 0 ? 1f : 0f, now, PanelWidget.HOVER_MS, Ease.OUT);
            confirmHover.animateTo(lit == 1 ? 1f : 0f, now, PanelWidget.HOVER_MS, Ease.OUT);
            litButton = lit;
            if (lit >= 0) {
                button = lit;
            }
        }
        float by = buttonsY();
        drawButton(g, cancelX(), by, cancelW, cancelLabel, Frames.BUTTON_SECONDARY, Frames.BUTTON_SECONDARY_HOVER,
            cancelHover.value(now), Theme.BONE_100, button == 0 && PanelInput.keyboardMode());
        drawButton(g, confirmX(), by, confirmW, confirmLabel, Frames.BUTTON_DANGER, Frames.BUTTON_DANGER_HOVER,
            confirmHover.value(now), Theme.mix(Theme.FLARE_300, Theme.BONE_50, confirmHover.value(now) * 0.35f),
            button == 1 && PanelInput.keyboardMode());
    }

    private static void drawButton(GuiGraphicsExtractor g, float bx, float by, float bw, String label, Frames face,
        Frames hoverFace, float hover, int ink, boolean ring) {
        Draw.frame(g, face, bx, by, bw, BUTTON_H);
        if (hover > 0f) {
            Draw.pushAlpha(hover);
            Draw.frame(g, hoverFace, bx, by, bw, BUTTON_H);
            Draw.popAlpha();
        }
        if (ring) {
            Draw.frame(g, Frames.FOCUS_16, bx, by, bw, BUTTON_H);
        }
        Font ui = Fonts.ui();
        Draw.text(g, ui, label, bx + (bw - ui.width(label)) / 2f, Fonts.centeredY(Fonts.Face.UI, by + BUTTON_H / 2f),
            ink);
    }
}
