package gg.projects.overlay.ui.panel;

import com.mojang.blaze3d.platform.cursor.CursorTypes;
import gg.projects.overlay.state.Presence;
import gg.projects.overlay.ui.PresenceDot;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarratedElementType;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;

/**
 * The status switcher in the panel's header (OVL-SOC 2; the launcher's own card, {@code status-menu.tsx}): the player's
 * presence dot before their tag and name (bright jade "Çevrimiçi", since they play now; gold with its bar "Rahatsız
 * etmeyin"; ash "Görünmez"), the three a press away (a click, Enter or Space) in the person menu's popover. The name and
 * the tag are drawn by the header over it; this is the dot, the quiet plate under the pointer and the ring for the keys.
 * It never moves and nothing in it runs while nothing changes.
 */
public final class StatusButton extends PanelWidget {
    public static final int HEIGHT = 14;
    /** The dot's room before the tag and the name. */
    public static final int DOT_ROOM = 9;

    private final Runnable onPress;
    private String presence = "online";
    private Component spoken = Component.empty();

    public StatusButton(Runnable onPress) {
        super(0, 0, 40, HEIGHT, Texts.tr("status.title"));
        this.onPress = onPress;
    }

    /** What it shows: the status ({@code online}, {@code dnd}, {@code invisible}) and what it says ("Rahatsız etmeyin,
     * 17:30'a kadar"). */
    public void show(String status, String words) {
        presence = status;
        spoken = Texts.tr("status.narration", words);
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        pressed();
        onPress.run();
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (active && visible && (event.isSelection() || event.isConfirmation())) {
            PanelInput.usedKeyboard();
            pressed();
            onPress.run();
            return true;
        }
        return false;
    }

    /** The dot's middle (the menu opens under the button; the scenario driver points at it). */
    public float dotX() {
        return getX() + 3 + PresenceDot.DOT / 2f;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float h = hover.value(now);
        float x = getX();
        float y = getY();
        if (h > 0f) {
            Draw.pushAlpha(h);
            Draw.frame(g, Frames.CHIP_HOVER, x, y, getWidth(), HEIGHT);
            Draw.popAlpha();
        }
        if (isHovered()) {
            g.requestCursor(CursorTypes.POINTING_HAND);
        }
        if (keyboardFocus()) {
            Draw.frame(g, Frames.FOCUS_14, x, y, getWidth(), HEIGHT);
        }
        float cx = dotX();
        float cy = y + HEIGHT / 2f;
        switch (presence) {
            case "dnd" -> PresenceDot.dot(g, Presence.DND, cx, cy);
            // The launcher's ash dot: nobody sees the player online.
            case "invisible" -> Draw.disc(g, PresenceDot.DOT, cx, cy, Theme.ASH_500);
            default -> PresenceDot.dot(g, Presence.PLAYING, cx, cy);
        }
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        output.add(NarratedElementType.TITLE, getMessage());
        output.add(NarratedElementType.HINT, spoken);
    }
}
