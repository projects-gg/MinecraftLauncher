package gg.projects.overlay.dev.mixin;

import gg.projects.overlay.OverlayClient;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Development only (the dev source set never reaches the released jar): in the link scenario
 * ({@code -Dprojects.overlay.dev.script=link}) the singleplayer world's own server plays ProBotLinker's part on
 * {@code projects:link}, so the overlay takes it for Projects there. Every other run keeps the real rule.
 */
@Mixin(value = OverlayClient.class, remap = false)
abstract class LinkProjectsMixin {
    @Inject(method = "onProjects", require = 1, at = @At("HEAD"), cancellable = true)
    private void skin1o$scenarioServerIsProjects(CallbackInfoReturnable<Boolean> cir) {
        if ("link".equals(System.getProperty("projects.overlay.dev.script"))) {
            cir.setReturnValue(true);
        }
    }
}
