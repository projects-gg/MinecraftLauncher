package gg.projects.overlay.keys.mixin;

import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.compat.Compat;
import net.minecraft.client.KeyboardHandler;
import net.minecraft.client.input.KeyEvent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * KEY-HINT: the keys the game's window hears, as the game hears them (GLFW's key callback through the game's own
 * handler: no system hook), for the toasts' key hint to learn whether this computer gives F1 to F12 only with Fn
 * ({@link gg.projects.overlay.keys.KeyHint}). The overlay looks only while a hint is on screen and keeps nothing but the
 * conclusion; the key goes on to the game unchanged. Not required: a game whose handler differs only learns nothing.
 */
@Mixin(KeyboardHandler.class)
abstract class KeyboardHandlerMixin {
    @Inject(method = "keyPress", at = @At("HEAD"), require = 0)
    private void projects$heard(long window, int action, KeyEvent event, CallbackInfo info) {
        OverlayClient overlay = OverlayClient.get();
        if (overlay != null) {
            overlay.heardKey(event.key(), Compat.scancode(event), action);
        }
    }
}
