package gg.projects.overlay.dev.mixin;

import gg.projects.overlay.dev.FrameClock;
import net.minecraft.client.Minecraft;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Timestamps the start of every frame for the spike's frame-time measurement. */
@Mixin(Minecraft.class)
abstract class MinecraftFrameMixin {
    @Inject(method = "runTick", at = @At("HEAD"))
    private void ovl1$frameStart(boolean advanceGameTime, CallbackInfo ci) {
        FrameClock.onFrameStart();
    }
}
