package gg.projects.overlay.dev;

import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.blaze3d.platform.InputConstants;
import java.util.function.Function;
import net.minecraft.client.Minecraft;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import net.minecraft.world.level.levelgen.WorldDimensions;

/** What the scenario driver needs that differs between Minecraft versions (26.3). */
final class DevCompat {
    private DevCompat() {
    }

    static RenderTarget mainTarget(Minecraft mc) {
        return mc.gameRenderer.mainRenderTarget();
    }

    /** A flat world's dimensions for a new test world. */
    static Function<HolderLookup.Provider, WorldDimensions> flatWorld() {
        return registries -> registries.lookupOrThrow(Registries.WORLD_PRESET).getOrThrow(WorldPresets.FLAT).value()
            .createWorldDimensions();
    }

    /** A normal world's dimensions (real terrain for screenshots). */
    static Function<HolderLookup.Provider, WorldDimensions> normalWorld() {
        return registries -> registries.lookupOrThrow(Registries.WORLD_PRESET).getOrThrow(WorldPresets.NORMAL).value()
            .createWorldDimensions();
    }

    /**
     * Ctrl+E as this version's keyboard reports it: SDL's scancode, its keycode ("e") and Control (the emoji picker's
     * old shortcut, which must do nothing now).
     */
    static KeyEvent ctrlE() {
        return new KeyEvent(InputConstants.KEY_E, 'e', InputConstants.MOD_CONTROL);
    }
}
