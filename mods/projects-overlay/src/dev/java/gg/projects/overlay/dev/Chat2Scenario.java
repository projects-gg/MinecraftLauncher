package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.Perf;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.panel.LinkScreen;
import gg.projects.overlay.ui.panel.OverlayScreen;
import java.io.IOException;
import java.lang.management.ManagementFactory;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
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
 * Development-only scenario for CHAT2-O, enabled with {@code -Dprojects.overlay.dev.script=chat2} and the mock
 * launcher's {@code --chat2}: in a flat test world, frame times and the overlay's own time with the panel closed
 * (nothing may run), a sticker toast, then the panel on a conversation with every extra (a quoted message,
 * reactions, a link with its card, a sticker, "Görüldü") measured in turns against a plain one (frame time, the
 * panel's own time, bytes the render thread allocates per frame); then, with the keyboard only (no OS input),
 * a message picked, the quick reactions, a reaction given and one refused (its note), a reply written and sent,
 * the sticker picker and a sticker sent (its arrival motion), "yazıyor…", "Görüldü" moving, a friend's sticker
 * arriving, crew chat with three writers, the link question, English, and the panel closed again. Results and
 * screenshots go to {@code -Dprojects.overlay.dev.out}.
 */
public final class Chat2Scenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("CHAT2-O scenario");
    private static final String WORLD = "chat2-scenario";
    private static final double MEASURE_S = 10.0;
    private static final int SLICES = 8;
    private static final double SLICE_S = 2.5;
    private static final String RICH = "dm:ayse";
    private static final String PLAIN = "dm:mert";

    private enum Step {
        TITLE, LOADING, SETTLE, IDLE, TOAST, TOAST_CLEAR, OPEN, BENCH, KEYS, NOTE, REPLY, PICKER, TYPING, ARRIVAL,
        CREW, LINK, ENGLISH, ENGLISH_PANEL, CLOSE, IDLE_AFTER, QUIT, DONE
    }

    private final JsonObject results = new JsonObject();
    private final JsonArray events = new JsonArray();
    private final long runStarted = System.nanoTime();
    private Path out;
    private Step step = Step.TITLE;
    private long stepStarted = System.nanoTime();
    private int stage;
    private int shots;
    private int slice;
    private double sliceStart = -1;
    private boolean recording;
    private final long[][] perfByConv = new long[2][7];
    private final long[] allocByConv = new long[2];
    private final long[] framesByConv = new long[2];
    private long allocAtSlice;
    private int framesAtSlice;
    private CompletableFuture<Void> reload;
    private Object lastScreen = "start";
    private BridgeClient.Status lastStatus;

    @Override
    public void onInitializeClient() {
        if (!"chat2".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "chat2-out")).toAbsolutePath();
        ToastLayer.pauseWhileAway = false;
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("CHAT2-O scenario driver on; results in {}", out);
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
                    results.addProperty("chat2Features", overlay.state().has("stickers")
                        && overlay.state().has("reactions") && overlay.state().has("replies"));
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
                    mc.player.setXRot(25f);
                    Perf.drain();
                    FrameClock.start();
                    go(Step.IDLE);
                }
            }
            case IDLE -> {
                if (secs() > MEASURE_S) {
                    results.add("idle", FrameClock.stop());
                    results.add("idleOverlay", perf(Perf.drain()));
                    dev(overlay, "sticker-in", "conv", "dm:zeynep", "from", "Zeynep", "sticker", "pl1/love");
                    go(Step.TOAST);
                }
            }
            case TOAST -> {
                if (at(0, 0.2)) {
                    shot(mc, "toast-sticker-arriving");
                }
                if (at(1, 1.2)) {
                    shot(mc, "toast-sticker");
                    go(Step.TOAST_CLEAR);
                }
            }
            case TOAST_CLEAR -> {
                if (overlay.toasts().stack().isEmpty() && secs() > 0.5) {
                    overlay.togglePanel();
                    go(Step.OPEN);
                } else if (secs() > 20) {
                    fail(mc, "toast did not leave");
                }
            }
            case OPEN -> {
                if (panel == null) {
                    if (secs() > 3) {
                        fail(mc, "panel did not open");
                    }
                    return;
                }
                if (at(0, 0.5)) {
                    results.addProperty("openedOn", panel.openConversation());
                    shot(mc, "panel-from-sticker-toast");
                    panel.showConversation(RICH);
                }
                if (at(1, 1.6)) {
                    shot(mc, "dm-rich");
                    beginSlices();
                    go(Step.BENCH);
                }
            }
            case BENCH -> {
                if (panel == null) {
                    fail(mc, "panel not open");
                    return;
                }
                if (slices(panel)) {
                    results.add("panelPlain", FrameClock.summary(0));
                    results.add("panelPlainRender", perfOf(perfByConv[0], allocByConv[0], framesByConv[0]));
                    results.add("panelRich", FrameClock.summary(1));
                    results.add("panelRichRender", perfOf(perfByConv[1], allocByConv[1], framesByConv[1]));
                    panel.showConversation(RICH);
                    go(Step.KEYS);
                }
            }
            case KEYS -> {
                if (panel == null) {
                    fail(mc, "panel not open");
                    return;
                }
                if (at(0, 1.0)) {
                    panel.focusMessages();
                    key(panel, InputConstants.KEY_UP);
                    key(panel, InputConstants.KEY_UP);
                    key(panel, InputConstants.KEY_UP);
                }
                if (at(1, 1.3)) {
                    shot(mc, "keyboard-pick-toolbar");
                    key(panel, InputConstants.KEY_RETURN);
                }
                if (at(2, 1.6)) {
                    shot(mc, "quick-reactions");
                    key(panel, InputConstants.KEY_RIGHT);
                    key(panel, InputConstants.KEY_RETURN);
                }
                if (at(3, 2.3)) {
                    shot(mc, "reacted");
                    go(Step.NOTE);
                }
            }
            case NOTE -> {
                if (panel == null) {
                    fail(mc, "panel not open");
                    return;
                }
                if (at(0, 0.2)) {
                    key(panel, InputConstants.KEY_RETURN);
                    for (int i = 0; i < 5; i++) {
                        key(panel, InputConstants.KEY_RIGHT);
                    }
                    key(panel, InputConstants.KEY_RETURN);
                }
                if (at(1, 0.6)) {
                    shot(mc, "note-refused-reaction");
                    // A reaction only makes a bubble taller: no "Yeni mesajlar".
                    results.addProperty("pillAfterReactions", panel.newMessagesBelow());
                    go(Step.REPLY);
                }
            }
            case REPLY -> {
                if (panel == null) {
                    fail(mc, "panel not open");
                    return;
                }
                if (at(0, 3.5)) {
                    // The note is gone: open the quick strip and take its last cell, "Yanıtla".
                    key(panel, InputConstants.KEY_RETURN);
                    for (int i = 0; i < 6; i++) {
                        key(panel, InputConstants.KEY_RIGHT);
                    }
                    key(panel, InputConstants.KEY_RETURN);
                }
                if (at(1, 3.8)) {
                    "Kuralları okudum, akşam görüşürüz".codePoints()
                        .forEach(cp -> panel.charTyped(new CharacterEvent(cp)));
                }
                if (at(2, 4.1)) {
                    shot(mc, "replying");
                    // Writing the reply: the picked message (and its toolbar) let go.
                    results.addProperty("pickKeptWhileReplying", panel.messagePicked());
                    panel.keyPressed(new KeyEvent(InputConstants.KEY_RETURN, 0, 0));
                }
                if (at(3, 4.25)) {
                    shot(mc, "reply-sending");
                }
                if (at(4, 5.0)) {
                    shot(mc, "reply-sent");
                    go(Step.PICKER);
                }
            }
            case PICKER -> {
                if (panel == null) {
                    fail(mc, "panel not open");
                    return;
                }
                if (at(0, 0.3)) {
                    panel.toggleStickers();
                }
                if (at(1, 1.0)) {
                    shot(mc, "sticker-picker");
                    for (int i = 0; i < 3; i++) {
                        panel.keyPressed(new KeyEvent(InputConstants.KEY_RIGHT, 0, 0));
                    }
                }
                if (at(2, 1.3)) {
                    shot(mc, "sticker-picker-keys");
                    panel.keyPressed(new KeyEvent(InputConstants.KEY_RETURN, 0, 0));
                }
                if (at(3, 1.62)) {
                    shot(mc, "sticker-sent-arriving");
                }
                if (at(4, 2.6)) {
                    shot(mc, "sticker-sent");
                    go(Step.TYPING);
                }
            }
            case TYPING -> {
                if (at(0, 0.2)) {
                    dev(overlay, "typing", "conv", RICH);
                    dev(overlay, "seen", "conv", RICH);
                }
                if (at(1, 0.6)) {
                    shot(mc, "typing-and-seen");
                    go(Step.ARRIVAL);
                }
            }
            case ARRIVAL -> {
                if (at(0, 0.2)) {
                    dev(overlay, "sticker-in", "conv", RICH, "from", "Ayşe", "sticker", "pl1/party");
                }
                if (at(1, 0.45)) {
                    shot(mc, "sticker-in-moving");
                }
                if (at(2, 1.6)) {
                    shot(mc, "sticker-in");
                    // Ayşe writes again under her own sticker: the line keeps clear of it.
                    dev(overlay, "typing", "conv", RICH);
                }
                if (at(3, 2.1)) {
                    shot(mc, "typing-under-theirs");
                    if (panel != null) {
                        panel.showConversation("crew:gece");
                    }
                    go(Step.CREW);
                }
            }
            case CREW -> {
                if (at(0, 0.8)) {
                    dev(overlay, "typing", "conv", "crew:gece");
                }
                if (at(1, 1.2)) {
                    shot(mc, "crew-rich-typing");
                    if (panel != null) {
                        // Scrolled up: the list's edges fade into the panel.
                        panel.focusMessages();
                        key(panel, InputConstants.KEY_PAGEUP);
                    }
                }
                if (at(2, 2.0)) {
                    // The messages fade out above "yazıyor…" instead of passing under it.
                    shot(mc, "crew-scrolled-up");
                    // A message comes in while the player reads above: "Yeni mesajlar" (with reduced motion
                    // too), the line moves over it.
                    dev(overlay, "sticker-in", "conv", "crew:gece", "from", "Mert", "sticker", "pl1/gg");
                }
                if (at(3, 2.7)) {
                    shot(mc, "crew-new-below");
                    results.addProperty("newBelowShown", panel != null && panel.newMessagesBelow());
                    if (panel != null) {
                        key(panel, InputConstants.KEY_END);
                        panel.askLink("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
                    }
                    go(Step.LINK);
                }
            }
            case LINK -> {
                if (at(0, 0.6)) {
                    shot(mc, "link-question");
                    results.addProperty("linkQuestionShown", screen instanceof LinkScreen);
                    if (screen instanceof LinkScreen link) {
                        link.onClose();
                    }
                }
                if (at(1, 1.0)) {
                    if (screen instanceof OverlayScreen p) {
                        p.close();
                    }
                    go(Step.ENGLISH);
                }
            }
            case ENGLISH -> {
                if (at(0, 0.5)) {
                    mc.options.languageCode = "en_us";
                    mc.getLanguageManager().setSelected("en_us");
                    reload = mc.reloadResourcePacks();
                }
                if (stage == 1 && reload != null && reload.isDone() && !Compat.overlayShowing(mc) && secs() > 2) {
                    stage++;
                    overlay.togglePanel();
                    go(Step.ENGLISH_PANEL);
                }
            }
            case ENGLISH_PANEL -> {
                if (panel == null) {
                    if (secs() > 3) {
                        fail(mc, "English panel did not open");
                    }
                    return;
                }
                if (at(0, 0.3)) {
                    panel.showConversation(RICH);
                }
                if (at(1, 1.4)) {
                    shot(mc, "en-dm-rich");
                    panel.toggleStickers();
                }
                if (at(2, 2.0)) {
                    shot(mc, "en-sticker-picker");
                    panel.close();
                    go(Step.CLOSE);
                }
            }
            case CLOSE -> {
                if (secs() > 3) {
                    Perf.drain();
                    FrameClock.start();
                    go(Step.IDLE_AFTER);
                }
            }
            case IDLE_AFTER -> {
                if (secs() > MEASURE_S) {
                    results.add("idleAfter", FrameClock.stop());
                    results.add("idleAfterOverlay", perf(Perf.drain()));
                    go(Step.QUIT);
                }
            }
            case QUIT -> {
                if (secs() > 1) {
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

    private static void key(OverlayScreen panel, int key) {
        panel.keyPressed(new KeyEvent(key, 0, 0));
    }

    private void beginSlices() {
        slice = 0;
        sliceStart = -1;
        recording = false;
        for (long[] p : perfByConv) {
            java.util.Arrays.fill(p, 0L);
        }
        java.util.Arrays.fill(allocByConv, 0L);
        java.util.Arrays.fill(framesByConv, 0L);
        FrameClock.clear();
    }

    /**
     * The plain conversation and the rich one take turns ({@link #SLICE_S} each, plain first), so both see the
     * same moments of the world and the machine; each slice's first 0.4 s (the history arriving, the layout) is
     * left out. Returns true once the last slice ended.
     */
    private boolean slices(OverlayScreen panel) {
        double t = secs();
        int mode = slice % 2;
        if (sliceStart < 0) {
            panel.showConversation(mode == 0 ? PLAIN : RICH);
            sliceStart = t;
            return false;
        }
        if (!recording && t >= sliceStart + 0.4) {
            Perf.drain();
            allocAtSlice = allocated();
            framesAtSlice = FrameClock.count(mode);
            FrameClock.record(mode);
            recording = true;
            event("slice", mode == 0 ? "plain" : "rich");
            return false;
        }
        if (recording && t >= sliceStart + SLICE_S) {
            FrameClock.record(-1);
            long[] drained = Perf.drain();
            for (int i = 0; i < Math.min(drained.length, perfByConv[mode].length); i++) {
                perfByConv[mode][i] += drained[i];
            }
            allocByConv[mode] += allocated() - allocAtSlice;
            framesByConv[mode] += FrameClock.count(mode) - framesAtSlice;
            recording = false;
            sliceStart = -1;
            slice++;
            return slice >= 2 * SLICES;
        }
        return false;
    }

    /** Bytes this (the render) thread has allocated so far. */
    private static long allocated() {
        if (ManagementFactory.getThreadMXBean() instanceof com.sun.management.ThreadMXBean bean) {
            return bean.getCurrentThreadAllocatedBytes();
        }
        return 0L;
    }

    private void openWorld(Minecraft mc) {
        if (mc.getLevelSource().levelExists(WORLD)) {
            event("world", "open");
            mc.createWorldOpenFlows().openWorld(WORLD, () -> Compat.setScreen(mc, new TitleScreen()));
            return;
        }
        event("world", "create");
        LevelSettings settings = new LevelSettings("CHAT2-O scenario", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261023L, false, false),
            DevCompat.flatWorld(), new TitleScreen());
    }

    private void dev(OverlayClient overlay, String cmd, String... pairs) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
        for (int i = 0; i + 1 < pairs.length; i += 2) {
            o.addProperty(pairs[i], pairs[i + 1]);
        }
        boolean sent = overlay.bridge().send(o);
        event("dev", cmd + (sent ? "" : " (not sent)"));
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("chat2-%02d-%s.png", ++shots, name);
        Screenshot.grab(mc.gameDirectory, file, DevCompat.mainTarget(mc), 1,
            message -> LOG.info("screenshot {}: {}", name, message.getString()));
        event("shot", file);
    }

    private static JsonObject perf(long[] drained) {
        JsonObject o = new JsonObject();
        o.addProperty("toastCalls", drained[1]);
        o.addProperty("toastAvgMicros", drained[1] == 0 ? 0 : round(drained[0] / 1e3 / drained[1]));
        o.addProperty("panelCalls", drained[3]);
        o.addProperty("panelAvgMicros", drained[3] == 0 ? 0 : round(drained[2] / 1e3 / drained[3]));
        return o;
    }

    /** The panel's own time per frame and the render thread's allocations per frame in a set of slices. */
    private static JsonObject perfOf(long[] drained, long allocatedBytes, long frames) {
        JsonObject o = perf(drained);
        o.addProperty("layoutsMadePerCall", drained[3] == 0 ? 0 : round(drained[4] / (double) drained[3]));
        o.addProperty("layoutsReusedPerCall", drained[3] == 0 ? 0 : round(drained[5] / (double) drained[3]));
        // What the panel's own drawing allocates (its code and the game's record for each element it submits).
        o.addProperty("panelBytesPerCall", drained[3] == 0 ? 0 : Math.round(drained[6] / (double) drained[3]));
        o.addProperty("frames", frames);
        o.addProperty("renderThreadBytesPerFrame", frames == 0 ? 0 : Math.round(allocatedBytes / (double) frames));
        return o;
    }

    private void event(String kind, String value) {
        JsonObject e = new JsonObject();
        e.addProperty("t", round((System.nanoTime() - runStarted) / 1e9));
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
            Files.writeString(out.resolve("chat2-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
    }

    private static double round(double v) {
        return Math.round(v * 1000.0) / 1000.0;
    }
}
