package gg.projects.overlay;

import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.compat.Compat;
import net.minecraft.client.input.KeyEvent;

/**
 * The overlay key, listed under "Projects" in Minecraft's Controls: F7 by default, which nothing in the
 * official pack or the game uses (crates/pl-mc/testdata/key-bindings). Players rebind it there or from the settings
 * page ({@link ProjectsKey}).
 */
public final class OverlayKey extends ProjectsKey {
    public static final String NAME = "key.projects_overlay.toggle";

    public OverlayKey(Category category) {
        super(NAME, InputConstants.KEY_F7, category);
    }

    /**
     * Whether a key press in the panel is the overlay key and should close it. A key that types a character
     * (a letter, a digit) closes only while nobody types, so it still writes into the message field.
     */
    public boolean closes(KeyEvent event, boolean typing) {
        if (isUnbound() || !matches(event)) {
            return false;
        }
        return !typing || !Compat.typesCharacter(event.key());
    }
}
