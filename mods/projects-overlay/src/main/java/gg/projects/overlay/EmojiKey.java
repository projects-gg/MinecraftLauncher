package gg.projects.overlay;

import com.mojang.blaze3d.platform.InputConstants;
import net.minecraft.client.input.KeyEvent;

/**
 * The chat emoji picker's key (CHAT-EMOJI), listed under "Projects" in Minecraft's Controls next to the overlay key: F9
 * by default (the owner, 26 Oct: an easy key nothing else uses; F8 is the game's smooth camera, and nothing in the
 * official pack or the game uses F9, crates/pl-mc/testdata/key-bindings). In the game with no screen open it opens the
 * chat with the picker; on the chat screen it opens and closes the picker. Players rebind it there or from the
 * settings page ({@link ProjectsKey}).
 */
public final class EmojiKey extends ProjectsKey {
    public static final String NAME = "key.projects_overlay.emoji";

    public EmojiKey(Category category) {
        super(NAME, InputConstants.KEY_F9, category);
    }

    /** Whether a key press on the chat screen is this key (never while it is unbound). */
    public boolean pressed(KeyEvent event) {
        return !isUnbound() && matches(event);
    }
}
