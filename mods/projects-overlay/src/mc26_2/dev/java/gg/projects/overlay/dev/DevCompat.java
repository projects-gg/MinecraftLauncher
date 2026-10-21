package gg.projects.overlay.dev;

import com.mojang.blaze3d.pipeline.RenderTarget;
import java.util.function.Function;
import net.minecraft.client.Minecraft;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import net.minecraft.world.level.levelgen.WorldDimensions;

/** What the scenario driver needs that differs between Minecraft versions (26.2). */
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
}
