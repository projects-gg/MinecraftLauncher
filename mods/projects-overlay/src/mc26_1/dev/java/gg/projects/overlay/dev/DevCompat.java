package gg.projects.overlay.dev;

import com.mojang.blaze3d.pipeline.RenderTarget;
import java.util.function.Function;
import net.minecraft.client.Minecraft;
import net.minecraft.core.HolderLookup;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import net.minecraft.world.level.levelgen.WorldDimensions;

/** What the scenario driver needs that differs between Minecraft versions (26.1.2). */
final class DevCompat {
    private DevCompat() {
    }

    static RenderTarget mainTarget(Minecraft mc) {
        return mc.getMainRenderTarget();
    }

    /** A flat world's dimensions for a new test world. */
    static Function<HolderLookup.Provider, WorldDimensions> flatWorld() {
        return WorldPresets::createFlatWorldDimensions;
    }
}
