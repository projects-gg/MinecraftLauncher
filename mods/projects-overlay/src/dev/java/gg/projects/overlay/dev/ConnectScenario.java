package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.connect.ServerTarget;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.panel.OverlayScreen;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.screens.ConnectScreen;
import net.minecraft.client.gui.screens.LevelLoadingScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.core.BlockPos;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.WorldOptions;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Development-only check of the launcher's "Bağlan" in the game, and the smoke run for each Minecraft
 * version, enabled with {@code -Dprojects.overlay.dev.script=connect} and a local test server in
 * {@code -Dprojects.overlay.dev.server=host:port}. In a fresh singleplayer world: one toast, F7 opens and
 * closes the panel; then a gold block is placed (the last change), the pause menu opens (the player went to
 * the launcher), and the mock launcher sends {@code connect}. The travel toast shows over the pause menu,
 * the world is saved and left, the game connects to the test server (a second {@code connect} meanwhile
 * must be "busy"). On the server: {@code connect} to the same server is "already", to its other name a
 * switch. From the menus it goes at once. Last, the singleplayer world opens again and the block must be
 * there. The {@code connect-result} answers are in the mock launcher's log.
 */
public final class ConnectScenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("OVL2 connect scenario");
    private static final String WORLD = "ovl2-connect";

    private enum Step {
        TITLE, LOADING, SETTLE, TOAST, KEY_OPEN, KEY_CLOSE, MARK, PAUSE, TRAVEL, ARRIVED, ALREADY, SWITCH, SWITCHED,
        LEAVE, MENU_CONNECT, MENU_ARRIVED, LEAVE_AGAIN, REOPEN, CHECK, QUIT, DONE
    }

    private final JsonObject results = new JsonObject();
    private final JsonArray events = new JsonArray();
    private final long runStarted = System.nanoTime();
    private Path out;
    private ServerTarget server;
    private Step step = Step.TITLE;
    private long stepStarted = System.nanoTime();
    private int stage;
    private int shots;
    private BlockPos mark;
    private Object lastScreen = "start";
    private BridgeClient.Status lastStatus;
    private boolean sawConnectScreen;
    private boolean sawLoadingScreen;

    @Override
    public void onInitializeClient() {
        if (!"connect".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "ovl2-out")).toAbsolutePath();
        server = ServerTarget.parse(System.getProperty("projects.overlay.dev.server", "localhost:25599"));
        ToastLayer.pauseWhileAway = false;
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        results.addProperty("minecraft", Compat.VERSION);
        LOG.info("OVL2 connect scenario on ({}, server {}); results in {}", Compat.VERSION, server, out);
    }

    private double secs() {
        return (System.nanoTime() - stepStarted) / 1e9;
    }

    private void go(Step next) {
        event("step", next.name());
        step = next;
        stepStarted = System.nanoTime();
        stage = 0;
    }

    private boolean at(int index, double atSeconds) {
        if (stage == index && secs() >= atSeconds) {
            stage++;
            return true;
        }
        return false;
    }

    /** In a world with no screen up: arrived. */
    private static boolean playing(Minecraft mc, Screen screen) {
        return mc.level != null && mc.player != null && screen == null;
    }

    private static String serverIp(Minecraft mc) {
        ServerData data = mc.getCurrentServer();
        return data == null ? "" : data.ip;
    }

    private void tick(Minecraft mc) {
        OverlayClient overlay = OverlayClient.get();
        if (overlay == null) {
            return;
        }
        mc.options.pauseOnLostFocus = false;
        BridgeClient.Status status = overlay.bridge().status();
        Screen screen = Compat.screen(mc);
        if (screen != lastScreen) {
            lastScreen = screen;
            event("screen", screen == null ? "none" : screen.getClass().getSimpleName());
        }
        if (status != lastStatus) {
            lastStatus = status;
            event("bridge", String.valueOf(status));
        }
        switch (step) {
            case TITLE -> {
                boolean ready = screen instanceof TitleScreen && !Compat.overlayShowing(mc);
                if (ready && secs() > 3 && (status == BridgeClient.Status.ONLINE || secs() > 30)) {
                    createWorld(mc);
                    go(Step.LOADING);
                }
            }
            case LOADING -> {
                if (playing(mc, screen)) {
                    results.addProperty("worldLoadSeconds", round(secs()));
                    go(Step.SETTLE);
                } else if (secs() > 180) {
                    fail(mc, "world did not load");
                }
            }
            case SETTLE -> {
                if (secs() > 4) {
                    dev(overlay, "one");
                    go(Step.TOAST);
                }
            }
            case TOAST -> {
                if (at(0, 1.0)) {
                    shot(mc, "smoke-toast");
                }
                if (at(1, 1.4)) {
                    // F7 the way the game delivers it: a click on whatever mapping is bound to that key.
                    KeyMapping.click(Compat.KEYBOARD.getOrCreate(InputConstants.KEY_F7));
                    go(Step.KEY_OPEN);
                }
            }
            case KEY_OPEN -> {
                if (screen instanceof OverlayScreen panel && secs() > 0.8) {
                    shot(mc, "smoke-panel");
                    results.addProperty("f7Opens", true);
                    panel.keyPressed(new KeyEvent(InputConstants.KEY_F7, 0, 0));
                    go(Step.KEY_CLOSE);
                } else if (secs() > 5) {
                    fail(mc, "F7 did not open the panel");
                }
            }
            case KEY_CLOSE -> {
                if (screen == null && secs() > 0.5) {
                    results.addProperty("f7Closes", true);
                    go(Step.MARK);
                } else if (secs() > 5) {
                    fail(mc, "F7 did not close the panel");
                }
            }
            case MARK -> {
                if (at(0, 0.5)) {
                    // The last change before leaving: a gold block beside the player, set on the world's server.
                    IntegratedServer local = mc.getSingleplayerServer();
                    mark = mc.player.blockPosition().offset(3, 0, 0);
                    BlockPos pos = mark;
                    local.execute(() ->
                        local.overworld().setBlockAndUpdate(pos, Blocks.GOLD_BLOCK.defaultBlockState()));
                    mc.player.setYRot(-90f);
                    mc.player.setXRot(30f);
                    results.addProperty("mark", pos.toShortString());
                }
                if (at(1, 1.5)) {
                    BlockState placed = mc.level.getBlockState(mark);
                    results.addProperty("markPlaced", placed.is(Blocks.GOLD_BLOCK));
                    shot(mc, "connect-mark");
                    // The player went to the launcher: the game shows its pause menu.
                    mc.pauseGame(false);
                    go(Step.PAUSE);
                }
            }
            case PAUSE -> {
                if (at(0, 0.8)) {
                    devConnect(overlay, server.host(), server.port());
                    go(Step.TRAVEL);
                }
            }
            case TRAVEL -> {
                if (at(0, 0.3)) {
                    shot(mc, "connect-toast-over-pause");
                }
                if (screen instanceof ConnectScreen && !sawConnectScreen) {
                    sawConnectScreen = true;
                    results.addProperty("connectScreenAfterSeconds", round(secs()));
                    shot(mc, "connect-connecting");
                    // A second "Bağlan" while connecting: the answer must be busy.
                    devConnect(overlay, server.host(), server.port());
                }
                if (screen instanceof LevelLoadingScreen && !sawLoadingScreen) {
                    sawLoadingScreen = true;
                    shot(mc, "connect-loading");
                }
                if (playing(mc, screen) && !mc.hasSingleplayerServer() && mc.getCurrentServer() != null) {
                    results.addProperty("arrivedAfterSeconds", round(secs()));
                    results.addProperty("arrivedAt", serverIp(mc));
                    go(Step.ARRIVED);
                } else if (secs() > 60) {
                    fail(mc, "did not reach the test server");
                }
            }
            case ARRIVED -> {
                if (at(0, 0.06)) {
                    shot(mc, "connect-arrived");
                }
                if (at(1, 2.0)) {
                    devConnect(overlay, server.host(), server.port());
                    go(Step.ALREADY);
                }
            }
            case ALREADY -> {
                if (at(0, 1.0)) {
                    shot(mc, "connect-already");
                    results.addProperty("alreadyStayed", playing(mc, screen) && mc.getCurrentServer() != null);
                }
                if (at(1, 1.6)) {
                    // The test server's other name: a real switch from server to server.
                    devConnect(overlay, "127.0.0.1", server.port());
                    go(Step.SWITCH);
                }
            }
            case SWITCH -> {
                if (playing(mc, screen) && serverIp(mc).startsWith("127.0.0.1")) {
                    results.addProperty("switchSeconds", round(secs()));
                    go(Step.SWITCHED);
                } else if (secs() > 60) {
                    fail(mc, "server to server switch did not arrive");
                }
            }
            case SWITCHED -> {
                if (at(0, 1.0)) {
                    shot(mc, "connect-switched");
                    mc.disconnectFromWorld(ClientLevel.DEFAULT_QUIT_MESSAGE);
                    go(Step.LEAVE);
                }
            }
            case LEAVE -> {
                if (mc.level == null && screen != null && !(screen instanceof ConnectScreen) && secs() > 1.5) {
                    // From the menus the game goes at once, without a moment for the toast.
                    devConnect(overlay, server.host(), server.port());
                    go(Step.MENU_CONNECT);
                }
            }
            case MENU_CONNECT -> {
                if (at(0, 0.15)) {
                    shot(mc, "connect-from-menu");
                }
                if (playing(mc, screen) && mc.getCurrentServer() != null) {
                    results.addProperty("menuConnectSeconds", round(secs()));
                    go(Step.MENU_ARRIVED);
                } else if (secs() > 60) {
                    fail(mc, "connect from the menus did not arrive");
                }
            }
            case MENU_ARRIVED -> {
                if (at(0, 1.0)) {
                    mc.disconnectFromWorld(ClientLevel.DEFAULT_QUIT_MESSAGE);
                    go(Step.LEAVE_AGAIN);
                }
            }
            case LEAVE_AGAIN -> {
                if (mc.level == null && screen != null && secs() > 1.5) {
                    event("world", "open again");
                    mc.createWorldOpenFlows().openWorld(WORLD, () -> Compat.setScreen(mc, new TitleScreen()));
                    go(Step.REOPEN);
                }
            }
            case REOPEN -> {
                if (playing(mc, screen) && mc.hasSingleplayerServer()) {
                    go(Step.CHECK);
                } else if (secs() > 120) {
                    fail(mc, "world did not open again");
                }
            }
            case CHECK -> {
                if (stage == 0 && (mc.level.isLoaded(mark) || secs() > 10)) {
                    stage++;
                    BlockState found = mc.level.getBlockState(mark);
                    results.addProperty("worldSaved", found.is(Blocks.GOLD_BLOCK));
                    results.addProperty("blockAfterReopen", found.toString());
                }
                if (stage == 1 && secs() > 1.5) {
                    stage++;
                    shot(mc, "connect-world-reopened");
                    go(Step.QUIT);
                }
            }
            case QUIT -> {
                if (secs() > 1.0) {
                    results.addProperty("runSeconds", round((System.nanoTime() - runStarted) / 1e9));
                    write();
                    go(Step.DONE);
                    mc.stop();
                }
            }
            case DONE -> {
            }
        }
    }

    private void createWorld(Minecraft mc) {
        event("world", "create");
        LevelSettings settings = new LevelSettings("OVL2 connect", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261022L, false, false),
            DevCompat.flatWorld(), new TitleScreen());
    }

    private void dev(OverlayClient overlay, String cmd) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
        boolean sent = overlay.bridge().send(o);
        event("dev", cmd + (sent ? "" : " (not sent)"));
    }

    private void devConnect(OverlayClient overlay, String host, int port) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", "connect");
        o.addProperty("host", host);
        o.addProperty("port", port);
        boolean sent = overlay.bridge().send(o);
        event("dev", "connect " + host + ":" + port + (sent ? "" : " (not sent)"));
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("ovl2-%02d-%s.png", ++shots, name);
        Screenshot.grab(mc.gameDirectory, file, DevCompat.mainTarget(mc), 1,
            message -> LOG.info("screenshot {}: {}", name, message.getString()));
        event("shot", file);
    }

    private void event(String kind, String value) {
        JsonObject e = new JsonObject();
        e.addProperty("t", round((System.nanoTime() - runStarted) / 1e9));
        e.addProperty(kind, value);
        events.add(e);
        LOG.info("{}: {}", kind, value);
    }

    private void fail(Minecraft mc, String why) {
        results.addProperty("failed", why);
        LOG.error("connect scenario failed: {}", why);
        write();
        go(Step.DONE);
        mc.stop();
    }

    private void write() {
        results.add("events", events);
        try {
            Files.createDirectories(out);
            Files.writeString(out.resolve("ovl2-connect-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
    }

    private static double round(double v) {
        return Math.round(v * 1000.0) / 1000.0;
    }
}
