package gg.projects.overlay.dev.mixin;

import com.mojang.blaze3d.platform.Window;
import org.lwjgl.sdl.SDLHints;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Automated runs share the desktop with a person: the test window opens without taking keyboard focus, so
 * nothing typed elsewhere lands in the game. 26.3 makes its window with SDL3: the hints say not to activate
 * it when it is shown or raised.
 */
@Mixin(Window.class)
abstract class WindowNoFocusMixin {
    @Inject(method = "createWindow", require = 1, at = @At("HEAD"))
    private void ovl2$noFocus(CallbackInfoReturnable<Long> cir) {
        SDLHints.SDL_SetHint("SDL_WINDOW_ACTIVATE_WHEN_SHOWN", "0");
        SDLHints.SDL_SetHint("SDL_WINDOW_ACTIVATE_WHEN_RAISED", "0");
    }
}
