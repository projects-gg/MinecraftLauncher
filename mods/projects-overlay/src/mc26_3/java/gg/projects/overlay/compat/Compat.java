package gg.projects.overlay.compat;

import com.mojang.blaze3d.Blaze3D;
import com.mojang.blaze3d.platform.InputConstants;
import com.mojang.blaze3d.platform.NativeImage;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.textures.FilterMode;
import java.net.URI;
import java.util.function.Supplier;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.renderer.texture.DynamicTexture;

/**
 * What differs between Minecraft versions, for 26.3. One copy per version lives in src/<compat>/java; the
 * build picks the one for -Pmc= (build.gradle). Everything else in the mod is shared.
 */
public final class Compat {
    /** The Minecraft version this copy is for. */
    public static final String VERSION = "26.3";

    /** The key type a keyboard key mapping uses. */
    public static final InputConstants.Type KEYBOARD = InputConstants.Type.KEYBOARD;

    private Compat() {
    }

    /**
     * Keys that type a character, as SDL scancodes (26.3 reads the keyboard through SDL3): a letter key
     * bound to the overlay still types while writing.
     */
    public static boolean typesCharacter(int key) {
        return (key >= 4 && key <= 39) || (key >= 44 && key <= 56) || key == 100;
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

    /** Opens a web address in the player's browser, the way the game opens a link from its chat. */
    public static void openUri(URI uri) {
        Blaze3D.openUri(uri);
    }

    /** A texture sampled with linear filtering: stickers stay smooth while they move, sharp where they rest. */
    public static DynamicTexture smoothTexture(Supplier<String> label, NativeImage image) {
        return new DynamicTexture(label, image) {
            {
                sampler = RenderSystem.getSamplerCache().getClampToEdge(FilterMode.LINEAR);
            }
        };
    }
}
