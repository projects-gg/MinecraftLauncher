package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.InputConstants;
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
import java.util.function.Function;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.util.FormattedCharSequence;
import org.joml.Matrix3x2fStack;

/**
 * A person's menu in the panel (OVL-UX, OVL-UX2; FIX2-D: the launcher's person menu, its rows and order from
 * {@link FriendMenu}). It opens at the pointer for a right click, under the row's "…" (or the chip) for its click,
 * Shift+F10 or the menu key, and there too for a click on a row with no conversation to open. Rows: a Phosphor sign and
 * the words, soft rows dimmed, "Engelle" in flare, groups apart by a hairline; a check on the right where the
 * launcher's menu has one (a set online notice, an invite that went, the category they are in), a caret where the row
 * opens a list; the pointer and the arrow keys light the same row (as the launcher's menus).
 *
 * <p>A row with a list ("Kategoriye taşı", "Ekibe davet et" with several crews) turns the popover into that list, in
 * place: a row back to the menu first ("‹ Kategoriye taşı"), then the choices; → or Enter opens it, ← goes back, Esc
 * closes it all. In place rather than beside it: the panel stands at the screen's right edge, where a list beside the
 * menu would cover the friend list, and at GUI 3 it would not fit.
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

        /** A choice of a row's list (OVL-UX2): a category ({@code ""} for "Arkadaşlar") or a crew, by id. */
        default void chose(String friendId, FriendMenu.Action action, String choice) {
        }

        /** "Yeni kategori…" (OVL-UX3): a new category's name, its spaces folded, for the friend to move into. */
        default void named(String friendId, String name) {
        }
    }

    /**
     * The id of "Yeni kategori…" in "Kategoriye taşı"'s list (OVL-UX3): no category's (those are lower case letters
     * and digits, "" for "Arkadaşlar"). Picking it turns the list into a one-line name field in place.
     */
    public static final String NEW_CHOICE = "+new";
    /** The most a category's name keeps (the launcher's {@code GROUP_NAME_MAX}). */
    private static final int NAME_MAX = FriendMenu.CATEGORY_NAME_MAX;

    /** A choice in a row's list (OVL-UX2): {@code checked} where the launcher's menu has its check. */
    public record Choice(String id, String label, boolean disabled, boolean checked) {
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
    /** The column on the right for a check or a caret. */
    private static final int MARK_W = 12;
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

    /**
     * A line: a menu row ({@code row}), the way back from a list ({@code back}), or one of a list's choices
     * ({@code choice}); a null line is a hairline.
     */
    private record Line(FriendMenu.Row row, boolean back, Choice choice, FormattedCharSequence label, float width,
        Icon icon, boolean checked) {
        boolean disabled() {
            return choice != null ? choice.disabled() : row != null && row.disabled();
        }

        boolean danger() {
            return row != null && row.action().danger();
        }

        String words() {
            return choice != null ? choice.label() : Texts.str(labelKey(row));
        }
    }

    private final Actions actions;
    private final Tween alpha = new Tween(0f);
    private boolean reducedMotion;
    private String friendId;
    private String friendName = "";
    private Anchor anchor;
    private final List<Line> lines = new ArrayList<>();
    /** The menu's own rows (a list goes back to them). */
    private List<FriendMenu.Row> rows = List.of();
    private Function<FriendMenu.Action, List<Choice>> choices = action -> List.of();
    /** The row whose list shows now, or null. */
    private FriendMenu.Action listOf;
    /** OVL-UX3: the name typed into "Yeni kategori…"'s field while it shows, else null. */
    private StringBuilder naming;
    /** The lit row (pointer or keys), -1 for none. */
    private int focus = -1;
    private boolean keys;
    // The question, when a row asks first.
    private FriendMenu.Action asking;
    /**
     * OVL-UX3: "Liderliği devret" leaves the player a moderator (the crew has roles and fewer than 4 others moderate;
     * the launcher's {@code afterHandingOn}), else a member: its question says which.
     */
    private boolean leadKeepsModerator;
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
    private float panelLeft;
    private long shownAt;
    private int lastMouseX = Integer.MIN_VALUE;
    private int lastMouseY = Integer.MIN_VALUE;
    /**
     * {@link #lastMouseX} of rows the keys opened: the first frame takes the pointer where it rests, so a resting
     * pointer never takes the lit row from the keys (Shift+F10 and Enter pick the first row); only a move lights one.
     */
    private static final int REST = Integer.MAX_VALUE;

    public PersonMenu(Actions actions) {
        this.actions = actions;
    }

    public void setReducedMotion(boolean value) {
        reducedMotion = value;
    }

    /** OVL-UX3: what "Liderliği devret"'s question says the player becomes (set before the menu opens). */
    public void setLeadKeepsModerator(boolean value) {
        leadKeepsModerator = value;
    }

    /** Whether it is open (not fading out). */
    public boolean isOpen() {
        return friendId != null && alpha.target() > 0f;
    }

    /** The person whose menu is open, or null. */
    public String friend() {
        return isOpen() ? friendId : null;
    }

    /** Whether its question shows now. */
    public boolean asking() {
        return isOpen() && asking != null;
    }

    /** The row whose list shows now (OVL-UX2), or null. */
    public FriendMenu.Action listing() {
        return isOpen() ? listOf : null;
    }

    /** Whether {@code (mx, my)} is on it. */
    public boolean contains(double mx, double my) {
        return isOpen() && mx >= x && mx < x + width && my >= y && my < y + height;
    }

    /** Opens {@code rows} for a friend; {@code keys}: from the keyboard (its first row lit). */
    public void open(String friend, String name, List<FriendMenu.Row> rows, Anchor at, boolean fromKeys, int screenW,
        int screenH, long now) {
        open(friend, name, rows, action -> List.of(), at, fromKeys, screenW, screenH, now);
    }

    /**
     * Opens {@code rows} for a person; {@code lists} gives the choices of a row that opens a list (OVL-UX2);
     * {@code keys}: from the keyboard (its first row lit).
     */
    public void open(String friend, String name, List<FriendMenu.Row> rows,
        Function<FriendMenu.Action, List<Choice>> lists, Anchor at, boolean fromKeys, int screenW, int screenH,
        long now) {
        friendId = friend;
        friendName = name == null ? "" : name;
        anchor = at;
        keys = fromKeys;
        asking = null;
        this.rows = rows;
        this.choices = lists == null ? action -> List.of() : lists;
        showRows(-1, screenW, screenH);
        focus = fromKeys ? next(-1, 1) : -1;
        lastMouseX = fromKeys ? REST : Integer.MIN_VALUE;
        appear(now);
        int count = 0;
        for (Line line : lines) {
            if (line != null) {
                count++;
            }
        }
        Component said = Texts.tr("people.menu.narration", friendName, count);
        if (focus >= 0) {
            said = Component.empty().append(said).append(". ").append(Component.literal(lines.get(focus).words()));
        }
        Minecraft.getInstance().getNarrator().saySystemQueued(said);
    }

    /** The menu's own rows again (from a list: {@code from} is lit when the keys came back). */
    private void showRows(int from, int screenW, int screenH) {
        listOf = null;
        naming = null;
        Font ui = Fonts.ui();
        lines.clear();
        float widest = 0f;
        boolean marks = false;
        for (FriendMenu.Row row : rows) {
            if (row == null) {
                lines.add(null);
                continue;
            }
            String words = Texts.str(labelKey(row));
            float w = ui.width(words);
            widest = Math.max(widest, w);
            boolean checked = checked(row);
            marks |= checked || row.sub();
            lines.add(new Line(row, false, null, Texts.visual(words), w, icon(row), checked));
        }
        size(widest, marks, screenW, screenH);
        focus = from;
    }

    /** {@code action}'s list in place of the rows (OVL-UX2), its checked choice lit for the keys. */
    private void showList(FriendMenu.Action action, int screenW, int screenH, long now) {
        List<Choice> list = choices.apply(action);
        if (list == null || list.isEmpty()) {
            return;
        }
        listOf = action;
        Font ui = Fonts.ui();
        lines.clear();
        FriendMenu.Row parent = null;
        for (FriendMenu.Row row : rows) {
            if (row != null && row.action() == action) {
                parent = row;
            }
        }
        String backWords = parent == null ? "" : Texts.str(labelKey(parent));
        float widest = ui.width(backWords);
        lines.add(new Line(parent, true, null, Texts.visual(backWords), widest, Icon.BACK, false));
        lines.add(null);
        int lit = -1;
        naming = null;
        for (Choice choice : list) {
            float w = ui.width(choice.label());
            widest = Math.max(widest, w);
            if (lit < 0 && choice.checked() && !choice.disabled()) {
                lit = lines.size();
            }
            // "Yeni kategori…" after a hairline, with the launcher's sign (OVL-UX3).
            boolean fresh = NEW_CHOICE.equals(choice.id());
            if (fresh) {
                lines.add(null);
            }
            lines.add(new Line(null, false, choice, Texts.visual(choice.label()), w, fresh ? Icon.FOLDER_PLUS : null,
                choice.checked()));
        }
        size(widest, true, screenW, screenH);
        boolean byKeys = keys || PanelInput.keyboardMode();
        focus = byKeys ? (lit >= 0 ? lit : next(1, 1)) : -1;
        lastMouseX = byKeys ? REST : Integer.MIN_VALUE;
        appear(now);
        Minecraft.getInstance().getNarrator().saySystemQueued(Texts.tr("people.list.narration", backWords,
            list.size()));
    }

    private void size(float widest, boolean marks, int screenW, int screenH) {
        width = Math.max(MIN_W, (float) Math.ceil(TEXT_X + widest + 10 + (marks ? MARK_W : 0)));
        width = Math.min(width, screenW - 2 * EDGE);
        height = PAD * 2;
        for (Line line : lines) {
            height += line == null ? SEP_H : ROW_H;
        }
        place(screenW, screenH);
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
        listOf = null;
        naming = null;
    }

    /** Where the panel starts (GUI pixels): a menu under a box keeps to its side of it. */
    public void setPanelLeft(float left) {
        panelLeft = left;
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
            // Under the "…", its right edge on the button's; under a box near the panel's left edge (a chat chip from
            // the keys) its left edge on the box's, so it stays over the panel instead of the game.
            x = anchor.x() + anchor.w() - width;
            if (x < panelLeft + EDGE && anchor.x() + width <= screenW - EDGE) {
                x = anchor.x();
            }
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
        if (row == null) {
            return "people.cancel";
        }
        return switch (row.action()) {
            case JOIN -> "people.join";
            case INVITE -> row.done() ? "people.invited" : "people.invite";
            case WORLD_INVITE -> row.done() ? "people.world_invited" : "people.world_invite";
            case MESSAGE -> "people.message";
            case DISCORD -> "people.discord";
            case ADD_FRIEND -> row.accept() ? "people.accept_friend" : row.done() ? "people.request_sent"
                : "people.add_friend";
            case FAVORITE -> row.on() ? "people.unfavorite" : "people.favorite";
            case ALERT -> "people.alert";
            case MOVE -> "people.move";
            case CREW_INVITE -> row.done() ? "people.crew_invited" : "people.crew_invite";
            case MAKE_MODERATOR -> "people.make_moderator";
            case UNMAKE_MODERATOR -> "people.unmake_moderator";
            case MAKE_FOUNDER -> "people.make_founder";
            case REMOVE_MEMBER -> "people.remove_member";
            case REMOVE_FROM_WORLD -> "people.remove_from_world";
            case UNFRIEND -> "people.unfriend";
            case BLOCK -> "people.block";
            case REPORT -> "people.report";
        };
    }

    private static Icon icon(FriendMenu.Row row) {
        return switch (row.action()) {
            case JOIN -> Icon.JOIN;
            case INVITE -> Icon.PLAY_INVITE;
            case WORLD_INVITE -> Icon.WORLD_INVITE;
            case MESSAGE -> Icon.MESSAGE;
            case DISCORD -> Icon.DISCORD;
            case ADD_FRIEND -> row.accept() ? Icon.USER_CHECK : Icon.USER_PLUS;
            case FAVORITE -> row.on() ? Icon.STAR_FILL : Icon.STAR;
            case ALERT -> row.on() ? Icon.BELL_FILL : Icon.BELL;
            case MOVE -> Icon.FOLDER;
            case CREW_INVITE -> Icon.CREW;
            case MAKE_MODERATOR, UNMAKE_MODERATOR -> Icon.SHIELD_STAR;
            case MAKE_FOUNDER -> Icon.CROWN;
            case REMOVE_MEMBER -> Icon.USER_MINUS;
            case REMOVE_FROM_WORLD -> Icon.SEND_OFF;
            case UNFRIEND -> Icon.UNFRIEND;
            case BLOCK -> Icon.BLOCK;
            case REPORT -> Icon.REPORT;
        };
    }

    /** The launcher's check: a set online notice, a world invite that went, a friend request that went. */
    private static boolean checked(FriendMenu.Row row) {
        return switch (row.action()) {
            case ALERT -> row.on();
            case WORLD_INVITE, ADD_FRIEND -> row.done();
            default -> false;
        };
    }

    /**
     * The next line that can be picked after {@code from} in {@code step}'s direction (from the top or the bottom when
     * {@code from} is -1), wrapping; -1 for none.
     */
    private int next(int from, int step) {
        int n = lines.size();
        int at = from >= 0 ? from : step > 0 ? -1 : n;
        for (int i = 0; i < n; i++) {
            at = Math.floorMod(at + step, n);
            Line line = lines.get(at);
            if (line != null && !line.disabled()) {
                return at;
            }
        }
        return -1;
    }

    /** The line of a menu row's action, or -1. */
    private int indexOf(FriendMenu.Action action) {
        for (int i = 0; i < lines.size(); i++) {
            Line line = lines.get(i);
            if (line != null && !line.back() && line.row() != null && line.row().action() == action) {
                return i;
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
        // "Dünyadan çıkar" asks in world sharing's own words (its page asks the same), the crew rows in the launcher's
        // (OVL-UX3: who leads after the hand-over and what the player becomes, who leaves and that the code changes).
        String titleKey = switch (action) {
            case BLOCK -> "people.block.ask";
            case REMOVE_FROM_WORLD -> "host.remove.ask";
            case MAKE_FOUNDER -> leadKeepsModerator ? "people.make_founder.ask_moderator" : "people.make_founder.ask";
            case REMOVE_MEMBER -> "people.remove_member.ask";
            default -> "people.unfriend.ask";
        };
        String bodyKey = switch (action) {
            case BLOCK -> "people.block.body";
            case REMOVE_FROM_WORLD -> "host.remove.body";
            default -> null;
        };
        confirmLabel = Texts.str(switch (action) {
            case BLOCK -> "people.block";
            case REMOVE_FROM_WORLD -> "people.remove_from_world";
            case MAKE_FOUNDER -> "people.make_founder";
            case REMOVE_MEMBER -> "people.remove_member";
            default -> "people.unfriend";
        });
        cancelW = ui.width(cancelLabel) + BUTTON_PAD * 2;
        confirmW = ui.width(confirmLabel) + BUTTON_PAD * 2;
        float buttons = cancelW + BUTTON_GAP + confirmW;
        float maxW = Math.min(ASK_MAX_W, screenW - 2 * EDGE);
        width = Math.min(maxW, Math.max(ASK_MIN_W, buttons + ASK_PAD * 2));
        int textW = Math.round(width - ASK_PAD * 2);
        String title = Texts.str(titleKey, friendName.isEmpty() ? Texts.str("toast.someone") : friendName);
        question.clear();
        question.addAll(Texts.wrap(strong, title, textW, 2));
        body.clear();
        if (bodyKey != null) {
            body.addAll(Texts.wrap(small, Texts.str(bodyKey), textW, 3));
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
        if (bodyKey != null) {
            said += ". " + Texts.str(bodyKey);
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

    /** The line under the pointer, or -1. */
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
        if (at >= 0 && !lines.get(at).disabled()) {
            pick(at, screenW, screenH, now);
        }
    }

    private void pick(int index, int screenW, int screenH, long now) {
        Line line = lines.get(index);
        if (line.back()) {
            if (naming != null && listOf != null) {
                // From the name field back to the list it came from.
                showList(listOf, screenW, screenH, now);
            } else {
                back(screenW, screenH, now);
            }
            return;
        }
        if (line.choice() != null && NEW_CHOICE.equals(line.choice().id())) {
            if (naming == null) {
                startNaming(line, screenW, screenH, now);
            } else {
                submitName(now);
            }
            return;
        }
        if (line.choice() != null) {
            String friend = friendId;
            FriendMenu.Action action = listOf;
            close(now);
            if (action != null) {
                actions.chose(friend, action, line.choice().id());
            }
            return;
        }
        FriendMenu.Action action = line.row().action();
        if (line.row().sub()) {
            showList(action, screenW, screenH, now);
            return;
        }
        if (action.asks()) {
            ask(action, screenW, screenH, now);
            return;
        }
        String friend = friendId;
        close(now);
        actions.picked(friend, action);
    }

    /**
     * "Yeni kategori…" picked (OVL-UX3): the list turns into its way back and a one-line name field in place, which
     * the keys type into at once (Enter makes the category and moves the friend, Esc goes back to the list).
     */
    private void startNaming(Line fresh, int screenW, int screenH, long now) {
        naming = new StringBuilder();
        Line back = lines.isEmpty() ? null : lines.get(0);
        lines.clear();
        float widest = Fonts.ui().width(Texts.str("people.move.name"));
        if (back != null) {
            lines.add(back);
            lines.add(null);
            widest = Math.max(widest, back.width());
        }
        lines.add(new Line(null, false, fresh.choice(), Texts.visual(""), widest, null, false));
        size(widest, true, screenW, screenH);
        focus = lines.size() - 1;
        lastMouseX = REST;
        appear(now);
        Minecraft.getInstance().getNarrator().saySystemQueued(Texts.tr("people.move.name"));
    }

    /** Enter in the name field: the category goes with the friend in it; nothing while the name is empty. */
    private void submitName(long now) {
        String name = naming == null ? "" : FriendMenu.cleanCategoryName(naming.toString());
        if (name.isEmpty()) {
            return;
        }
        String friend = friendId;
        close(now);
        actions.named(friend, name);
    }

    /** A typed character for the name field (OVL-UX3); whether the menu took it (it takes every one while open). */
    public boolean charTyped(int codepoint) {
        if (!isOpen() || naming == null || asking != null) {
            return isOpen();
        }
        PanelInput.usedKeyboard();
        if (Character.isISOControl(codepoint) || codepoint == 0xA7) {
            // No control characters and no legacy formatting sign in a name.
            return true;
        }
        if (naming.codePointCount(0, naming.length()) < NAME_MAX) {
            naming.appendCodePoint(codepoint);
        }
        return true;
    }

    /** Types {@code name} into the open name field as the keys would (the scenario driver); false without one. */
    public boolean typeForDriver(String name) {
        if (naming == null || !isOpen()) {
            return false;
        }
        name.codePoints().forEach(this::charTyped);
        return true;
    }

    /** Whether "Yeni kategori…"'s name field shows (the scenario driver checks it). */
    public boolean naming() {
        return naming != null && isOpen();
    }

    /** From a list back to the menu, its row lit for the keys. */
    private void back(int screenW, int screenH, long now) {
        naming = null;
        FriendMenu.Action was = listOf;
        showRows(-1, screenW, screenH);
        boolean byKeys = keys || PanelInput.keyboardMode();
        focus = was != null && byKeys ? indexOf(was) : -1;
        lastMouseX = byKeys ? REST : Integer.MIN_VALUE;
        appear(now);
    }

    /** Presses the row of {@code action} as a click would (the scenario driver); false when it is not there or soft. */
    public boolean press(FriendMenu.Action action, int screenW, int screenH, long now) {
        if (!isOpen() || asking != null || listOf != null) {
            return false;
        }
        for (int i = 0; i < lines.size(); i++) {
            Line line = lines.get(i);
            if (line != null && line.row() != null && line.row().action() == action && !line.disabled()) {
                focus = i;
                pick(i, screenW, screenH, now);
                return true;
            }
        }
        return false;
    }

    /** Picks the choice {@code id} of the list showing now as a click would (the scenario driver); false without it. */
    public boolean choose(String id, int screenW, int screenH, long now) {
        if (!isOpen() || listOf == null) {
            return false;
        }
        for (int i = 0; i < lines.size(); i++) {
            Line line = lines.get(i);
            if (line != null && line.choice() != null && line.choice().id().equals(id) && !line.disabled()) {
                focus = i;
                pick(i, screenW, screenH, now);
                return true;
            }
        }
        return false;
    }

    /** Whether the open menu has the row of {@code action} (the scenario driver checks it). */
    public boolean hasRow(FriendMenu.Action action) {
        return isOpen() && indexOf(action) >= 0;
    }

    /** Lights the row of {@code action} as the arrow keys would (the scenario driver); false when it is not there. */
    public boolean focusForDriver(FriendMenu.Action action) {
        int at = isOpen() && asking == null && listOf == null ? indexOf(action) : -1;
        if (at < 0 || lines.get(at).disabled()) {
            return false;
        }
        focus = at;
        return true;
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
            if (naming != null && listOf != null && asking == null) {
                // Esc in "Yeni kategori…"'s field goes back to the list, as the launcher's field does.
                showList(listOf, screenW, screenH, now);
            } else {
                close(now);
            }
            return true;
        }
        PanelInput.usedKeyboard();
        if (naming != null && asking == null) {
            if (event.key() == InputConstants.KEY_BACKSPACE) {
                if (!naming.isEmpty()) {
                    naming.setLength(naming.offsetByCodePoints(naming.length(), -1));
                }
                return true;
            }
            if (event.isUp() || event.isDown()) {
                focus = next(focus, event.isUp() ? -1 : 1);
                sayFocus();
                return true;
            }
            if (event.isSelection() || event.isConfirmation()) {
                // Enter on the field makes the category; on the way back, the list again. (Space types.)
                if (focus >= 0 && lines.get(focus) != null && lines.get(focus).back()) {
                    pick(focus, screenW, screenH, now);
                } else if (event.key() != InputConstants.KEY_SPACE) {
                    submitName(now);
                }
                return true;
            }
            if (event.isCycleFocus()) {
                close(now);
                return false;
            }
            // Letters come as typed characters; nothing else reaches the panel under the field.
            return true;
        }
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
        if (event.isLeft() && listOf != null) {
            // As a submenu: the way back.
            back(screenW, screenH, now);
            return true;
        }
        if (event.isRight() && listOf == null && focus >= 0 && lines.get(focus).row() != null
            && lines.get(focus).row().sub()) {
            pick(focus, screenW, screenH, now);
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
            Minecraft.getInstance().getNarrator().saySystemQueued(Component.literal(lines.get(focus).words()));
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
                listOf = null;
                naming = null;
            }
            return;
        }
        if (lastMouseX == REST) {
            lastMouseX = mouseX;
            lastMouseY = mouseY;
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
                if (at >= 0 && !lines.get(at).disabled()) {
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
            if (naming != null && line.choice() != null && NEW_CHOICE.equals(line.choice().id())) {
                drawNameField(g, ui, ry, i == focus);
                ry += ROW_H;
                continue;
            }
            boolean soft = line.disabled();
            boolean lit = i == focus && !soft;
            if (lit) {
                Draw.frame(g, Frames.ROW_HOVER, x + PAD, ry, width - PAD * 2, ROW_H);
            }
            boolean danger = line.danger();
            int ink = danger ? Theme.FLARE_300 : Theme.BONE_100;
            int iconInk = danger ? Theme.FLARE_300 : lit ? Theme.BONE_100 : Theme.ASH_300;
            if (soft) {
                Draw.pushAlpha(0.45f);
            }
            if (line.icon() != null) {
                Draw.icon(g, line.icon(), 10, x + ICON_X, ry + (ROW_H - 10) / 2f, iconInk);
            }
            Draw.text(g, ui, line.label(), x + TEXT_X, Fonts.centeredY(Fonts.Face.UI, ry + ROW_H / 2f), ink);
            float markX = x + width - PAD - 4 - 8;
            if (line.checked()) {
                Draw.icon(g, Icon.CHECK, 8, markX, ry + (ROW_H - 8) / 2f, Theme.GOLD_400);
            } else if (line.row() != null && !line.back() && line.row().sub()) {
                Draw.icon(g, Icon.NEXT, 8, markX, ry + (ROW_H - 8) / 2f, Theme.ASH_400);
            }
            if (soft) {
                Draw.popAlpha();
            }
            ry += ROW_H;
        }
    }

    /**
     * "Yeni kategori…"'s field (OVL-UX3): the typed name (its end kept in view) with a steady caret, or the quiet
     * "Kategori adı" while it is empty; the gold ring while the keys are on it.
     */
    private void drawNameField(GuiGraphicsExtractor g, Font ui, float ry, boolean lit) {
        float fx = x + PAD + 1;
        float fw = width - PAD * 2 - 2;
        Draw.frame(g, lit ? Frames.FIELD_FOCUS : Frames.FIELD, fx, ry, fw, ROW_H);
        float tx = fx + 5;
        float room = fw - 12;
        float mid = Fonts.centeredY(Fonts.Face.UI, ry + ROW_H / 2f);
        String typed = naming == null ? "" : naming.toString();
        if (typed.isEmpty()) {
            Draw.text(g, ui, Texts.visual(Texts.str("people.move.name")), tx, mid, Theme.ASH_500);
            if (lit) {
                Draw.fill(g, tx, ry + 3, tx + 1, ry + ROW_H - 3, Theme.BONE_100);
            }
            return;
        }
        // The end of a long name stays in view: its start goes first.
        String shown = typed;
        while (ui.width(shown) > room && shown.length() > 1) {
            shown = shown.substring(shown.offsetByCodePoints(0, 1));
        }
        Draw.text(g, ui, Texts.visual(shown), tx, mid, Theme.BONE_50);
        if (lit) {
            float cx = tx + ui.width(shown) + 1;
            Draw.fill(g, cx, ry + 3, cx + 1, ry + ROW_H - 3, Theme.BONE_100);
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
        if (asking == FriendMenu.Action.MAKE_FOUNDER) {
            // Handing the lead on loses nothing (the launcher's primary button there).
            drawButton(g, confirmX(), by, confirmW, confirmLabel, Frames.BUTTON_PRIMARY, Frames.BUTTON_PRIMARY_HOVER,
                confirmHover.value(now), Theme.GOLD_INK, button == 1 && PanelInput.keyboardMode());
        } else {
            drawButton(g, confirmX(), by, confirmW, confirmLabel, Frames.BUTTON_DANGER, Frames.BUTTON_DANGER_HOVER,
                confirmHover.value(now), Theme.mix(Theme.FLARE_300, Theme.BONE_50, confirmHover.value(now) * 0.35f),
                button == 1 && PanelInput.keyboardMode());
        }
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
