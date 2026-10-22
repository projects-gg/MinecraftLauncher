package gg.projects.overlay.dev.mixin;

import com.mojang.blaze3d.platform.Window;
import com.mojang.blaze3d.systems.GpuBackend;
import org.lwjgl.glfw.GLFW;
import org.lwjgl.glfw.GLFWNativeWin32;
import org.lwjgl.system.Platform;
import org.lwjgl.system.windows.User32;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Automated runs share the desktop with a person: the test window opens without taking keyboard focus, and
 * it never becomes the active window later either, so nothing typed elsewhere lands in the game.
 */
@Mixin(Window.class)
abstract class WindowNoFocusMixin {
    @Inject(method = "createGlfwWindow", require = 1, at = @At(value = "INVOKE",
        target = "Lorg/lwjgl/glfw/GLFW;glfwCreateWindow(IILjava/lang/CharSequence;JJ)J"))
    private static void ovl1$noFocus(int width, int height, String title, long monitor, GpuBackend backend,
        CallbackInfoReturnable<Long> cir) {
        GLFW.glfwWindowHint(GLFW.GLFW_FOCUSED, GLFW.GLFW_FALSE);
        GLFW.glfwWindowHint(GLFW.GLFW_FOCUS_ON_SHOW, GLFW.GLFW_FALSE);
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
        long hwnd = GLFWNativeWin32.glfwGetWin32Window(((Window) (Object) this).handle());
        if (hwnd != 0L) {
            long style = User32.GetWindowLongPtr(hwnd, User32.GWL_EXSTYLE);
            User32.SetWindowLongPtr(null, hwnd, User32.GWL_EXSTYLE, style | User32.WS_EX_NOACTIVATE);
        }
    }
}
