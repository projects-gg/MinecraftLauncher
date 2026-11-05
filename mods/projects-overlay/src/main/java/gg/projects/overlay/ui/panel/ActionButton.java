package gg.projects.overlay.ui.panel;

import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.LabelFit;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import gg.projects.overlay.ui.draw.Icon;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;

/**
 * The launcher's chamfered action button, 16 GUI pixels tall: gold (primary), obsidian (secondary), bare until
 * hovered (ghost) or obsidian under a flare hairline for what ends something (danger); an optional icon before the
 * words. Hover crossfades to the lighter face, a press settles it a pixel down. Words wider than the button its layout
 * gave it get smaller, then lose the icon, then are cut with the whole label as the tooltip ({@link LabelFit}): they
 * never cross its edge.
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
    private boolean selected;
    private boolean chosen;
    /** The icon's own colour (a voice ring's gold phone, a leave's flare one; VOICE-O), 0 for the words' colour. */
    private int iconInk;
    /** How the label fits the button now, and for what it was worked out (width, GUI scale and font, the label). */
    private LabelFit.Fit fit;
    private int fitWidth = -1;
    private int fitScale = -1;
    private String fitLabel;
    private Icon fitIcon;
    private Fonts.Face fitLargest;
    /** The largest size its words may take: a row of buttons gives them all the size its narrowest one needed. */
    private Fonts.Face largest = Fonts.Face.UI;

    public ActionButton(Style style, Component label, Icon icon, Runnable onPress) {
        super(0, 0, 10, HEIGHT, label);
        this.style = style;
        this.onPress = onPress;
        this.icon = icon;
        setLabel(label);
    }

    /** Width that fits the words and the icon, in the letters the game draws now. */
    public int preferredWidth() {
        return PAD * 2 + Fonts.ui().width(label) + (icon == null ? 0 : 10 + (label.isEmpty() ? 0 : ICON_GAP));
    }

    public void setLabel(Component text) {
        setMessage(text);
        label = text.getString();
    }

    /** The largest size its words may take (UI, or smaller to match the rest of its row; LABEL-FIT). */
    public void setLargestFace(Fonts.Face face) {
        largest = face;
    }

    /** The size its words take at its width now. */
    public Fonts.Face fitFace() {
        return fit().face();
    }

    public void setStyle(Style next) {
        style = next;
    }

    public void setIcon(Icon next) {
        icon = next;
    }

    /** The icon in its own colour (0: the words' colour). */
    public void setIconInk(int argb) {
        iconInk = argb;
    }

    /** Chosen (a crew call's own answer): drawn as primary and narrated as selected. */
    public void setSelected(boolean value) {
        selected = value;
    }

    /** Chosen among quiet choices (a plan's day, the player's answer; OVL-SOC2): the quiet chosen plate, never gold. */
    public void setChosen(boolean value) {
        chosen = value;
    }

    public boolean selected() {
        return selected || chosen;
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
                Draw.frame(g, chosen ? Frames.BUTTON_CHOSEN : Frames.BUTTON_SECONDARY, x, y, getWidth(), HEIGHT);
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
        LabelFit.Fit f = fit();
        float cx = x + (getWidth() - f.width()) / 2f;
        float dy = p * 0.5f;
        if (icon != null && f.icon()) {
            Draw.icon(g, icon, 10, cx, y + (HEIGHT - 10) / 2f + dy, iconInk != 0 ? iconInk : ink);
            cx += 10 + (label.isEmpty() ? 0 : ICON_GAP);
        }
        if (!f.text().isEmpty()) {
            Font font = Fonts.get(f.face());
            Draw.text(g, font, f.text(), cx, Fonts.centeredY(f.face(), y + HEIGHT / 2f) + dy, ink);
        }
    }

    /**
     * How the icon and words fit inside the padding at this width, GUI scale and font (LABEL-FIT): worked out again only
     * when one of them changes. A label cut to fit is the button's tooltip.
     */
    private LabelFit.Fit fit() {
        int scale = (Draw.scale() * 2 + (Fonts.bundled() ? 1 : 0)) * 1024 + LabelFit.generation() % 1024;
        if (fit == null || fitWidth != getWidth() || fitScale != scale || !label.equals(fitLabel) || fitIcon != icon
            || fitLargest != largest) {
            int iconRoom = icon == null ? 0 : 10 + (label.isEmpty() ? 0 : ICON_GAP);
            fit = LabelFit.fit(label, largest, iconRoom, getWidth() - 2 * PAD, Fonts.bundled(), LabelFit.FONTS);
            fitWidth = getWidth();
            fitScale = scale;
            fitLabel = label;
            fitIcon = icon;
            fitLargest = largest;
            setTooltip(fit.cut() ? Tooltip.create(getMessage()) : null);
            LabelFit.noteForDriver(label, Fonts.Face.UI, icon != null, Draw.scale(), fit);
        }
        return fit;
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        defaultButtonNarrationText(output);
    }
}
