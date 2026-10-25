package gg.projects.overlay.ui.panel;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.motion.Ease;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.Theme;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.draw.Frames;
import java.util.List;
import net.fabricmc.fabric.api.client.screen.v1.ScreenEvents;
import net.fabricmc.fabric.api.client.screen.v1.Screens;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.narration.NarrationElementOutput;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.util.FormattedCharSequence;

/**
 * The pause menu's Projects button (OVL-UX; the owner, 24 Oct: "ESC menüsüne de girebilir, sağ alta"): a small square in
 * the bottom right corner with our own sprite (the chat bubble with Küp's eyes, 16 x 16 pixels, so it stays sharp at every
 * GUI scale), opening the panel as the overlay key does. Only while the launcher's bridge is connected; nothing
 * otherwise.
 *
 * <p>Its tooltip is the overlay's own (OVL-UX2): "Projects katmanı" and the key on a keycap, as the toasts show the key,
 * in the launcher's type on the overlay's tooltip plate, above the button (the game's tooltip, a purple frame and white
 * pixel letters, is the game's own buttons' voice). It comes 250 ms after the pointer settles or at once for the
 * keyboard's focus, fading in over 110 ms; the narration still says "Projects katmanı (F7)".
 *
 * <p>It never covers another button: its place is chosen at the menu's first frame, once every mod has added its own (Mod
 * Menu, voice chat and replay mods put theirs in the menu's column or a top corner; world sharing's "Arkadaşlarını çağır"
 * stands in the top right one), going up the right edge and then to the left corner if the bottom right is taken, and it
 * stays away when no corner is free. Hover lights its face, a press
 * settles the sprite half a pixel; nothing moves otherwise.
 */
public final class PausePanelButton extends PanelWidget {
    public static final int SIZE = 20;
    private static final int SPRITE = 16;
    /** World sharing's "Arkadaşlarını çağır" keeps the same distance from its corner (top right). */
    private static final int MARGIN = 8;
    /** Rows up the corner it tries before the other corner. */
    private static final int TRIES = 3;
    private static final Identifier SPRITE_TEXTURE = Identifier.fromNamespaceAndPath(OverlayClient.MOD_ID,
        "textures/gui/overlay_button.png");
    private static final long TIP_DELAY_MS = 250;
    private static final long TIP_IN_MS = 110;
    private static final int TIP_H = 14;
    private static final int TIP_PAD = 5;
    private static final int TIP_GAP = 3;
    private static final int KEY_H = 10;
    private static final int KEY_PAD = 3;

    /** The last one added (the scenario driver looks at it and presses it as a click would). */
    private static PausePanelButton last;

    private final OverlayClient overlay;
    private final Screen screen;
    private boolean placed;
    // The tooltip: its words and the key, laid out once; since when it is wanted (0: not).
    private final FormattedCharSequence tipWords;
    private final FormattedCharSequence tipKey;
    private final String tipWordsText;
    private final String tipKeyText;
    private long tipSince;

    /** The button the last pause menu got, or null (the scenario driver). */
    public static PausePanelButton last() {
        return last;
    }

    /** Opens the panel as a click on it would (the scenario driver). */
    public void pressForDriver() {
        open();
    }

    private PausePanelButton(OverlayClient overlay, Screen screen, Component label, String key) {
        super(0, 0, SIZE, SIZE, label);
        this.overlay = overlay;
        this.screen = screen;
        visible = false;
        tipWordsText = Texts.str("pause.button");
        tipWords = Texts.visual(tipWordsText);
        tipKeyText = key;
        tipKey = key == null ? null : Texts.visual(key);
    }

    /** Adds the button to a pause menu while the launcher's bridge is connected. */
    public static void attach(OverlayClient overlay, Screen screen) {
        if (!(screen instanceof PauseScreen pause) || !pause.showsPauseMenu() || !overlay.state().online()) {
            return;
        }
        String key = overlay.key().keyName();
        Component label = key == null ? Texts.tr("pause.button") : Texts.tr("pause.button.key", key);
        PausePanelButton button = new PausePanelButton(overlay, screen, label, key);
        last = button;
        Screens.getWidgets(screen).add(button);
        ScreenEvents.beforeExtract(screen).register((s, g, mouseX, mouseY, delta) -> button.placeOnce());
        // Over everything the menu drew, the way the overlay's toasts sit over a screen.
        ScreenEvents.afterExtract(screen).register((s, g, mouseX, mouseY, delta) -> button.drawTip(g));
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

    /** The tooltip, while the pointer rests on the button or the keyboard's focus is on it. */
    private void drawTip(GuiGraphicsExtractor g) {
        long now = now();
        // As the game's own tooltips: under the pointer, or on the focus the keyboard moved here.
        boolean focusedByKeys = isFocused() && !isHovered()
            && Minecraft.getInstance().getLastInputType().isKeyboard();
        if (!visible || !(isHovered() || focusedByKeys)) {
            tipSince = 0;
            return;
        }
        if (tipSince == 0) {
            // The keyboard's focus shows it at once; the pointer only once it settles.
            tipSince = focusedByKeys ? now - TIP_DELAY_MS : now;
        }
        long shown = now - tipSince - TIP_DELAY_MS;
        if (shown < 0) {
            return;
        }
        Font small = Fonts.small();
        float wordsW = small.width(tipWordsText);
        float keyW = tipKey == null ? 0f : small.width(tipKeyText) + 2 * KEY_PAD;
        float w = TIP_PAD + wordsW + (tipKey == null ? 0f : 4 + keyW) + TIP_PAD;
        // Above the button, on the side of the screen it stands on.
        float x = getX() + getWidth() / 2f > screen.width / 2f ? getX() + getWidth() - w : getX();
        x = Draw.snap(Math.max(2f, Math.min(screen.width - 2f - w, x)));
        float y = getY() - TIP_GAP - TIP_H;
        if (y < 2f) {
            y = getY() + getHeight() + TIP_GAP;
        }
        y = Draw.snap(y);
        Draw.stratum(g);
        Draw.pushAlpha(Ease.OUT.at(Math.min(1f, shown / (float) TIP_IN_MS)));
        Draw.frame(g, Frames.TOOLTIP, x, y, w, TIP_H);
        float textY = Fonts.centeredY(Fonts.Face.SMALL, y + TIP_H / 2f);
        Draw.text(g, small, tipWords, x + TIP_PAD, textY, Theme.BONE_100);
        if (tipKey != null) {
            float kx = x + TIP_PAD + wordsW + 4;
            float ky = y + (TIP_H - KEY_H) / 2f;
            Draw.frame(g, Frames.KEYCAP, kx, ky, keyW, KEY_H);
            Draw.text(g, small, tipKey, kx + KEY_PAD, Fonts.centeredY(Fonts.Face.SMALL, ky + KEY_H / 2f) - 0.5f,
                Theme.BONE_100);
        }
        Draw.popAlpha();
    }

    @Override
    protected void updateWidgetNarration(NarrationElementOutput output) {
        defaultButtonNarrationText(output);
    }
}
