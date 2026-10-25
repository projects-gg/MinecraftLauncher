package gg.projects.overlay.connect.mixin;

import gg.projects.overlay.connect.WorldResume;
import net.minecraft.client.Minecraft;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Counts the frames the game begins, for "Devam et" into a world ({@link WorldResume}: the world opens only after a frame
 * that followed the title screen). Optional: should the method be gone in a later version, the title screen's own first
 * drawn frame stands in and nothing breaks.
 */
@Mixin(Minecraft.class)
abstract class MinecraftFramesMixin {
    @Inject(method = "renderFrame", at = @At("HEAD"), require = 0)
    private void projects$frame(boolean advanceGameTime, CallbackInfo info) {
        WorldResume.frameStarted();
    }
}
