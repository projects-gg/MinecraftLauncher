package gg.projects.overlay;

import com.mojang.blaze3d.platform.InputConstants;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.network.chat.Component;
import org.lwjgl.glfw.GLFW;

/**
 * The overlay key, listed under "Projects" in Minecraft's Controls. The default is the Shift+Tab chord, which
 * vanilla key mappings cannot express. It is stored as "no key" so that it never collides with the player
 * list on Tab (no red conflict in Controls), shown as "Shift + Tab", and detected by the keyboard mixin.
 * Binding any key in Controls replaces the chord; "Reset" brings it back.
 */
public final class OverlayKey extends KeyMapping {
    public static final String NAME = "key.projects_overlay.toggle";

    public OverlayKey(Category category) {
        super(NAME, InputConstants.Type.KEYSYM, InputConstants.UNKNOWN.getValue(), category);
    }

    /** True while the player kept the default binding (the Shift+Tab chord). */
    public boolean usesChord() {
        return isUnbound();
    }

    @Override
    public Component getTranslatedKeyMessage() {
        return usesChord() ? Component.translatable("projects_overlay.key.chord") : super.getTranslatedKeyMessage();
    }

    /** Whether a key press (in a screen or in the game) is the overlay key. */
    public boolean isToggle(KeyEvent event) {
        return usesChord() ? isChord(event) : matches(event);
    }

    static boolean isChord(KeyEvent event) {
        return event.key() == GLFW.GLFW_KEY_TAB && (event.modifiers() & GLFW.GLFW_MOD_SHIFT) != 0;
    }
}
