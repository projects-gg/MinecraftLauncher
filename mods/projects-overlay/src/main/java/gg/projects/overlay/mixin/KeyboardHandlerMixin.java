package gg.projects.overlay.mixin;

import gg.projects.overlay.OverlayClient;
import net.minecraft.client.KeyboardHandler;
import net.minecraft.client.input.KeyEvent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Sees Shift+Tab before vanilla does, so the chord opens the overlay instead of flashing the player list. */
@Mixin(KeyboardHandler.class)
abstract class KeyboardHandlerMixin {
    @Inject(method = "keyPress", at = @At("HEAD"), cancellable = true)
    private void projectsOverlay$openOnChord(long window, int action, KeyEvent event, CallbackInfo ci) {
        if (OverlayClient.consumeChord(window, action, event)) {
            ci.cancel();
        }
    }
}
