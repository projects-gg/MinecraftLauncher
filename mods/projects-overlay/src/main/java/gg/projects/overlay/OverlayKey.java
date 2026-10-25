package gg.projects.overlay;

import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.panel.SettingsList;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.input.KeyEvent;

/**
 * The overlay key, listed under "Projects" in Minecraft's Controls: F7 by default, which nothing in the
 * official pack or the game uses (crates/pl-mc/testdata/key-bindings). Players rebind it there, and the settings
 * page's "Varsayılana dön" puts it back ({@link #shortcut}).
 */
public final class OverlayKey extends KeyMapping {
    public static final String NAME = "key.projects_overlay.toggle";

    public OverlayKey(Category category) {
        super(NAME, Compat.KEYBOARD, InputConstants.KEY_F7, category);
    }

    /** The bound key's name in the game's language ("F7"), or null when no key is bound. */
    public String keyName() {
        return isUnbound() ? null : getTranslatedKeyMessage().getString();
    }

    /**
     * Binds the key {@code saved} names ({@code key.keyboard.f8}; null: F7, the default) the way the game's Controls do,
     * and keeps it in {@code options.txt} at once.
     */
    public void bind(String saved) {
        InputConstants.Key key = getDefaultKey();
        if (saved != null) {
            try {
                key = InputConstants.getKey(saved);
            } catch (IllegalArgumentException e) {
                // Not a key this game knows: the default.
            }
        }
        setKey(key);
        KeyMapping.resetMapping();
        Minecraft.getInstance().options.save();
    }

    /** The settings page's "Kısayol" row for this key; {@code change} opens the game's Controls. */
    public SettingsList.Shortcut shortcut(Runnable change) {
        return new SettingsList.Shortcut() {
            @Override
            public String label() {
                return Texts.str("settings.key.toggle");
            }

            @Override
            public String name() {
                return keyName();
            }

            @Override
            public void change() {
                change.run();
            }

            @Override
            public String saved() {
                return saveString();
            }

            @Override
            public boolean isDefault() {
                return OverlayKey.this.isDefault();
            }

            @Override
            public void bind(String saved) {
                OverlayKey.this.bind(saved);
            }
        };
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
