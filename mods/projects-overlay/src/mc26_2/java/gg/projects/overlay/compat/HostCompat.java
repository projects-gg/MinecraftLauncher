package gg.projects.overlay.compat;

import gg.projects.overlay.host.HostGate;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.GameType;

/**
 * What world sharing does differently per Minecraft version (host1.md §4.2), for 26.2: {@code publishServer(
 * MultiplayerScope, GameType, boolean, int)} and {@code unpublishServer()}. One copy per version in src/<compat>/java.
 */
public final class HostCompat {
    /** The pause menu's own sharing button (dimmed while our session is open). */
    public static final String VANILLA_SHARE_KEY = "menu.multiplayerOptions.button";
    /** Stopping closes the listener: afterwards the world is not published at all. */
    public static final boolean CAN_UNPUBLISH = true;

    private HostCompat() {
    }

    /**
     * Opens the world for guests on {@code port} (the mixins keep it on loopback and silent). Guests play {@code mode};
     * {@code commands} lets them use commands.
     */
    public static boolean publish(MinecraftServer server, String mode, boolean commands, int port) {
        return server instanceof IntegratedServer integrated && integrated.publishServer(
            MinecraftServer.MultiplayerScope.LAN, GameType.byName(mode, GameType.SURVIVAL), commands, port);
    }

    /** Sharing stops (the guests were sent off already): the game closes the listener and resets what it published. */
    public static void unpublish(MinecraftServer server) {
        if (server instanceof IntegratedServer integrated) {
            integrated.unpublishServer();
        }
    }

    /** The game's own sharing is on (and the listener is not ours). */
    public static boolean vanillaLanOpen(MinecraftServer server) {
        return server.isPublished() && !HostGate.publishedBy(server);
    }

    /** A guest is in the world: the game itself gave it the published game mode. */
    public static void guestJoined(MinecraftServer server, ServerPlayer player, String mode) {
    }

    /** The game language a guest's client reported ({@code tr_tr}, {@code en_us}). */
    public static String language(ServerPlayer player) {
        return player.clientInformation().language();
    }
}
