package gg.projects.overlay.ui.panel;

import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.motion.Tween;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;
import java.util.function.Supplier;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;

/**
 * Crew chat's quick replies, the launcher's four ("Geliyorum", "5 dk", "Neredesin?", "Gel"): one press sends. They fade
 * away while the player writes something of their own. The row shows as many as fit the panel whole (English ones are
 * longer; a narrow panel at GUI 3 leaves the last out), measured in the font the game draws now (GUI 1 draws its own,
 * wider one).
 *
 * <p>CREW-PLUS: "Konumum" at the row's end, apart from the replies (so a narrow panel drops a reply, never it), sends
 * where the player stands ({@code MyLocation}): in crew chat and, alone on its row, in a direct conversation; only in a
 * world whose coordinates the game shows (never under reduced debug info).
 */
public final class QuickReplies extends PanelWidget {
    public static final int HEIGHT = 14;
    private static final String[] KEYS = {"chat.quick.coming", "chat.quick.five", "chat.quick.where", "chat.quick.come"};
    /** Between two chips, and between the replies and "Konumum". */
    private static final int GAP = 4;

    private final Consumer<String> onSend;
    private final BooleanSupplier locationShown;
    private final Supplier<String> location;
    private final String[] labels = new String[KEYS.length];
    private final float[] widths = new float[KEYS.length];
    private final String locationLabel;
    private final Tween shown = new Tween(1f);
    /** The replies' index, or {@link #fitting()} for "Konumum". */
    private int selected;
    private boolean hidden;
    /** Crew chat: the replies show; a direct conversation has "Konumum" alone. */
    private boolean crew = true;

    /**
     * {@code locationShown}: "Konumum" may show now (in a world whose coordinates the game shows); {@code location}: its
     * line at the press, or null when it may not be told any more.
     */
    public QuickReplies(Consumer<String> onSend, BooleanSupplier locationShown, Supplier<String> location) {
        super(0, 0, 100, HEIGHT, Texts.tr("chat.quick"));
        this.onSend = onSend;
        this.locationShown = locationShown;
        this.location = location;
        for (int i = 0; i < KEYS.length; i++) {
            labels[i] = Texts.str(KEYS[i]);
        }
        locationLabel = Texts.str("chat.quick.location");
    }

    /** The conversation the row is under: crew chat shows the replies, a direct one only "Konumum". */
    public void setCrew(boolean value) {
        crew = value;
    }

    /** Whether the row has anything to show in this conversation now (a direct one: only "Konumum"). */
    public boolean hasChips() {
        return crew || locationShown.getAsBoolean();
    }

    private boolean withLocation() {
        return locationShown.getAsBoolean();
    }

    /** "Konumum"'s width in the font drawn now. */
    private float locationWidth() {
        return Fonts.small().width(locationLabel) + 12;
    }

    /** How many replies fit the row whole beside "Konumum", their widths measured in the font drawn now. */
    private int fitting() {
        if (!crew) {
            selected = 0;
            return 0;
        }
        Font small = Fonts.small();
        float room = getWidth() - (withLocation() ? locationWidth() + GAP : 0);
        float x = 0f;
        int n = 0;
        for (int i = 0; i < labels.length; i++) {
            widths[i] = small.width(labels[i]) + 12;
            if (x + widths[i] > room) {
                break;
            }
            x += widths[i] + GAP;
            n++;
        }
        int last = withLocation() ? n : n - 1;
        selected = Math.max(0, Math.min(selected, last));
        return n;
    }

    /** Hidden while the field has text: still laid out (nothing moves), just not there. */
    public void setHidden(boolean value, long now) {
        if (value != hidden) {
            hidden = value;
            shown.animateTo(value ? 0f : 1f, now, value ? 110 : 160, Ease.OUT);
        }
        active = !value;
    }

    /** "Konumum"'s left edge: the row's end. */
    private float locationX() {
        return getX() + getWidth() - locationWidth();
    }

    /** The chip under {@code mouseX}: a reply's index, the replies' count for "Konumum", -1 none. */
    private int at(double mouseX) {
        int n = fitting();
        float x = getX();
        for (int i = 0; i < n; i++) {
            if (mouseX >= x && mouseX < x + widths[i]) {
                return i;
            }
            x += widths[i] + GAP;
        }
        if (withLocation() && mouseX >= locationX() && mouseX < getX() + getWidth()) {
            return n;
        }
        return -1;
    }

    /** Sends chip {@code i} (a reply, or "Konumum" where the player stands now). */
    private void press(int i, int n) {
        if (i < n) {
            pressed();
            onSend.accept(labels[i]);
            return;
        }
        String line = location.get();
        if (line != null) {
            pressed();
            onSend.accept(line);
        }
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        int n = fitting();
        int i = at(event.x());
        if (i >= 0) {
            selected = i;
            press(i, n);
        }
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        int n = fitting();
        int last = withLocation() ? n : n - 1;
        if (event.isLeft() && selected > 0) {
            PanelInput.usedKeyboard();
            selected--;
            return true;
        }
        if (event.isRight() && selected < last) {
            PanelInput.usedKeyboard();
            selected++;
            return true;
        }
        if ((event.isSelection() || event.isConfirmation()) && last >= 0) {
            PanelInput.usedKeyboard();
            press(selected, n);
            return true;
        }
        return false;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float a = shown.value(now);
        if (a <= 0.01f) {
            return;
        }
        Draw.pushAlpha(a);
        int n = fitting();
        Font small = Fonts.small();
        float x = getX();
        float y = getY();
        for (int i = 0; i < n; i++) {
            chip(g, labels[i], x, y, widths[i], i, mouseX, small);
            x += widths[i] + GAP;
        }
        if (withLocation()) {
            chip(g, locationLabel, locationX(), y, locationWidth(), n, mouseX, small);
        }
        Draw.popAlpha();
    }

    private void chip(GuiGraphicsExtractor g, String label, float x, float y, float w, int index, int mouseX,
        Font small) {
        boolean lit = isHovered() && mouseX >= x && mouseX < x + w;
        Draw.frame(g, lit ? Frames.CHIP_HOVER : Frames.CHIP, x, y, w, HEIGHT);
        if (keyboardFocus() && index == selected) {
            Draw.frame(g, Frames.FOCUS_14, x, y, w, HEIGHT);
        }
        Draw.text(g, small, label, x + 6, Fonts.centeredY(Fonts.Face.SMALL, y + HEIGHT / 2f),
            lit ? Theme.BONE_50 : Theme.ASH_300);
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        int n = fitting();
        String label = selected < n ? labels[selected] : locationLabel;
        output.add(NarratedElementType.TITLE, Texts.tr("narration.quick", label));
    }
}
