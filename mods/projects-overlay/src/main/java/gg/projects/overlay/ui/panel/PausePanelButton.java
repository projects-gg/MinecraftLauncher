package gg.projects.overlay.ui.panel;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import java.util.List;
import net.fabricmc.fabric.api.client.screen.v1.ScreenEvents;
import net.fabricmc.fabric.api.client.screen.v1.Screens;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;

/**
 * The pause menu's Projects button (OVL-UX; the owner, 24 Oct: "ESC menüsüne de girebilir, sağ alta"): a small square in
 * the bottom right corner with our own sprite (the chat bubble with Küp's eyes, 16 x 16 pixels, so it stays sharp at every
 * GUI scale), opening the panel as the overlay key does; its tooltip names the key ("Projects katmanı (F7)"). Only while
 * the launcher's bridge is connected; nothing otherwise.
 *
 * <p>It never covers another button: its place is chosen at the menu's first frame, once every mod has added its own (Mod
 * Menu, voice chat and replay mods put theirs in the menu's column or a top corner), going up the right edge and then to
 * the left corner if the bottom right is taken, and it stays away when no corner is free. Hover lights its face, a press
 * settles the sprite half a pixel; nothing moves otherwise.
 */
public final class PausePanelButton extends PanelWidget {
    public static final int SIZE = 20;
    private static final int SPRITE = 16;
    private static final int MARGIN = 6;
    /** Rows up the corner it tries before the other corner. */
    private static final int TRIES = 3;
    private static final Identifier SPRITE_TEXTURE = Identifier.fromNamespaceAndPath(OverlayClient.MOD_ID,
        "textures/gui/overlay_button.png");

    /** The last one added (the scenario driver looks at it and presses it as a click would). */
    private static PausePanelButton last;

    private final OverlayClient overlay;
    private final Screen screen;
    private boolean placed;

    /** The button the last pause menu got, or null (the scenario driver). */
    public static PausePanelButton last() {
        return last;
    }

    /** Opens the panel as a click on it would (the scenario driver). */
    public void pressForDriver() {
        open();
    }

    private PausePanelButton(OverlayClient overlay, Screen screen, Component label) {
        super(0, 0, SIZE, SIZE, label);
        this.overlay = overlay;
        this.screen = screen;
        visible = false;
        setTooltip(Tooltip.create(label));
    }

    /** Adds the button to a pause menu while the launcher's bridge is connected. */
    public static void attach(OverlayClient overlay, Screen screen) {
        if (!(screen instanceof PauseScreen pause) || !pause.showsPauseMenu() || !overlay.state().online()) {
            return;
        }
        String key = overlay.key().keyName();
        Component label = key == null ? Texts.tr("pause.button") : Texts.tr("pause.button.key", key);
        PausePanelButton button = new PausePanelButton(overlay, screen, label);
        last = button;
        Screens.getWidgets(screen).add(button);
        ScreenEvents.beforeExtract(screen).register((s, g, mouseX, mouseY, delta) -> button.placeOnce());
    }

    /** Takes a free corner once every mod has added its buttons (the menu's first frame). */
    private void placeOnce() {
        if (placed) {
            return;
        }
        placed = true;
        List<AbstractWidget> others = Screens.getWidgets(screen);
        int[][] corners = {
            {screen.width - SIZE - MARGIN, screen.height - SIZE - MARGIN, -1},
            {MARGIN, screen.height - SIZE - MARGIN, -1},
        };
        for (int[] corner : corners) {
            for (int i = 0; i < TRIES; i++) {
                int x = corner[0];
                int y = corner[1] + corner[2] * i * (SIZE + 4);
                if (y >= MARGIN && free(others, x, y)) {
                    setX(x);
                    setY(y);
                    visible = true;
                    return;
                }
            }
        }
    }

    private boolean free(List<AbstractWidget> others, int x, int y) {
        for (AbstractWidget o : others) {
            if (o != this && o.visible && o.getX() < x + SIZE + 3 && o.getX() + o.getWidth() > x - 3
                && o.getY() < y + SIZE + 3 && o.getY() + o.getHeight() > y - 3) {
                return false;
            }
        }
        return true;
    }

    private void open() {
        pressed();
        overlay.openPanelFrom(screen);
    }

    @Override
    public void onClick(MouseButtonEvent event, boolean doubleClick) {
        open();
    }

    @Override
    public boolean keyPressed(KeyEvent event) {
        if (active && visible && (event.isSelection() || event.isConfirmation())) {
            open();
            return true;
        }
        return false;
    }

    @Override
    protected void draw(GuiGraphicsExtractor g, int mouseX, int mouseY, long now) {
        float x = getX();
        float y = getY();
        float h = Math.max(hover.value(now), isFocused() ? 1f : 0f);
        Draw.frame(g, Frames.PAUSE_BUTTON, x, y, SIZE, SIZE);
        if (h > 0f) {
            Draw.pushAlpha(h);
            Draw.frame(g, Frames.PAUSE_BUTTON_HOVER, x, y, SIZE, SIZE);
            Draw.popAlpha();
        }
        if (isFocused()) {
            Draw.frame(g, Frames.FOCUS_PAUSE, x, y, SIZE, SIZE);
        }
        float dy = press.value(now) * 0.5f;
        float inset = (SIZE - SPRITE) / 2f;
        Draw.texture(g, SPRITE_TEXTURE, x + inset, y + inset + dy, SPRITE, SPRITE, 0, 0, SPRITE, SPRITE, SPRITE, SPRITE,
            0xFFFFFFFF);
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        defaultButtonNarrationText(output);
    }
}
