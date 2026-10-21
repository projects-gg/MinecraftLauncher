package gg.projects.overlay.dev.mixin;

import com.mojang.blaze3d.platform.Window;
import com.mojang.blaze3d.systems.GpuBackend;
import org.lwjgl.glfw.GLFW;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Automated runs share the desktop with a person: the test window opens without taking keyboard focus, so
 * nothing typed elsewhere lands in the game.
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
}
