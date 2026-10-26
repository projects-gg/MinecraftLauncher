package gg.projects.overlay.dev;

import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.host.HostController;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.panel.OverlayScreen;
import java.io.File;
import java.io.IOException;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.Arrays;
import java.util.Comparator;
import java.util.List;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.input.MouseButtonInfo;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Development only, for HOST1-E's end-to-end rig ({@code -Dprojects.overlay.dev.script=h1e}): the real game, started by
 * a real test launcher with this jar next to the overlay's, takes its steps from files, so the rig drives the host's
 * side without any OS input. Each {@code <out>/cmd/*.txt} (oldest name first) holds one command; it runs on the render
 * thread, the file goes and one JSON line goes to {@code <out>/events.jsonl}. {@code <out>/state.json} is rewritten
 * every second. Commands: {@code world <folder> [survival|creative] [flat|normal]}, {@code title}, {@code pause}, {@code page},
 * {@code panel}, {@code close}, {@code invite <friend>} (the overlay's {@link HostController#inviteFriend}),
 * {@code menu-invite <friend>} (the friend menu's own "Dünyana çağır" on the open panel), {@code share <mode>
 * <commands> <friend>...} (the world page's "Davet et"), {@code stop}, {@code remove <guest>}, {@code join <friend>},
 * {@code shot <name>}, {@code perf start|stop}, {@code chat <text>}, {@code cmd <command>}, {@code leave},
 * {@code quit}.
 */
public final class H1eScenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("HOST1-E scenario");

    private final long started = System.nanoTime();
    private Path out;
    private Path commands;
    private long stateAt;
    private int shots;
    private OverlayClient overlay;

    @Override
    public void onInitializeClient() {
        if (!"h1e".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "h1e-out")).toAbsolutePath();
        commands = out.resolve("cmd");
        try {
            Files.createDirectories(commands);
        } catch (IOException e) {
            LOG.warn("no command folder", e);
        }
        ToastLayer.pauseWhileAway = false;
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("HOST1-E scenario driver on; commands in {}", commands);
        event("start", "commands", commands.toString());
    }

    private void tick(Minecraft mc) {
        if (overlay == null) {
            overlay = OverlayClient.get();
            if (overlay == null) {
                return;
            }
        }
        long now = System.nanoTime();
        if (now - stateAt > 1_000_000_000L) {
            stateAt = now;
            writeState(mc);
        }
        File[] files = commands.toFile().listFiles((dir, name) -> name.endsWith(".txt"));
        if (files == null || files.length == 0) {
            return;
        }
        Arrays.sort(files, Comparator.comparing(File::getName));
        String line;
        try {
            line = Files.readString(files[0].toPath(), StandardCharsets.UTF_8).trim();
            Files.delete(files[0].toPath());
        } catch (IOException e) {
            return;
        }
        try {
            run(mc, line);
        } catch (RuntimeException e) {
            LOG.error("command failed: {}", line, e);
            event("error", "command", line, "error", e.toString());
        }
    }

    private void run(Minecraft mc, String line) {
        String[] w = line.split("\\s+");
        HostController host = overlay.host();
        switch (w[0]) {
            case "world" -> world(mc, w[1], w.length > 2 ? w[2] : "survival", w.length > 3 && "normal".equals(w[3]));
            case "title" -> Compat.setScreen(mc, new TitleScreen());
            case "pause" -> Compat.setScreen(mc, new PauseScreen(true));
            case "page" -> overlay.openWorldSharing(Compat.screen(mc));
            case "panel" -> {
                if (!(Compat.screen(mc) instanceof OverlayScreen)) {
                    overlay.togglePanel();
                }
            }
            case "close" -> Compat.setScreen(mc, null);
            case "invite" -> {
                HostController.WorldInvite r = host.inviteFriend(w[1]);
                event("invite", "friend", w[1], "result", r.name(), "line", String.valueOf(r.line()));
                return;
            }
            case "menu-invite" -> {
                Screen screen = Compat.screen(mc);
                if (!(screen instanceof OverlayScreen panel)) {
                    event("menu-invite", "error", "the panel is not open");
                    return;
                }
                try {
                    Method m = OverlayScreen.class.getDeclaredMethod("inviteFriend", String.class);
                    m.setAccessible(true);
                    m.invoke(panel, w[1]);
                } catch (ReflectiveOperationException e) {
                    event("menu-invite", "error", e.toString());
                    return;
                }
            }
            case "share" -> {
                boolean ok = host.invite(w[1], Boolean.parseBoolean(w[2]), List.of(Arrays.copyOfRange(w, 3, w.length)));
                event("share", "ok", String.valueOf(ok));
                return;
            }
            case "stop" -> host.stopSharing();
            case "remove" -> host.removeGuest(w[1]);
            case "join" -> overlay.joinWorld(w[1]);
            case "shot" -> {
                shot(mc, w.length > 1 ? w[1] : "shot");
                return;
            }
            case "perf" -> {
                if ("start".equals(w[1])) {
                    FrameClock.start();
                } else {
                    JsonObject summary = FrameClock.stop();
                    event("perf", "summary", summary.toString());
                    return;
                }
            }
            case "chat" -> {
                if (mc.player != null) {
                    mc.player.connection.sendChat(line.substring(5));
                }
            }
            case "cmd" -> {
                if (mc.player != null) {
                    mc.player.connection.sendCommand(line.substring(4));
                }
            }
            case "leave" -> leave(mc);
            case "quit" -> mc.stop();
            default -> {
                event("unknown", "command", line);
                return;
            }
        }
        event("done", "command", line);
    }

    private void world(Minecraft mc, String folder, String mode, boolean normal) {
        if (mc.level != null) {
            event("world", "error", "already in a world");
            return;
        }
        if (mc.getLevelSource().levelExists(folder)) {
            event("world", "open", folder);
            mc.createWorldOpenFlows().openWorld(folder, () -> Compat.setScreen(mc, new TitleScreen()));
            return;
        }
        event("world", "create", folder, "mode", mode);
        GameType type = "creative".equals(mode) ? GameType.CREATIVE : GameType.SURVIVAL;
        // Commands on for the host (the rig teleports guests to load chunks for the bandwidth measurement); guests get
        // the session's own rule whatever the world allows.
        LevelSettings settings = new LevelSettings(folder, type,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        // A normal world for the bandwidth measurement (real terrain to send), a flat one otherwise.
        mc.createWorldOpenFlows().createFreshLevel(folder, settings, new WorldOptions(20261024L, false, false),
            normal ? DevCompat.normalWorld() : DevCompat.flatWorld(), new TitleScreen());
    }

    /** The pause menu's own way back to the title screen (saving the world), pressed. */
    private void leave(Minecraft mc) {
        if (!(Compat.screen(mc) instanceof PauseScreen)) {
            Compat.setScreen(mc, new PauseScreen(true));
        }
        for (GuiEventListener child : Compat.screen(mc).children()) {
            if (child instanceof Button b && b.getMessage().getContents() instanceof TranslatableContents t
                && ("menu.returnToMenu".equals(t.getKey()) || "menu.disconnect".equals(t.getKey()))) {
                b.mouseClicked(new MouseButtonEvent(b.getX() + b.getWidth() / 2.0, b.getY() + b.getHeight() / 2.0,
                    new MouseButtonInfo(InputConstants.MOUSE_BUTTON_LEFT, 0)), false);
                return;
            }
        }
        event("leave", "error", "no button back to the title screen");
    }

    private void writeState(Minecraft mc) {
        JsonObject s = new JsonObject();
        s.addProperty("t", secs());
        Screen screen = Compat.screen(mc);
        s.addProperty("screen", screen == null ? "" : screen.getClass().getSimpleName());
        s.addProperty("inWorld", mc.level != null);
        s.addProperty("fps", mc.getFps());
        s.addProperty("bridge", String.valueOf(overlay.bridge().status()));
        HostController host = overlay.host();
        s.addProperty("canInvite", host.canInvite());
        s.addProperty("invitesToWorld", host.invitesToWorld());
        s.addProperty("hosting", host.hosting());
        s.addProperty("requesting", host.requesting());
        s.addProperty("failure", String.valueOf(host.failure()));
        IntegratedServer server = mc.getSingleplayerServer();
        s.addProperty("published", server != null && server.isPublished());
        s.addProperty("players", server == null ? 0 : server.getPlayerCount());
        // The integrated server's average tick (ms): what hosting costs the world while guests play.
        s.addProperty("mspt", server == null ? 0 : Math.round(server.getAverageTickTimeNanos() / 10_000.0) / 100.0);
        try {
            Files.writeString(out.resolve("state.json"), s.toString(), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.debug("state not written", e);
        }
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("h1e-%02d-%s.png", ++shots, name);
        Screenshot.grab(mc.gameDirectory, file, DevCompat.mainTarget(mc), 1,
            message -> LOG.info("screenshot {}: {}", name, message.getString()));
        event("shot", "file", file);
    }

    private void event(String kind, String... pairs) {
        JsonObject e = new JsonObject();
        e.addProperty("t", secs());
        e.addProperty("event", kind);
        for (int i = 0; i + 1 < pairs.length; i += 2) {
            e.addProperty(pairs[i], pairs[i + 1]);
        }
        LOG.info("{}", e);
        try {
            Files.writeString(out.resolve("events.jsonl"), e + "\n", StandardCharsets.UTF_8,
                StandardOpenOption.CREATE, StandardOpenOption.APPEND);
        } catch (IOException ex) {
            LOG.warn("event not written", ex);
        }
    }

    private double secs() {
        return Math.round((System.nanoTime() - started) / 1e6) / 1000.0;
    }
}
