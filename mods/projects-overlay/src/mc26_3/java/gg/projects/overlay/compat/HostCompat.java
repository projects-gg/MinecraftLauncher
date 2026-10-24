package gg.projects.overlay.compat;

import gg.projects.overlay.host.HostGate;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.GameType;

/**
 * What world sharing does differently per Minecraft version (host1.md §4.2), for 26.3: {@code publishServer(
 * MultiplayerScope, boolean, int)} without a game mode, {@code unpublishServer()}, guest commands through
 * {@code setGuestCommandAccess}. The guests' game mode is given as each one joins. One copy per version in
 * src/<compat>/java.
 */
public final class HostCompat {
    /**
     * The pause menu's own sharing lives in the world's options with everything else: not dimmed (its LAN switch
     * turning sharing off ends our session first, the mixin's job).
     */
    public static final String VANILLA_SHARE_KEY = null;
    /** Stopping closes the listener: afterwards the world is not published at all. */
    public static final boolean CAN_UNPUBLISH = true;

    /** The guests' command access as it was before our publish, put back when sharing stops. */
    private static Boolean commandsBefore;

    private HostCompat() {
    }

    /**
     * Opens the world for guests on {@code port} (the mixins keep it on loopback and silent). {@code commands} lets them
     * use commands; their game mode comes as they join ({@link #guestJoined}).
     */
    public static boolean publish(MinecraftServer server, String mode, boolean commands, int port) {
        if (!(server instanceof IntegratedServer integrated)) {
            return false;
        }
        commandsBefore = integrated.getGuestCommandAccess();
        boolean ok = integrated.publishServer(MinecraftServer.MultiplayerScope.LAN, commands, port);
        if (!ok) {
            integrated.setGuestCommandAccess(commandsBefore);
            commandsBefore = null;
        }
        return ok;
    }

    /** Sharing stops (the guests were sent off already): the game closes the listener; guest commands as before. */
    public static void unpublish(MinecraftServer server) {
        if (server instanceof IntegratedServer integrated) {
            integrated.unpublishServer();
            if (commandsBefore != null) {
                integrated.setGuestCommandAccess(commandsBefore);
                commandsBefore = null;
            }
        }
    }

    /** The game's own sharing is on (and the listener is not ours). */
    public static boolean vanillaLanOpen(MinecraftServer server) {
        return server.isPublished() && !HostGate.publishedBy(server);
    }

    /** A guest is in the world: it plays the mode the host chose (a hardcore world keeps its own rules). */
    public static void guestJoined(MinecraftServer server, ServerPlayer player, String mode) {
        if (!server.isHardcore()) {
            player.setGameMode(GameType.byName(mode, GameType.SURVIVAL));
        }
    }

    /** The game language a guest's client reported ({@code tr_tr}, {@code en_us}). */
    public static String language(ServerPlayer player) {
        return player.clientInformation().language();
    }
}
