package gg.projects.overlay.gametest;

import gg.projects.overlay.ui.panel.OverlayScreen;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.minecraft.client.gui.screens.ChatScreen;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.client.gui.screens.TitleScreen;
import org.lwjgl.glfw.GLFW;

/**
 * The overlay's smoke test for one Minecraft version (OVL-AUTO), run by Fabric's client game test API against the jar
 * players get: the jar loads (its mixins apply and its entry point runs, or the title screen never comes), a world opens,
 * the pause menu opens over the overlay's hooks, its key opens the panel (no launcher listens: its offline state) and
 * the emoji picker's key opens the chat with the picker. CI runs it before Studio may publish a jar; the screenshots go
 * with the report. Only Fabric's own test calls and the game's screen classes are used, so one source serves every
 * version.
 */
@SuppressWarnings("UnstableApiUsage")
public final class OverlaySmokeTest implements FabricClientGameTest {
    /** Ticks a screen draws before its picture is taken. */
    private static final int SETTLE_TICKS = 10;

    @Override
    public void runTest(ClientGameTestContext context) {
        context.waitForScreen(TitleScreen.class);
        context.takeScreenshot("overlay-title");
        try (TestSingleplayerContext world = context.worldBuilder().create()) {
            context.waitFor(mc -> mc.player != null && mc.level != null);
            context.waitTicks(40);
            context.takeScreenshot("overlay-world");

            context.getInput().pressKey(GLFW.GLFW_KEY_ESCAPE);
            context.waitForScreen(PauseScreen.class);
            context.waitTicks(SETTLE_TICKS);
            context.takeScreenshot("overlay-pause");
            context.setScreen(() -> null);

            context.getInput().pressKey(GLFW.GLFW_KEY_F7);
            context.waitForScreen(OverlayScreen.class);
            context.waitTicks(SETTLE_TICKS);
            context.takeScreenshot("overlay-panel");
            context.setScreen(() -> null);

            context.getInput().pressKey(GLFW.GLFW_KEY_F9);
            context.waitForScreen(ChatScreen.class);
            context.waitTicks(SETTLE_TICKS);
            context.takeScreenshot("overlay-emoji");
            context.setScreen(() -> null);
        }
    }
}
