package gg.projects.overlay.compat;

import com.mojang.blaze3d.platform.InputConstants;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.Screen;

/**
 * What differs between Minecraft versions, for 26.2. One copy per version lives in src/<compat>/java; the
 * build picks the one for -Pmc= (build.gradle). Everything else in the mod is shared.
 */
public final class Compat {
    /** The Minecraft version this copy is for. */
    public static final String VERSION = "26.2";

    /** The key type a keyboard key mapping uses. */
    public static final InputConstants.Type KEYBOARD = InputConstants.Type.KEYSYM;

    private Compat() {
    }

    /** Keys that type a character (GLFW key codes): a letter key bound to the overlay still types while writing. */
    public static boolean typesCharacter(int key) {
        return key >= 32 && key <= 162;
    }

    /** The screen on top, or null in the game. */
    public static Screen screen(Minecraft mc) {
        return mc.gui.screen();
    }

    public static void setScreen(Minecraft mc, Screen screen) {
        mc.gui.setScreen(screen);
    }

    /** The loading overlay is up (resources reloading). */
    public static boolean overlayShowing(Minecraft mc) {
        return mc.gui.overlay() != null;
    }

    /** F1: the HUD is hidden (screenshots, videos). */
    public static boolean hudHidden(Minecraft mc) {
        return mc.gui.hud.isHidden();
    }
}
