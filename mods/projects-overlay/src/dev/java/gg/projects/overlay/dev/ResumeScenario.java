package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.connect.WorldResume;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.screen.v1.ScreenEvents;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.screens.DisconnectedScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Development-only check of "Devam et" into a world (OVL-FIX), enabled with {@code -Dprojects.overlay.dev.script=resume}.
 *
 * <p>Run {@code make} ({@code -Dprojects.overlay.dev.resume=make}): a flat test world {@value #WORLD} is created from the
 * title screen and kept (the game closes in it). Run {@code check} (the default) is started the way the launcher starts
 * "Devam et": either with the game's quick play ({@code -Povl.quickPlay=}{@value #WORLD}) or with the overlay's world
 * ({@code -Povl.world=}{@value #WORLD}), with or without other mods in the run's {@code mods} folder (Remove Reloading
 * Screen, Xaero's Minimap). It records when the first title screen frame came, when the world's server started, when
 * the world was on screen, takes a screenshot there and closes the game. A crash leaves no results file: the run's
 * {@code crash-reports} say why. A server's quick play to a closed port ({@code -Povl.quickPlayServer=}) ends on the
 * game's own error screen (the multiplayer check). Run {@code missing} names a world that is not there: the title screen
 * stays, with the overlay's note over it.
 */
public final class ResumeScenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("OVL-FIX resume scenario");
    private static final String WORLD = "ovl-resume";

    private final JsonObject results = new JsonObject();
    private final JsonArray events = new JsonArray();
    private final long runStarted = System.nanoTime();
    private String variant;
    private Path out;
    private boolean created;
    private int worldTicks;
    private int clientTicks;
    private int titleTicks;
    private boolean done;
    private Object lastScreen = "start";

    @Override
    public void onInitializeClient() {
        if (!"resume".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        variant = System.getProperty("projects.overlay.dev.resume", "check");
        out = Path.of(System.getProperty("projects.overlay.dev.out", "resume-out")).toAbsolutePath();
        results.addProperty("variant", variant);
        results.addProperty("minecraft", Compat.VERSION);
        // What the run passed as the launcher would (the overlay clears its own property once read: build.gradle gives the
        // scenario a copy).
        results.addProperty("overlayWorldArg", String.valueOf(System.getProperty("projects.overlay.dev.world")));
        results.addProperty("mods", FabricLoader.getInstance().getAllMods().size());
        results.addProperty("rrls", FabricLoader.getInstance().isModLoaded("rrls"));
        results.addProperty("xaerolib", FabricLoader.getInstance().isModLoaded("xaerolib"));
        ScreenEvents.AFTER_INIT.register((mc, screen, width, height) -> {
            if (screen instanceof TitleScreen) {
                event("title", "init");
                if (!results.has("framesAtTitleInit")) {
                    results.addProperty("framesAtTitleInit", WorldResume.frames());
                }
                ScreenEvents.afterExtract(screen).register((s, g, mouseX, mouseY, delta) -> {
                    if (!results.has("titleFrameAt")) {
                        results.addProperty("titleFrameAt", seconds());
                        results.addProperty("titleFrameClientTicks", clientTicks);
                    }
                });
            }
        });
        ServerLifecycleEvents.SERVER_STARTING.register(server -> {
            results.addProperty("serverStartingAt", seconds());
            results.addProperty("serverStartingBeforeTitleFrame", !results.has("titleFrameAt"));
            results.addProperty("serverStartingClientTicks", clientTicks);
            // Frames the game began before the world's server started: 0 is the crash's order (quick play inside the
            // game's start, Remove Reloading Screen).
            results.addProperty("framesAtServerStarting", WorldResume.frames());
            event("server", "starting");
        });
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("OVL-FIX resume scenario on (run {}); results in {}", variant, out);
    }

    private void tick(Minecraft mc) {
        if (done) {
            return;
        }
        clientTicks++;
        mc.options.pauseOnLostFocus = false;
        Screen screen = Compat.screen(mc);
        Object kind = screen == null ? "none" : screen.getClass().getSimpleName();
        if (!kind.equals(lastScreen)) {
            lastScreen = kind;
            event("screen", String.valueOf(kind));
        }
        if (screen instanceof DisconnectedScreen) {
            // A server quick play to nowhere (the multiplayer check): the game got as far as its own error screen.
            results.addProperty("disconnectedScreen", true);
            finish(mc);
            return;
        }
        if ("missing".equals(variant)) {
            // A world that is gone (-Povl.world= a name with no folder): the title screen stays with the overlay's note.
            // Counted from the title screen's own first frame, when the note shows (a second in).
            if (screen instanceof TitleScreen && results.has("titleFrameAt") && ++titleTicks == 20) {
                shot(mc, "missing-note");
            }
            if (titleTicks > 30 || mc.level != null || seconds() > 120) {
                results.addProperty("stayedOnTitle", mc.level == null && screen instanceof TitleScreen);
                finish(mc);
            }
            return;
        }
        if ("make".equals(variant) && !created) {
            if (screen instanceof TitleScreen && !Compat.overlayShowing(mc)) {
                created = true;
                if (mc.getLevelSource().levelExists(WORLD)) {
                    // Made by an earlier run: nothing to do.
                    event("world", "already there");
                    results.addProperty("inWorld", false);
                    finish(mc);
                    return;
                }
                event("world", "create");
                LevelSettings settings = new LevelSettings("OVL-FIX resume", GameType.CREATIVE,
                    new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
                    WorldDataConfiguration.DEFAULT);
                mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261025L, false, false),
                    DevCompat.flatWorld(), new TitleScreen());
            }
            return;
        }
        if (mc.level == null || mc.player == null || screen != null) {
            // Gave up waiting: a resume that never came (the results say so).
            if (seconds() > 240) {
                results.addProperty("timedOut", true);
                finish(mc);
            }
            return;
        }
        if (worldTicks++ == 0) {
            results.addProperty("worldAt", seconds());
            results.addProperty("worldLevel", mc.getSingleplayerServer() == null ? "?"
                : mc.getSingleplayerServer().getWorldData().getLevelName());
            event("world", "on screen");
        }
        if (worldTicks == 60) {
            mc.player.setXRot(20f);
            shot(mc, "in-world");
        }
        if (worldTicks >= 80) {
            results.addProperty("inWorld", true);
            finish(mc);
        }
    }

    private void finish(Minecraft mc) {
        if (done) {
            return;
        }
        done = true;
        results.addProperty("runSeconds", seconds());
        results.add("events", events);
        try {
            Files.createDirectories(out);
            Files.writeString(out.resolve("resume-" + variant + "-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
        mc.stop();
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("resume-%s-%s.png", variant, name);
        Screenshot.grab(mc.gameDirectory, file, DevCompat.mainTarget(mc), 1,
            message -> LOG.info("screenshot {}: {}", name, message.getString()));
        event("shot", file);
    }

    private double seconds() {
        return Math.round((System.nanoTime() - runStarted) / 1e6) / 1000.0;
    }

    private void event(String kind, String value) {
        JsonObject e = new JsonObject();
        e.addProperty("t", seconds());
        e.addProperty(kind, value);
        events.add(e);
        LOG.info("{}: {}", kind, value);
    }
}
