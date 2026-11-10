package gg.projects.overlay.chat;

import gg.projects.overlay.ui.Texts;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.Level;

/**
 * "Konumum" (CREW-PLUS, a player's suggestion): the player's block position and dimension as one plain chat line in the
 * overlay's language, readable everywhere a message shows (launcher, phone, overlay): "Konumum: X 120, Y 64, Z -340
 * (Nether)". The letters stay between the numbers on purpose: the friends service turns a run of ten digits away as a
 * phone number ("5321 64 -5432" reads as 5321645432), and a letter ends the run. The Nether and the End by name; the
 * Overworld and any other world (a server's own) without one, so a server's inner world name never shows.
 *
 * <p>Never while the game itself shows only reduced debug info ({@link Minecraft#showOnlyReducedInfo()}: the server's
 * {@code reducedDebugInfo} game rule or the player's own "Reduced Debug Info" option, F3's own rule): the overlay never
 * tells what F3 would not. Nor outside a world. The names are the same in 26.1.2, 26.2 and 26.3 (checked in Loom's
 * merged jars), so no version needs its own.
 */
public final class MyLocation {
    private MyLocation() {
    }

    /** The player is in a world whose coordinates the game shows: "Konumum" may be offered. */
    public static boolean available() {
        Minecraft mc = Minecraft.getInstance();
        return mc.player != null && mc.level != null && !mc.showOnlyReducedInfo();
    }

    /** The line for the player where they stand now, or null when it may not be told. */
    public static String now() {
        Minecraft mc = Minecraft.getInstance();
        LocalPlayer player = mc.player;
        if (player == null || mc.level == null || mc.showOnlyReducedInfo()) {
            return null;
        }
        BlockPos at = player.blockPosition();
        return text(at.getX(), at.getY(), at.getZ(), dimension(mc.level.dimension()));
    }

    /** {@code nether} or {@code end} for those two dimensions; null for the Overworld and any other world. */
    static String dimension(ResourceKey<Level> key) {
        if (Level.NETHER.equals(key)) {
            return "nether";
        }
        if (Level.END.equals(key)) {
            return "end";
        }
        return null;
    }

    /** "Konumum: X 120, Y 64, Z -340" and, in the Nether or the End, " (Nether)" after it. */
    public static String text(int x, int y, int z, String dimension) {
        String line = Texts.str("chat.location.text", Integer.toString(x), Integer.toString(y), Integer.toString(z));
        if (dimension == null) {
            return line;
        }
        return Texts.str("chat.location.in", line, Texts.str("chat.location." + dimension));
    }
}
