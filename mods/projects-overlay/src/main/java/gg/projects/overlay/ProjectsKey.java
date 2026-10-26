package gg.projects.overlay;

import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.panel.SettingsList;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;

/**
 * A key of the overlay, listed under "Projects" in Minecraft's Controls with its default: the overlay key (F7,
 * {@link OverlayKey}) and the chat emoji picker's (F9, {@link EmojiKey}). Players rebind them there or from the settings
 * page's "Kısayol" rows, whose "Varsayılana dön" puts them back ({@link #shortcut}); every hint names the key bound now
 * ({@link #keyName}) and goes without a key cap while none is.
 */
public abstract class ProjectsKey extends KeyMapping {
    protected ProjectsKey(String name, int defaultKey, Category category) {
        super(name, Compat.KEYBOARD, defaultKey, category);
    }

    /** The bound key's name in the game's language ("F7"), or null when no key is bound. */
    public String keyName() {
        return isUnbound() ? null : getTranslatedKeyMessage().getString();
    }

    /**
     * Binds the key {@code saved} names ({@code key.keyboard.f8}; null: the default) the way the game's Controls do, and
     * keeps it in {@code options.txt} at once.
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

    /** The settings page's "Kısayol" row for this key ({@code label}: its words' key); {@code change} opens the Controls. */
    public SettingsList.Shortcut shortcut(String label, Runnable change) {
        return new SettingsList.Shortcut() {
            @Override
            public String label() {
                return Texts.str(label);
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
                return ProjectsKey.this.isDefault();
            }

            @Override
            public void bind(String saved) {
                ProjectsKey.this.bind(saved);
            }
        };
    }
}
