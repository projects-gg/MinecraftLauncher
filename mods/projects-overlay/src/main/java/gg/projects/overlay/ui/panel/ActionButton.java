package gg.projects.overlay.ui.panel;

import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;

/**
 * The launcher's chamfered action button, 16 GUI pixels tall: gold (primary), obsidian (secondary), bare until
 * hovered (ghost) or obsidian under a flare hairline for what ends something (danger); an optional icon before the
 * words. Hover crossfades to the lighter face, a press settles it a pixel down.
 */
public class ActionButton extends PanelWidget {
    public enum Style { PRIMARY, SECONDARY, GHOST, DANGER }

    public static final int HEIGHT = 16;
    static final int PAD = 7;
    static final int ICON_GAP = 4;

    private Style style;
    private final Runnable onPress;
    private Icon icon;
    private String label;
    private int labelWidth;
    private boolean selected;

    public ActionButton(Style style, Component label, Icon icon, Runnable onPress) {
        super(0, 0, 10, HEIGHT, label);
        this.style = style;
        this.onPress = onPress;
        this.icon = icon;
        setLabel(label);
    }

    /** Width that fits the words and the icon. */
    public int preferredWidth() {
        return PAD * 2 + labelWidth + (icon == null ? 0 : 10 + (label.isEmpty() ? 0 : ICON_GAP));
    }

    public void setLabel(Component text) {
        setMessage(text);
        label = text.getString();
        labelWidth = Fonts.ui().width(label);
    }

    public void setStyle(Style next) {
        style = next;
    }

    public void setIcon(Icon next) {
        icon = next;
    }

    /** Chosen (a crew call's own answer): drawn as primary and narrated as selected. */
    public void setSelected(boolean value) {
        selected = value;
    }

    public boolean selected() {
        return selected;
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        PanelInput.usedMouse();
        activate();
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (!active || !visible) {
            return false;
        }
        if (event.isSelection() || event.isConfirmation()) {
            PanelInput.usedKeyboard();
            activate();
            return true;
        }
        return false;
    }

    private void activate() {
        pressed();
        onPress.run();
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float x = getX();
        float y = getY();
        float h = hover.value(now);
        float p = press.value(now);
        Style shown = selected ? Style.PRIMARY : style;
        switch (shown) {
            case PRIMARY -> {
                Draw.frame(g, Frames.BUTTON_PRIMARY, x, y, getWidth(), HEIGHT);
                if (h > 0f) {
                    Draw.pushAlpha(h);
                    Draw.frame(g, Frames.BUTTON_PRIMARY_HOVER, x, y, getWidth(), HEIGHT);
                    Draw.popAlpha();
                }
            }
            case SECONDARY -> {
                Draw.frame(g, Frames.BUTTON_SECONDARY, x, y, getWidth(), HEIGHT);
                if (h > 0f) {
                    Draw.pushAlpha(h);
                    Draw.frame(g, Frames.BUTTON_SECONDARY_HOVER, x, y, getWidth(), HEIGHT);
                    Draw.popAlpha();
                }
            }
            case GHOST -> {
                if (h > 0f) {
                    Draw.pushAlpha(h);
                    Draw.frame(g, Frames.BUTTON_GHOST_HOVER, x, y, getWidth(), HEIGHT);
                    Draw.popAlpha();
                }
            }
            case DANGER -> {
                Draw.frame(g, Frames.BUTTON_DANGER, x, y, getWidth(), HEIGHT);
                if (h > 0f) {
                    Draw.pushAlpha(h);
                    Draw.frame(g, Frames.BUTTON_DANGER_HOVER, x, y, getWidth(), HEIGHT);
                    Draw.popAlpha();
                }
            }
        }
        if (p > 0f) {
            Draw.pushAlpha(p * 0.5f);
            Draw.frame(g, shown == Style.PRIMARY ? Frames.BUTTON_SECONDARY : Frames.BUTTON_GHOST_HOVER, x, y,
                getWidth(), HEIGHT, 0x66000000);
            Draw.popAlpha();
        }
        if (keyboardFocus()) {
            Draw.frame(g, Frames.FOCUS_16, x, y, getWidth(), HEIGHT);
        }
        int ink = switch (shown) {
            case PRIMARY -> Theme.GOLD_INK;
            case SECONDARY -> Theme.BONE_100;
            case GHOST -> Theme.mix(Theme.ASH_300, Theme.BONE_100, h);
            case DANGER -> Theme.mix(Theme.FLARE_300, Theme.BONE_50, h * 0.35f);
        };
        float content = (icon == null ? 0 : 10 + (label.isEmpty() ? 0 : ICON_GAP)) + labelWidth;
        float cx = x + (getWidth() - content) / 2f;
        float dy = p * 0.5f;
        if (icon != null) {
            Draw.icon(g, icon, 10, cx, y + (HEIGHT - 10) / 2f + dy, ink);
            cx += 10 + ICON_GAP;
        }
        if (!label.isEmpty()) {
            Font font = Fonts.ui();
            Draw.text(g, font, label, cx, Fonts.centeredY(Fonts.Face.UI, y + HEIGHT / 2f) + dy, ink);
        }
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        defaultButtonNarrationText(output);
    }
}
