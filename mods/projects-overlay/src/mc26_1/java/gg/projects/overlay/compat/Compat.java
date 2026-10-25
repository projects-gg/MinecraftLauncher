package gg.projects.overlay.compat;

import com.mojang.blaze3d.platform.InputConstants;
import com.mojang.blaze3d.platform.NativeImage;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.textures.FilterMode;
import java.net.URI;
import java.util.function.Supplier;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.util.Util;

/**
 * What differs between Minecraft versions, for 26.1.2. One copy per version lives in src/<compat>/java; the
 * build picks the one for -Pmc= (build.gradle). Everything else in the mod is shared.
 */
public final class Compat {
    /** The Minecraft version this copy is for. */
    public static final String VERSION = "26.1.2";

    /** The key type a keyboard key mapping uses. */
    public static final InputConstants.Type KEYBOARD = InputConstants.Type.KEYSYM;

    /** The menu (application) key, GLFW's code: opens the picked row's menu, as Shift+F10 (OVL-UX). */
    public static final int KEY_MENU = 348;

    private Compat() {
    }

    /** Keys that type a character (GLFW key codes): a letter key bound to the overlay still types while writing. */
    public static boolean typesCharacter(int key) {
        return key >= 32 && key <= 162;
    }

    /**
     * The chat's emoji picker shortcut (CHAT-EMOJI): Ctrl+E (⌘E on macOS), read as the game reads its own Ctrl+C in a
     * text field: the key in the GLFW layout's place, Control (Command on macOS), no Shift or Alt.
     */
    public static boolean isEmojiShortcut(KeyEvent event) {
        return event.input() == InputConstants.KEY_E && event.hasControlDownWithQuirk() && !event.hasShiftDown()
            && !event.hasAltDown();
    }

    /** The emoji picker shortcut as its tooltip names it. */
    public static String emojiShortcutLabel() {
        return System.getProperty("os.name", "").toLowerCase(java.util.Locale.ROOT).contains("mac") ? "⌘E" : "Ctrl+E";
    }

    /** The screen on top, or null in the game. */
    public static Screen screen(Minecraft mc) {
        return mc.screen;
    }

    public static void setScreen(Minecraft mc, Screen screen) {
        mc.setScreen(screen);
    }

    /** The loading overlay is up (resources reloading). */
    public static boolean overlayShowing(Minecraft mc) {
        return mc.getOverlay() != null;
    }

    /** F1: the HUD is hidden (screenshots, videos). */
    public static boolean hudHidden(Minecraft mc) {
        return mc.options.hideGui;
    }

    /** Opens a web address in the player's browser, the way the game opens a link from its chat. */
    public static void openUri(URI uri) {
        Util.getPlatform().openUri(uri);
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
