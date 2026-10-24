package gg.projects.overlay.ui.panel;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.compat.HostCompat;
import gg.projects.overlay.host.HostController;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.draw.Icon;
import java.util.List;
import net.fabricmc.fabric.api.client.screen.v1.Screens;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.network.chat.contents.TranslatableContents;

/**
 * The pause menu's "Arkadaşlarını çağır" (HOST1, host1.md §4.8): the Projects button in the top right corner of a
 * singleplayer world's pause menu (top left when another mod has that corner), added after the menu is built, its own
 * layout untouched. Dimmed with "Launcher açık olmalı." while the launcher is away. While the world is open to friends
 * the game's own sharing button is dimmed: two ways of sharing one world would fight.
 */
public final class HostPauseButton {
    private static final int MARGIN = 8;

    private HostPauseButton() {
    }

    /** Adds the button to {@code screen} when world sharing makes sense there. */
    public static void attach(OverlayClient overlay, PauseScreen screen) {
        HostController host = overlay.host();
        if (!host.inWorld()) {
            return;
        }
        List<AbstractWidget> widgets = Screens.getWidgets(screen);
        if (host.hosting() && HostCompat.VANILLA_SHARE_KEY != null) {
            for (AbstractWidget w : widgets) {
                if (w instanceof Button b && b.getMessage().getContents() instanceof TranslatableContents t
                    && HostCompat.VANILLA_SHARE_KEY.equals(t.getKey())) {
                    b.active = false;
                    b.setTooltip(Tooltip.create(Texts.tr("host.lan_off")));
                }
            }
        }
        if (!host.canInvite() && !host.hosting()) {
            return;
        }
        ActionButton button = new ActionButton(ActionButton.Style.SECONDARY, Texts.tr("host.invite_friends"),
            Icon.USER_PLUS, () -> overlay.openWorldSharing(screen));
        int w = button.preferredWidth();
        int h = ActionButton.HEIGHT;
        int x = screen.width - w - MARGIN;
        int y = MARGIN;
        if (taken(widgets, x, y, w, h)) {
            x = MARGIN;
        }
        PanelWidget.place(button, x, y, w, h);
        if (host.launcherAway()) {
            button.active = false;
            button.setTooltip(Tooltip.create(Texts.tr("host.launcher_needed")));
        }
        widgets.add(button);
    }

    /** Another widget already stands in that box. */
    private static boolean taken(List<AbstractWidget> widgets, int x, int y, int w, int h) {
        for (AbstractWidget o : widgets) {
            if (o.visible && o.getX() < x + w + 2 && o.getX() + o.getWidth() > x - 2 && o.getY() < y + h + 2
                && o.getY() + o.getHeight() > y - 2) {
                return true;
            }
        }
        return false;
    }
}
