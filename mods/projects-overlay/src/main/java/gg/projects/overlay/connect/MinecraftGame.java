package gg.projects.overlay.connect;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.toast.ToastCard;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.panel.OverlayScreen;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.ConnectScreen;
import net.minecraft.client.gui.screens.GenericMessageScreen;
import net.minecraft.client.gui.screens.GenericWaitingScreen;
import net.minecraft.client.gui.screens.LevelLoadingScreen;
import net.minecraft.client.gui.screens.ProgressScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.gui.screens.multiplayer.JoinMultiplayerScreen;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.resolver.ServerAddress;

/** The game side of {@link Connector}: its screens, leaving a world, and its own connect screen. */
public final class MinecraftGame implements Connector.Game {
    private final OverlayClient overlay;

    public MinecraftGame(OverlayClient overlay) {
        this.overlay = overlay;
    }

    /** The screens the game shows on its way somewhere: connecting, loading, saving, disconnecting. */
    static boolean onTheWay(Screen screen) {
        return screen instanceof ConnectScreen || screen instanceof LevelLoadingScreen
            || screen instanceof GenericMessageScreen || screen instanceof ProgressScreen
            || screen instanceof GenericWaitingScreen;
    }

    @Override
    public boolean busy() {
        Minecraft mc = Minecraft.getInstance();
        if (Compat.overlayShowing(mc) || onTheWay(Compat.screen(mc))) {
            return true;
        }
        // A world starting, or a server login, before a screen says so.
        return mc.level == null && (mc.getSingleplayerServer() != null || mc.getConnection() != null);
    }

    @Override
    public boolean inWorld() {
        return Minecraft.getInstance().level != null;
    }

    @Override
    public ServerTarget currentServer() {
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null || mc.isLocalServer()) {
            return null;
        }
        ServerData server = mc.getCurrentServer();
        return server == null || server.ip == null ? null : ServerTarget.parse(server.ip);
    }

    @Override
    public boolean valid(ServerTarget target) {
        return ServerAddress.isValidAddress(target.address());
    }

    @Override
    public boolean leaveAndConnect(ServerTarget target) {
        Minecraft mc = Minecraft.getInstance();
        String address = target.address();
        if (!valid(target)) {
            return false;
        }
        if (mc.level != null) {
            // What the pause menu's "Save and Quit to Title" (or "Disconnect") does: a singleplayer world is saved.
            mc.disconnectFromWorld(ClientLevel.DEFAULT_QUIT_MESSAGE);
        }
        ServerData server = new ServerData(target.displayName(), address, ServerData.Type.OTHER);
        ConnectScreen.startConnecting(new JoinMultiplayerScreen(new TitleScreen()), mc,
            ServerAddress.parseString(address), server, false, null);
        return true;
    }

    @Override
    public void showTravel(ServerTarget target) {
        if (Compat.screen(Minecraft.getInstance()) instanceof OverlayScreen panel) {
            panel.close();
        }
        overlay.toasts().travel(ToastCard.travel(Texts.str("connect.travel", target.displayName())),
            overlay.reducedMotion());
    }

    @Override
    public void endTravel() {
        overlay.toasts().endTravel();
    }

    @Override
    public boolean travelling() {
        Minecraft mc = Minecraft.getInstance();
        return Compat.overlayShowing(mc) || onTheWay(Compat.screen(mc));
    }

    @Override
    public void showAlready(ServerTarget target) {
        overlay.toasts().notice(ToastCard.travel(Texts.str("connect.already", target.displayName())),
            overlay.settings(), overlay.reducedMotion());
    }
}
