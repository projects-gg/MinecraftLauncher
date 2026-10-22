package gg.projects.overlay.dev.mixin;

import com.mojang.blaze3d.platform.Window;
import org.lwjgl.sdl.SDLHints;
import org.lwjgl.sdl.SDLProperties;
import org.lwjgl.sdl.SDLVideo;
import org.lwjgl.system.Platform;
import org.lwjgl.system.windows.User32;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Automated runs share the desktop with a person: the test window opens without taking keyboard focus, and
 * it never becomes the active window later either, so nothing typed elsewhere lands in the game. 26.3 makes
 * its window with SDL3: the hints say not to activate it when it is shown or raised.
 */
@Mixin(Window.class)
abstract class WindowNoFocusMixin {
    @Inject(method = "createWindow", require = 1, at = @At("HEAD"))
    private void ovl2$noFocus(CallbackInfoReturnable<Long> cir) {
        SDLHints.SDL_SetHint("SDL_WINDOW_ACTIVATE_WHEN_SHOWN", "0");
        SDLHints.SDL_SetHint("SDL_WINDOW_ACTIVATE_WHEN_RAISED", "0");
    }

    /**
     * Windows hands the keyboard to the next window when the active one closes; without this a person's
     * typing reached the test game once (run2: its chat opened by itself). A no-activate window still draws
     * and still gets the scenario's own calls.
     */
    @Inject(method = "<init>", require = 1, at = @At("TAIL"))
    private void ovl2$neverActivate(CallbackInfo ci) {
        if (Platform.get() != Platform.WINDOWS) {
            return;
        }
        int props = SDLVideo.SDL_GetWindowProperties(((Window) (Object) this).handle());
        long hwnd = SDLProperties.SDL_GetPointerProperty(props, SDLVideo.SDL_PROP_WINDOW_WIN32_HWND_POINTER, 0L);
        if (hwnd != 0L) {
            long style = User32.GetWindowLongPtr(hwnd, User32.GWL_EXSTYLE);
            User32.SetWindowLongPtr(null, hwnd, User32.GWL_EXSTYLE, style | User32.WS_EX_NOACTIVATE);
        }
    }
}
