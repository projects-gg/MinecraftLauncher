package gg.projects.overlay.compat;

import gg.projects.overlay.host.HostGate;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.GameType;

/**
 * What world sharing does differently per Minecraft version (host1.md §4.2), for 26.1.2: {@code publishServer(GameType,
 * boolean, int)}, no {@code unpublishServer()}. Stopping here means the guests are gone and the gate refuses everyone;
 * the listener stays on loopback until the world closes, harmless. One copy per version in src/<compat>/java.
 */
public final class HostCompat {
    /** The pause menu's own sharing button (dimmed while our session is open). */
    public static final String VANILLA_SHARE_KEY = "menu.shareToLan";
    /** Stopping cannot close the listener: the world stays published (to nobody) until it closes. */
    public static final boolean CAN_UNPUBLISH = false;

    private HostCompat() {
    }

    /**
     * Opens the world for guests on {@code port} (the mixins keep it on loopback and silent). Guests play {@code mode};
     * {@code commands} lets them use commands.
     */
    public static boolean publish(MinecraftServer server, String mode, boolean commands, int port) {
        return server instanceof IntegratedServer integrated
            && integrated.publishServer(GameType.byName(mode, GameType.SURVIVAL), commands, port);
    }

    /** Sharing stops (the guests were sent off already): commands go back to the world's own rule. */
    public static void unpublish(MinecraftServer server) {
        if (!server.getPlayerList().isAllowCommandsForAllPlayers()) {
            return;
        }
        server.getPlayerList().setAllowCommandsForAllPlayers(false);
        // As the game's own publish does when it changes them: the host's game hears its permissions again.
        LocalPlayer player = Minecraft.getInstance().player;
        if (player != null) {
            player.setPermissions(server.getProfilePermissions(player.nameAndId()));
            player.refreshChatAbilities();
        }
        for (ServerPlayer p : server.getPlayerList().getPlayers()) {
            server.getCommands().sendCommands(p);
        }
    }

    /** Vanilla's "LAN'a Aç" is on (and the listener is not ours). */
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
