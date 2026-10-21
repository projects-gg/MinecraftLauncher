package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.Perf;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.OverlayScreen;
import java.io.IOException;
import java.lang.management.ManagementFactory;
import java.lang.management.ThreadMXBean;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import org.lwjgl.glfw.GLFW;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Development-only scenario for the OVL1 spike, enabled with {@code -Dprojects.overlay.dev.script=bench}. It
 * opens a flat test world, measures frame times idle / with a toast / with the panel, types and sends a reply
 * through the panel's normal input path, asks the mock launcher to drop the bridge and checks the reconnect,
 * then the launcher's goodbye. Results and screenshots go to {@code -Dprojects.overlay.dev.out}.
 */
public final class SpikeDriver implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("OVL1 spike");
    private static final String WORLD = "ovl1-bench";
    private static final double MEASURE_S = 15.0;
    private static final String REPLY = "Selam! Geliyorum, 2 dk. Çıkıyorum şimdi (ğüşıöç)";

    private enum Step {
        TITLE, LOADING, SETTLE, BASELINE, DEMO, TOAST_BENCH, TOAST_FADE, PANEL_OPEN, PANEL_BENCH, TYPE,
        WAIT_REPLY, CLOSE, BASELINE_AFTER, DROP, OFFLINE_PANEL, WAIT_ONLINE, BYE, ENDED_PANEL, QUIT, DONE
    }

    private final JsonObject results = new JsonObject();
    private final JsonArray events = new JsonArray();
    private Path out;
    private Step step = Step.TITLE;
    private long stepStarted = System.nanoTime();
    private long runStarted = System.nanoTime();
    private double lastInject;
    private int serial;
    private long sentAt;
    private long dropAt;
    private long offlineAt;
    private boolean flag;
    private boolean secondFlag;
    private long[] bridgeCpuAtStart;

    @Override
    public void onInitializeClient() {
        if (!"bench".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "ovl1-out")).toAbsolutePath();
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("OVL1 spike driver on; results in {}", out);
    }

    private double secs() {
        return (System.nanoTime() - stepStarted) / 1e9;
    }

    private void go(Step next) {
        event("step", next.name());
        step = next;
        stepStarted = System.nanoTime();
        flag = false;
        secondFlag = false;
    }

    private void tick(Minecraft mc) {
        OverlayClient overlay = OverlayClient.get();
        if (overlay == null) {
            return;
        }
        mc.options.pauseOnLostFocus = false;
        BridgeClient.Status status = overlay.bridge().status();
        switch (step) {
            case TITLE -> {
                boolean ready = mc.screen instanceof TitleScreen && mc.getOverlay() == null;
                if (ready && secs() > 3 && (status == BridgeClient.Status.ONLINE || secs() > 30)) {
                    results.addProperty("bridgeOnlineAtTitle", status == BridgeClient.Status.ONLINE);
                    openWorld(mc);
                    go(Step.LOADING);
                }
            }
            case LOADING -> {
                if (mc.level != null && mc.player != null && mc.screen == null) {
                    results.addProperty("worldLoadSeconds", round(secs()));
                    go(Step.SETTLE);
                } else if (secs() > 180) {
                    fail(mc, "world did not load");
                }
            }
            case SETTLE -> {
                if (secs() > 6) {
                    Perf.drain();
                    bridgeCpuAtStart = bridgeCpu();
                    FrameClock.start();
                    go(Step.BASELINE);
                }
            }
            case BASELINE -> {
                if (secs() > MEASURE_S) {
                    results.add("idle", FrameClock.stop());
                    results.add("bridgeCpuIdleMs", cpuDelta(bridgeCpuAtStart, bridgeCpu()));
                    dev(overlay, "demo");
                    go(Step.DEMO);
                }
            }
            case DEMO -> {
                if (!flag && secs() > 1.6) {
                    shot(mc, "01-toast-message");
                    flag = true;
                }
                if (!secondFlag && secs() > 3.8) {
                    shot(mc, "02-toast-call");
                    secondFlag = true;
                }
                if (secs() > 11) {
                    Perf.drain();
                    FrameClock.start();
                    lastInject = -10;
                    go(Step.TOAST_BENCH);
                }
            }
            case TOAST_BENCH -> {
                if (secs() - lastInject > 1.5) {
                    lastInject = secs();
                    overlay.handleFrame(benchToast(++serial));
                }
                if (secs() > MEASURE_S) {
                    results.add("toast", FrameClock.stop());
                    results.add("toastRender", perf(Perf.drain(), 0));
                    go(Step.TOAST_FADE);
                }
            }
            case TOAST_FADE -> {
                if (secs() > 7) {
                    overlay.togglePanel();
                    go(Step.PANEL_OPEN);
                }
            }
            case PANEL_OPEN -> {
                if (secs() > 2.5) {
                    shot(mc, "03-panel");
                    Perf.drain();
                    FrameClock.start();
                    go(Step.PANEL_BENCH);
                }
            }
            case PANEL_BENCH -> {
                if (secs() > MEASURE_S) {
                    results.add("panel", FrameClock.stop());
                    results.add("panelRender", perf(Perf.drain(), 2));
                    go(Step.TYPE);
                }
            }
            case TYPE -> {
                if (mc.screen instanceof OverlayScreen screen) {
                    // Through the screen's normal key and character handlers, like a real keyboard.
                    REPLY.codePoints().forEach(cp -> screen.charTyped(new CharacterEvent(cp)));
                    String typed = screen.input().getValue();
                    results.addProperty("typedMatches", REPLY.equals(typed));
                    sentAt = System.nanoTime();
                    screen.keyPressed(new KeyEvent(GLFW.GLFW_KEY_ENTER, 0, 0));
                    OverlayState.CrewCall call = overlay.state().calls().stream().findFirst().orElse(null);
                    if (call != null) {
                        overlay.answer(call.id(), "coming");
                    }
                    go(Step.WAIT_REPLY);
                } else {
                    fail(mc, "panel not open");
                }
            }
            case WAIT_REPLY -> {
                OverlayScreen screen = mc.screen instanceof OverlayScreen s ? s : null;
                String conv = screen == null ? null : screen.openConversation();
                boolean acked = false;
                boolean replied = false;
                if (conv != null) {
                    for (OverlayState.ChatLine line : overlay.state().lines(conv)) {
                        if (line.mine && REPLY.equals(line.text) && line.delivery == OverlayState.Delivery.SENT) {
                            acked = true;
                        }
                        if (acked && !line.mine) {
                            replied = true;
                        }
                    }
                }
                if (acked && !results.has("sendAckMs")) {
                    results.addProperty("sendAckMs", round((System.nanoTime() - sentAt) / 1e6));
                }
                if (replied || secs() > 6) {
                    results.addProperty("replyArrived", replied);
                    results.addProperty("replyMs", round((System.nanoTime() - sentAt) / 1e6));
                    shot(mc, "04-panel-reply");
                    go(Step.CLOSE);
                }
            }
            case CLOSE -> {
                if (secs() > 1) {
                    overlay.togglePanel();
                    go(Step.BASELINE_AFTER);
                }
            }
            case BASELINE_AFTER -> {
                if (!flag && secs() > 3) {
                    FrameClock.start();
                    flag = true;
                }
                if (secs() > MEASURE_S + 3) {
                    results.add("idleAfter", FrameClock.stop());
                    JsonObject drop = Protocol.message("dev");
                    drop.addProperty("cmd", "drop");
                    drop.addProperty("ms", 4000);
                    overlay.bridge().send(drop);
                    dropAt = System.nanoTime();
                    go(Step.DROP);
                }
            }
            case DROP -> {
                if (status != BridgeClient.Status.ONLINE) {
                    offlineAt = System.nanoTime();
                    results.addProperty("dropNoticedMs", round((offlineAt - dropAt) / 1e6));
                    overlay.togglePanel();
                    go(Step.OFFLINE_PANEL);
                } else if (secs() > 10) {
                    fail(mc, "drop not noticed");
                }
            }
            case OFFLINE_PANEL -> {
                if (secs() > 1.2) {
                    shot(mc, "05-panel-offline");
                    overlay.togglePanel();
                    go(Step.WAIT_ONLINE);
                }
            }
            case WAIT_ONLINE -> {
                if (status == BridgeClient.Status.ONLINE && !overlay.state().friends().isEmpty()) {
                    results.addProperty("reconnectMs", round((System.nanoTime() - dropAt) / 1e6));
                    results.addProperty("friendsAfterReconnect", overlay.state().friends().size());
                    go(Step.BYE);
                } else if (secs() > 40) {
                    results.addProperty("reconnectMs", -1);
                    go(Step.BYE);
                }
            }
            case BYE -> {
                if (!flag && secs() > 2) {
                    dev(overlay, "bye");
                    flag = true;
                }
                if (status == BridgeClient.Status.ENDED) {
                    results.addProperty("endedReason", overlay.state().statusReason());
                    overlay.togglePanel();
                    go(Step.ENDED_PANEL);
                } else if (secs() > 12) {
                    fail(mc, "no goodbye");
                }
            }
            case ENDED_PANEL -> {
                if (secs() > 1.2) {
                    shot(mc, "06-panel-ended");
                    go(Step.QUIT);
                }
            }
            case QUIT -> {
                if (secs() > 1.5) {
                    results.add("bridgeCpuTotalMs", cpuDelta(new long[] {0, 0}, bridgeCpu()));
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

    private void openWorld(Minecraft mc) {
        if (mc.getLevelSource().levelExists(WORLD)) {
            event("world", "open");
            mc.createWorldOpenFlows().openWorld(WORLD, () -> mc.setScreen(new TitleScreen()));
            return;
        }
        event("world", "create");
        LevelSettings settings = new LevelSettings("OVL1 bench", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261021L, false, false),
            WorldPresets::createFlatWorldDimensions, new TitleScreen());
    }

    private static JsonObject benchToast(int n) {
        JsonObject o = Protocol.message("toast");
        o.addProperty("kind", "message");
        o.addProperty("key", "bench");
        o.addProperty("title", "Ayşe");
        o.addProperty("body", "Ölçüm mesajı " + n + ": akşam bedwars?");
        o.addProperty("conv", "dm:bench");
        return o;
    }

    private void dev(OverlayClient overlay, String cmd) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
        boolean sent = overlay.bridge().send(o);
        event("dev", cmd + (sent ? "" : " (not sent)"));
    }

    private void shot(Minecraft mc, String name) {
        Screenshot.grab(mc.gameDirectory, "ovl1-" + name + ".png", mc.getMainRenderTarget(), 1,
            message -> LOG.info("screenshot {}: {}", name, message.getString()));
        event("shot", name);
    }

    /** CPU time of the overlay's bridge threads in nanoseconds: {reader, writer}. */
    private static long[] bridgeCpu() {
        ThreadMXBean mx = ManagementFactory.getThreadMXBean();
        long reader = 0;
        long writer = 0;
        for (Thread t : Thread.getAllStackTraces().keySet()) {
            long cpu = mx.getThreadCpuTime(t.threadId());
            if (cpu < 0) {
                continue;
            }
            if (t.getName().equals("Projects Overlay bridge")) {
                reader += cpu;
            } else if (t.getName().equals("Projects Overlay bridge writer")) {
                writer += cpu;
            }
        }
        return new long[] {reader, writer};
    }

    private static JsonObject cpuDelta(long[] from, long[] to) {
        JsonObject o = new JsonObject();
        o.addProperty("reader", round((to[0] - from[0]) / 1e6));
        o.addProperty("writer", round((to[1] - from[1]) / 1e6));
        return o;
    }

    private static JsonObject perf(long[] drained, int offset) {
        JsonObject o = new JsonObject();
        long nanos = drained[offset];
        long calls = drained[offset + 1];
        o.addProperty("calls", calls);
        o.addProperty("avgMicros", calls == 0 ? 0 : round(nanos / 1e3 / calls));
        return o;
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
        LOG.error("spike failed: {}", why);
        write();
        go(Step.DONE);
        mc.stop();
    }

    private void write() {
        results.add("events", events);
        try {
            Files.createDirectories(out);
            Files.writeString(out.resolve("ovl1-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
    }

    private static double round(double v) {
        return Math.round(v * 1000.0) / 1000.0;
    }
}
