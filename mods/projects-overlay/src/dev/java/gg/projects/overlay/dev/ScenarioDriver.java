package gg.projects.overlay.dev;

import com.mojang.blaze3d.platform.InputConstants;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.Perf;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.PreparedLine;
import gg.projects.overlay.ui.draw.Draw;
import gg.projects.overlay.ui.panel.JoinScreen;
import gg.projects.overlay.ui.panel.OverlayScreen;
import java.io.IOException;
import java.lang.management.ManagementFactory;
import java.lang.management.ThreadMXBean;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.concurrent.CompletableFuture;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Development-only scenario for OVL2, enabled with {@code -Dprojects.overlay.dev.script=ovl2}: a flat test
 * world, frame times idle / with toasts moving / with three settled toasts / with the panel open (each runs
 * twice: the old drawing, every line laid out on every frame in the game's shared layer, and the new one, kept
 * layouts in the overlay's own layers; plus a frame of each with the world frozen, for a pixel comparison),
 * and screenshots of every state the overlay has (one toast, a stack of three with a fourth arriving, an update
 * in place, the panel's chat, sending and a refused message, friends, the join question, settings, closing,
 * offline, ended, English).
 * Everything goes through the overlay's normal paths (frames from the mock launcher, the panel's own key
 * and character handlers); no OS input. Results and screenshots go to {@code -Dprojects.overlay.dev.out}.
 */
public final class ScenarioDriver implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("OVL2 scenario");
    private static final String WORLD = "ovl2-scenario";
    private static final double MEASURE_S = 15.0;
    private static final String REPLY = "Geliyorum, 2 dk. Çıkıyorum şimdi (ğüşıöç)";

    private enum Step {
        TITLE, LOADING, SETTLE, BASELINE, ONE, ONE_CLEAR, STACK, STACK_CLEAR, NIGHT, NIGHT_CLEAR, TOAST_BENCH, BENCH_CLEAR,
        STEADY, STEADY_CLEAR, PANEL_OPEN, PANEL_BENCH, TYPE, LINK, FRIENDS, JOIN, SETTINGS, CLOSE, BASELINE_AFTER, DROP,
        OFFLINE_PANEL, WAIT_ONLINE, ENGLISH, ENGLISH_TOAST, ENGLISH_PANEL, BYE, ENDED_PANEL, QUIT, DONE
    }

    /** Slot and mode index of the old drawing and the new one in the interleaved measurements. */
    private static final int LEGACY = 0;
    private static final int NEW = 1;
    /** The toast bench brings a toast every this many seconds; each measured slice holds exactly one such cycle. */
    private static final double TOAST_EVERY_S = 1.2;
    private static final int TOAST_SLICES = 12;
    private static final int STEADY_SLICES = 6;
    private static final double STEADY_SLICE_S = 0.4;
    private static final int PANEL_SLICES = 10;
    private static final double PANEL_SLICE_S = 1.5;

    private final JsonObject results = new JsonObject();
    private final JsonArray events = new JsonArray();
    private Path out;
    private Step step = Step.TITLE;
    private long stepStarted = System.nanoTime();
    private final long runStarted = System.nanoTime();
    private double lastInject;
    private int serial;
    private int shots;
    private long dropAt;
    private long[] bridgeCpuAtStart;
    private CompletableFuture<Void> reload;
    private int stage;
    /** Interleaved measurement: slices done, when the next one starts, the layer counters per mode. */
    private int slice;
    private double nextSlice;
    private final long[][] perfByMode = new long[2][6];
    private Object lastScreen = "start";
    private BridgeClient.Status lastStatus;

    @Override
    public void onInitializeClient() {
        if (!"ovl2".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "ovl2-out")).toAbsolutePath();
        // The test window never takes the focus; toast time must still run.
        ToastLayer.pauseWhileAway = false;
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("OVL2 scenario driver on; results in {}", out);
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

    /** Runs {@code action} once when the step has run {@code at} seconds; stages run in order. */
    private boolean at(int index, double atSeconds) {
        if (stage == index && secs() >= atSeconds) {
            stage++;
            return true;
        }
        return false;
    }

    private void tick(Minecraft mc) {
        OverlayClient overlay = OverlayClient.get();
        if (overlay == null) {
            return;
        }
        mc.options.pauseOnLostFocus = false;
        BridgeClient.Status status = overlay.bridge().status();
        Screen screen = Compat.screen(mc);
        OverlayScreen panel = screen instanceof OverlayScreen s ? s : null;
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
                    results.addProperty("bridgeOnlineAtTitle", status == BridgeClient.Status.ONLINE);
                    openWorld(mc);
                    go(Step.LOADING);
                }
            }
            case LOADING -> {
                if (mc.level != null && mc.player != null && screen == null) {
                    results.addProperty("worldLoadSeconds", round(secs()));
                    go(Step.SETTLE);
                } else if (secs() > 180) {
                    fail(mc, "world did not load");
                }
            }
            case SETTLE -> {
                if (secs() > 6) {
                    results.addProperty("minecraft", Compat.VERSION);
                    results.addProperty("guiScale", mc.getWindow().getGuiScale());
                    results.addProperty("window", mc.getWindow().getWidth() + "x" + mc.getWindow().getHeight());
                    Perf.drain();
                    bridgeCpuAtStart = bridgeCpu();
                    FrameClock.start();
                    go(Step.BASELINE);
                }
            }
            case BASELINE -> {
                if (secs() > MEASURE_S) {
                    results.add("idle", FrameClock.stop());
                    results.add("idleOverlay", perf(Perf.drain(), 0));
                    results.add("bridgeCpuIdleMs", cpuDelta(bridgeCpuAtStart, bridgeCpu()));
                    dev(overlay, "one");
                    go(Step.ONE);
                }
            }
            case ONE -> {
                if (at(0, 0.18)) {
                    shot(mc, "toast-entering");
                }
                if (at(1, 1.0)) {
                    shot(mc, "toast-one");
                }
                if (secs() > 1.2) {
                    go(Step.ONE_CLEAR);
                }
            }
            case ONE_CLEAR -> {
                if (overlay.toasts().stack().isEmpty() && secs() > 1) {
                    // Bright: the toasts over the sky.
                    mc.player.setXRot(-40f);
                    dev(overlay, "stack");
                    go(Step.STACK);
                } else if (secs() > 15) {
                    fail(mc, "toast did not leave");
                }
            }
            case STACK -> {
                if (at(0, 1.6)) {
                    shot(mc, "bright-stack-three");
                    dev(overlay, "more");
                }
                if (at(1, 2.1)) {
                    shot(mc, "bright-stack-update-in-place");
                    dev(overlay, "fourth");
                }
                if (at(2, 2.23)) {
                    shot(mc, "bright-stack-arrival-moving");
                }
                if (at(3, 3.2)) {
                    shot(mc, "bright-stack-settled");
                    go(Step.STACK_CLEAR);
                }
            }
            case STACK_CLEAR -> {
                if (overlay.toasts().stack().isEmpty() && secs() > 1) {
                    results.addProperty("stackClearedSeconds", round(secs()));
                    // Dark: night, looking at the ground.
                    mc.player.connection.sendCommand("time set midnight");
                    mc.player.setXRot(25f);
                    go(Step.NIGHT);
                } else if (secs() > 40) {
                    fail(mc, "stack did not clear");
                }
            }
            case NIGHT -> {
                if (at(0, 2.0)) {
                    dev(overlay, "stack");
                }
                if (at(1, 3.6)) {
                    shot(mc, "dark-stack-three");
                    go(Step.NIGHT_CLEAR);
                }
            }
            case NIGHT_CLEAR -> {
                if (overlay.toasts().stack().isEmpty() && secs() > 1) {
                    // Noon, looking at the grass: the steady pixel pair comes much later, so by then the vignette
                    // has settled on the new brightness (it eases over seconds, ticks frozen or not).
                    mc.player.connection.sendCommand("time set noon");
                    mc.player.setXRot(70f);
                    lastInject = -10;
                    beginSlices(3 * TOAST_EVERY_S - 0.05);
                    go(Step.TOAST_BENCH);
                } else if (secs() > 40) {
                    fail(mc, "night stack did not clear");
                }
            }
            case TOAST_BENCH -> {
                // A toast every 1.2 s; once three are up, the old and the new drawing take turns, one cycle each.
                boolean done = slices(TOAST_SLICES, TOAST_EVERY_S);
                if (!done && secs() - lastInject >= TOAST_EVERY_S) {
                    lastInject = secs();
                    overlay.handleFrame(benchToast(++serial));
                }
                if (done) {
                    results.add("toastsLegacy", FrameClock.summary(LEGACY));
                    results.add("toastRenderLegacy", perf(perfByMode[LEGACY], 0, "legacy"));
                    results.add("toasts", FrameClock.summary(NEW));
                    results.add("toastRender", perf(perfByMode[NEW], 0, "new"));
                    go(Step.BENCH_CLEAR);
                }
            }
            case BENCH_CLEAR -> {
                if (overlay.toasts().stack().isEmpty() && secs() > 1) {
                    go(Step.STEADY);
                } else if (secs() > 40) {
                    fail(mc, "bench toasts did not clear");
                }
            }
            case STEADY -> {
                // Three toasts that stay put: what a settled stack costs per frame, both ways in turns, then one
                // frame of each way while the world stands still (ticks frozen) to compare pixels.
                for (int i = 0; i < STEADY_TOASTS.length; i++) {
                    if (at(i, 1.0 + 0.05 * i)) {
                        overlay.handleFrame(steadyToast(i));
                        if (i == STEADY_TOASTS.length - 1) {
                            beginSlices(2.0);
                        }
                    }
                }
                if (stage == 3 && slices(STEADY_SLICES, STEADY_SLICE_S)) {
                    stage++;
                    results.add("steady3Legacy", FrameClock.summary(LEGACY));
                    results.add("steady3RenderLegacy", perf(perfByMode[LEGACY], 0, "legacy"));
                    results.add("steady3", FrameClock.summary(NEW));
                    results.add("steady3Render", perf(perfByMode[NEW], 0, "new"));
                    results.addProperty("steadyVisible", overlay.toasts().stack().visibleCount());
                    mc.player.connection.sendCommand("tick freeze");
                    legacy(true);
                }
                if (at(4, 4.8)) {
                    shot(mc, "steady-legacy");
                    legacy(false);
                }
                if (at(5, 4.9)) {
                    shot(mc, "steady-new");
                    mc.player.connection.sendCommand("tick unfreeze");
                    go(Step.STEADY_CLEAR);
                }
            }
            case STEADY_CLEAR -> {
                if (overlay.toasts().stack().isEmpty() && secs() > 1) {
                    mc.player.setXRot(25f);
                    overlay.togglePanel();
                    go(Step.PANEL_OPEN);
                } else if (secs() > 40) {
                    fail(mc, "steady toasts did not clear");
                }
            }
            case PANEL_OPEN -> {
                if (at(0, 0.09)) {
                    shot(mc, "panel-opening");
                }
                if (at(1, 1.2)) {
                    shot(mc, "panel-chat");
                    beginSlices(0);
                    go(Step.PANEL_BENCH);
                }
            }
            case PANEL_BENCH -> {
                if (stage == 0 && slices(PANEL_SLICES, PANEL_SLICE_S)) {
                    stage++;
                    results.add("panelLegacy", FrameClock.summary(LEGACY));
                    results.add("panelRenderLegacy", perf(perfByMode[LEGACY], 2, "legacy"));
                    results.add("panel", FrameClock.summary(NEW));
                    results.add("panelRender", perf(perfByMode[NEW], 2, "new"));
                    mc.player.connection.sendCommand("tick freeze");
                    legacy(true);
                }
                if (at(1, PANEL_SLICES * PANEL_SLICE_S + 0.4)) {
                    shot(mc, "panel-legacy");
                    legacy(false);
                }
                if (at(2, PANEL_SLICES * PANEL_SLICE_S + 0.5)) {
                    shot(mc, "panel-new");
                    mc.player.connection.sendCommand("tick unfreeze");
                    go(Step.TYPE);
                }
            }
            case TYPE -> {
                if (panel == null) {
                    fail(mc, "panel not open");
                    return;
                }
                if (at(0, 0.1)) {
                    REPLY.codePoints().forEach(cp -> panel.charTyped(new CharacterEvent(cp)));
                    results.addProperty("typedMatches", REPLY.equals(panel.input().getValue()));
                    panel.keyPressed(new KeyEvent(InputConstants.KEY_RETURN, 0, 0));
                    dev(overlay, "edit");
                    dev(overlay, "delete");
                }
                if (at(1, 0.16)) {
                    shot(mc, "panel-sending");
                }
                if (at(2, 2.0)) {
                    shot(mc, "panel-reply");
                    go(Step.LINK);
                }
            }
            case LINK -> {
                if (panel == null) {
                    fail(mc, "panel not open");
                    return;
                }
                if (at(0, 0.1)) {
                    "Şuraya bak: site.gg".codePoints().forEach(cp -> panel.charTyped(new CharacterEvent(cp)));
                    panel.keyPressed(new KeyEvent(InputConstants.KEY_RETURN, 0, 0));
                }
                if (at(1, 0.9)) {
                    shot(mc, "panel-refused");
                    panel.showPage(OverlayScreen.Page.FRIENDS);
                    go(Step.FRIENDS);
                }
            }
            case FRIENDS -> {
                if (at(0, 0.7)) {
                    shot(mc, "panel-friends");
                    if (panel != null) {
                        panel.join("f-elif");
                    }
                    go(Step.JOIN);
                }
            }
            case JOIN -> {
                if (at(0, 0.7)) {
                    shot(mc, "join-question");
                    if (screen instanceof JoinScreen join) {
                        join.onClose();
                    }
                }
                if (at(1, 1.0) && screen instanceof OverlayScreen p) {
                    p.showPage(OverlayScreen.Page.SETTINGS);
                    go(Step.SETTINGS);
                }
            }
            case SETTINGS -> {
                if (panel == null) {
                    fail(mc, "panel not open");
                    return;
                }
                if (at(0, 0.8)) {
                    shot(mc, "settings");
                    panel.settings().mouseScrolled(panel.settings().getX() + 5, panel.settings().getY() + 5, 0, -12);
                }
                if (at(1, 1.6)) {
                    shot(mc, "settings-sound");
                    panel.showPage(OverlayScreen.Page.CHATS);
                    panel.close();
                    go(Step.CLOSE);
                }
            }
            case CLOSE -> {
                if (at(0, 0.05)) {
                    shot(mc, "panel-closing");
                }
                if (secs() > 3) {
                    FrameClock.start();
                    Perf.drain();
                    go(Step.BASELINE_AFTER);
                }
            }
            case BASELINE_AFTER -> {
                if (secs() > MEASURE_S) {
                    results.add("idleAfter", FrameClock.stop());
                    results.add("idleAfterOverlay", perf(Perf.drain(), 0));
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
                    results.addProperty("dropNoticedMs", round((System.nanoTime() - dropAt) / 1e6));
                    overlay.togglePanel();
                    go(Step.OFFLINE_PANEL);
                } else if (secs() > 10) {
                    fail(mc, "drop not noticed");
                }
            }
            case OFFLINE_PANEL -> {
                if (at(0, 1.0)) {
                    shot(mc, "panel-offline");
                    if (panel != null) {
                        panel.close();
                    }
                    go(Step.WAIT_ONLINE);
                }
            }
            case WAIT_ONLINE -> {
                if (status == BridgeClient.Status.ONLINE && !overlay.state().friends().isEmpty()) {
                    results.addProperty("reconnectMs", round((System.nanoTime() - dropAt) / 1e6));
                    go(Step.ENGLISH);
                } else if (secs() > 40) {
                    results.addProperty("reconnectMs", -1);
                    go(Step.ENGLISH);
                }
            }
            case ENGLISH -> {
                if (at(0, 0.5)) {
                    mc.player.connection.sendCommand("time set noon");
                    mc.player.setXRot(-40f);
                    mc.options.languageCode = "en_us";
                    mc.getLanguageManager().setSelected("en_us");
                    reload = mc.reloadResourcePacks();
                }
                if (stage == 1 && reload != null && reload.isDone() && !Compat.overlayShowing(mc)) {
                    stage++;
                    dev(overlay, "stack");
                    go(Step.ENGLISH_TOAST);
                }
            }
            case ENGLISH_TOAST -> {
                if (at(0, 1.6)) {
                    shot(mc, "en-stack");
                }
                if (at(1, 2.0)) {
                    overlay.togglePanel();
                    go(Step.ENGLISH_PANEL);
                }
            }
            case ENGLISH_PANEL -> {
                if (at(0, 1.0)) {
                    shot(mc, "en-panel");
                    if (panel != null) {
                        panel.showPage(OverlayScreen.Page.SETTINGS);
                    }
                }
                if (at(1, 1.8)) {
                    shot(mc, "en-settings");
                    if (screen instanceof OverlayScreen p) {
                        p.close();
                    }
                    go(Step.BYE);
                }
            }
            case BYE -> {
                if (at(0, 1.5)) {
                    dev(overlay, "bye");
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
                if (at(0, 1.0)) {
                    shot(mc, "panel-ended");
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

    /**
     * The old drawing (every line laid out on every frame, everything in the game's shared layer) or the new one
     * (kept layouts, the overlay's own layers): the measurements compare the two in the same run.
     */
    private static void legacy(boolean on) {
        PreparedLine.keepLayouts = !on;
        Draw.separateStrata = !on;
    }

    /** Prepares an interleaved measurement whose first slice starts {@code firstAt} seconds into the step. */
    private void beginSlices(double firstAt) {
        slice = 0;
        nextSlice = firstAt;
        for (long[] p : perfByMode) {
            Arrays.fill(p, 0L);
        }
        FrameClock.clear();
    }

    /**
     * Runs the old and the new drawing in turns (old first), {@code count} slices of {@code length} seconds, so
     * that both see the same moments of the scene and drift in the machine's load falls on both alike. Frame times
     * go to the mode's FrameClock slot, the layer's counters to {@link #perfByMode}. Returns true once, when the
     * last slice ends (the new drawing stays on).
     */
    private boolean slices(int count, double length) {
        if (secs() < nextSlice) {
            return false;
        }
        long[] drained = Perf.drain();
        if (slice > 0) {
            long[] into = perfByMode[(slice - 1) % 2];
            for (int i = 0; i < into.length; i++) {
                into[i] += drained[i];
            }
        }
        if (slice >= count) {
            FrameClock.record(-1);
            legacy(false);
            nextSlice = Double.MAX_VALUE;
            event("slice", "end");
            return true;
        }
        int mode = slice % 2;
        legacy(mode == LEGACY);
        FrameClock.record(mode);
        event("slice", mode == LEGACY ? "legacy" : "new");
        slice++;
        nextSlice += length;
        return false;
    }

    private void openWorld(Minecraft mc) {
        if (mc.getLevelSource().levelExists(WORLD)) {
            event("world", "open");
            mc.createWorldOpenFlows().openWorld(WORLD, () -> Compat.setScreen(mc, new TitleScreen()));
            return;
        }
        event("world", "create");
        LevelSettings settings = new LevelSettings("OVL2 scenario", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261022L, false, false),
            DevCompat.flatWorld(), new TitleScreen());
    }

    private static final String[][] BENCH = {
        {"dm:ayse", "Ayşe", "Ölçüm mesajı: akşam bedwars?"},
        {"dm:mert", "Mert", "Kılıcı sende mi bıraktım?"},
        {"dm:zeynep", "Zeynep", "Hadi gel, seni bekliyoruz."},
        {"crew:gece", "Deniz", "Ben de geliyorum, bekleyin!"},
    };

    private static final String[][] STEADY_TOASTS = {
        {"dm:selin", "Selin", "Bu akşam turnuvaya var mısın?"},
        {"crew:gece", "Deniz", "Ben de geliyorum, bekleyin!"},
        {"dm:kaan", "Kaan", "Haritayı açtım, köprünün orada buluşalım."},
    };

    private static JsonObject steadyToast(int i) {
        String[] pick = STEADY_TOASTS[i];
        JsonObject o = Protocol.message("toast");
        o.addProperty("kind", "message");
        o.addProperty("key", pick[0]);
        o.addProperty("title", pick[1]);
        o.addProperty("body", pick[2]);
        o.addProperty("conv", pick[0]);
        if (pick[0].startsWith("crew:")) {
            o.addProperty("crew", "Gece Kuşları");
        }
        return o;
    }

    private static JsonObject benchToast(int n) {
        String[] pick = BENCH[n % BENCH.length];
        JsonObject o = Protocol.message("toast");
        o.addProperty("kind", "message");
        o.addProperty("key", pick[0]);
        o.addProperty("title", pick[1]);
        o.addProperty("body", pick[2] + " (" + n + ")");
        o.addProperty("conv", pick[0]);
        if (pick[0].startsWith("crew:")) {
            o.addProperty("crew", "Gece Kuşları");
        }
        return o;
    }

    private void dev(OverlayClient overlay, String cmd) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
        boolean sent = overlay.bridge().send(o);
        event("dev", cmd + (sent ? "" : " (not sent)"));
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("ovl2-%02d-%s.png", ++shots, name);
        Screenshot.grab(mc.gameDirectory, file, DevCompat.mainTarget(mc), 1,
            message -> LOG.info("screenshot {}: {}", name, message.getString()));
        event("shot", file);
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
        return perf(drained, offset, PreparedLine.keepLayouts ? "new" : "legacy");
    }

    /** The layer's own time per call, and how many lines of text the game laid out (or reused) per call. */
    private static JsonObject perf(long[] drained, int offset, String drawing) {
        JsonObject o = new JsonObject();
        long nanos = drained[offset];
        long calls = drained[offset + 1];
        o.addProperty("calls", calls);
        o.addProperty("avgMicros", calls == 0 ? 0 : round(nanos / 1e3 / calls));
        o.addProperty("layoutsMadePerCall", calls == 0 ? 0 : round(drained[4] / (double) calls));
        o.addProperty("layoutsReusedPerCall", calls == 0 ? 0 : round(drained[5] / (double) calls));
        o.addProperty("drawing", drawing);
        return o;
    }

    private void event(String kind, String value) {
        JsonObject e = new JsonObject();
        e.addProperty("t", round((System.nanoTime() - runStarted) / 1e9));
        // Wall-clock time, to line steps up with a flight recording.
        e.addProperty("wall", System.currentTimeMillis());
        e.addProperty(kind, value);
        events.add(e);
        LOG.info("{}: {}", kind, value);
    }

    private void fail(Minecraft mc, String why) {
        results.addProperty("failed", why);
        LOG.error("scenario failed: {}", why);
        write();
        go(Step.DONE);
        mc.stop();
    }

    private void write() {
        results.add("events", events);
        try {
            Files.createDirectories(out);
            Files.writeString(out.resolve("ovl2-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
    }

    private static double round(double v) {
        return Math.round(v * 1000.0) / 1000.0;
    }
}
