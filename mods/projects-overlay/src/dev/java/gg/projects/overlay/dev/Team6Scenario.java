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
import gg.projects.overlay.ui.TagChip;
import gg.projects.overlay.ui.panel.OverlayScreen;
import gg.projects.overlay.ui.panel.PanelInput;
import java.io.IOException;
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
import net.minecraft.client.input.KeyEvent;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Development-only scenario for TEAM6-O, enabled with {@code -Dprojects.overlay.dev.script=team6} and the mock
 * launcher's {@code --chat2 --team6}: in a flat test world, the overlay's own time with the panel closed (nothing may
 * run), three toasts with crew tags and three without (the toasts' time), then the panel: friends with their tags, a
 * tag's bubble under a pointer the driver rests on the chip (never the OS pointer) and from one chip to the next, the
 * bubble from the keys, crew chat with tags before the names and an "ekibe katıldı" line, the bubble there, a line
 * arriving live, a crewmate taking another tag, the sticker picker on a short screen, a reply (no quick replies under
 * it), a crew hidden by its leader, Studio's switch off and on, English, and the panel closed again. Results and
 * screenshots go to {@code -Dprojects.overlay.dev.out}.
 */
public final class Team6Scenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("TEAM6-O scenario");
    private static final String WORLD = "team6-scenario";
    private static final double MEASURE_S = 6.0;
    private static final String CREW = "crew:gece";

    private enum Step {
        TITLE, LOADING, SETTLE, IDLE, TOAST_TAGS, TOAST_GAP, TOAST_PLAIN, TOAST_CLEAR, OPEN, BUBBLE_MOUSE,
        BUBBLE_KEYS, CREW_CHAT, CREW_BUBBLE, LIVE, PICKER, REPLY, HIDDEN, SWITCH, ENGLISH, ENGLISH_PANEL, CLOSE,
        IDLE_AFTER, QUIT, DONE
    }

    private final JsonObject results = new JsonObject();
    private final JsonArray events = new JsonArray();
    private final long runStarted = System.nanoTime();
    private Path out;
    private Step step = Step.TITLE;
    private long stepStarted = System.nanoTime();
    private int stage;
    private int shots;
    private CompletableFuture<Void> reload;
    private int slice;
    private double sliceAt;
    private final long[] toastNanos = new long[2];
    private final long[] toastCalls = new long[2];
    private Object lastScreen = "start";
    private BridgeClient.Status lastStatus;

    @Override
    public void onInitializeClient() {
        if (!"team6".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "team6-out")).toAbsolutePath();
        ToastLayer.pauseWhileAway = false;
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("TEAM6-O scenario driver on; results in {}", out);
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
                    results.addProperty("tagsOn", overlay.state().tagsOn());
                    results.addProperty("cards", overlay.state().badge("t-gk") != null);
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
                    // Three toasts at once, two of them with a tag ("[GK] Ayşe", "[GKŞ] Mert").
                    dev(overlay, "one");
                    dev(overlay, "crew-in");
                    dev(overlay, "crew-in", "from", "Deniz", "text", "Geliyorum, 5 dk");
                    go(Step.TOAST_TAGS);
                }
            }
            case TOAST_TAGS -> {
                if (at(0, 0.06)) {
                    shot(mc, "toasts-arriving");
                }
                if (at(1, 0.9)) {
                    shot(mc, "toasts-tags");
                    Perf.drain();
                    slice = 0;
                    sliceAt = secs();
                }
                if (stage == 2) {
                    // The same three cards sitting on screen, the chips drawn and left out in turns (0.3 s each, the
                    // same moments of the world and the machine for both): what the chips cost.
                    if (secs() >= sliceAt + 0.3) {
                        long[] drained = Perf.drain();
                        int mode = slice % 2;
                        toastNanos[mode] += drained[0];
                        toastCalls[mode] += drained[1];
                        slice++;
                        sliceAt = secs();
                        TagChip.drawn = slice % 2 == 0;
                    }
                    if (slice >= 12) {
                        TagChip.drawn = true;
                        results.add("toastsChips", toastSlices());
                        stage++;
                        go(Step.TOAST_GAP);
                    }
                }
            }
            case TOAST_GAP -> {
                if (overlay.toasts().stack().isEmpty() && secs() > 0.5) {
                    // The same three again without tags (Studio's switch off), for the cost of the chip.
                    dev(overlay, "tags-off");
                    go(Step.TOAST_PLAIN);
                } else if (secs() > 20) {
                    fail(mc, "toasts did not leave");
                }
            }
            case TOAST_PLAIN -> {
                if (at(0, 0.4)) {
                    dev(overlay, "one");
                    dev(overlay, "crew-in");
                    dev(overlay, "crew-in", "from", "Deniz", "text", "Geliyorum, 5 dk");
                }
                if (at(1, 1.3)) {
                    shot(mc, "toasts-tags-off");
                    Perf.drain();
                }
                if (at(2, 4.8)) {
                    results.add("toastsWithoutTags", perf(Perf.drain()));
                    dev(overlay, "tags-on");
                    go(Step.TOAST_CLEAR);
                }
            }
            case TOAST_CLEAR -> {
                if (overlay.toasts().stack().isEmpty() && secs() > 0.8) {
                    overlay.togglePanel();
                    go(Step.OPEN);
                } else if (secs() > 20) {
                    fail(mc, "toasts did not leave");
                }
            }
            case OPEN -> {
                if (panel == null) {
                    if (secs() > 3) {
                        fail(mc, "panel did not open");
                    }
                    return;
                }
                if (at(0, 0.4)) {
                    panel.showPage(OverlayScreen.Page.FRIENDS);
                }
                if (at(1, 1.2)) {
                    shot(mc, "friends-tags");
                    Perf.drain();
                }
                if (at(2, 4.2)) {
                    results.add("panelFriends", perf(Perf.drain()));
                    pointAt(panel.friendChip("f-ayse"));
                    go(Step.BUBBLE_MOUSE);
                }
            }
            case BUBBLE_MOUSE -> {
                if (panel == null) {
                    fail(mc, "panel not open");
                    return;
                }
                if (at(0, 0.15)) {
                    // Not yet: the pointer has to rest a moment.
                    shot(mc, "bubble-waiting");
                }
                if (at(1, 0.75)) {
                    shot(mc, "bubble-friend");
                    // To the next chip: the bubble goes there at once.
                    pointAt(panel.friendChip("f-mert"));
                }
                if (at(2, 0.9)) {
                    shot(mc, "bubble-next-chip");
                    PanelInput.clearPointer();
                }
                if (at(3, 1.4)) {
                    results.addProperty("bubbleClosedAfterLeaving", true);
                    shot(mc, "bubble-closed");
                    panel.focusFriends();
                    key(panel, InputConstants.KEY_DOWN);
                    go(Step.BUBBLE_KEYS);
                }
            }
            case BUBBLE_KEYS -> {
                if (panel == null) {
                    fail(mc, "panel not open");
                    return;
                }
                if (at(0, 0.3)) {
                    shot(mc, "bubble-keys-waiting");
                }
                if (at(1, 0.95)) {
                    shot(mc, "bubble-keys");
                    panel.showPage(OverlayScreen.Page.CHATS);
                    panel.showConversation(CREW);
                    go(Step.CREW_CHAT);
                }
            }
            case CREW_CHAT -> {
                if (panel == null) {
                    fail(mc, "panel not open");
                    return;
                }
                if (at(0, 1.2)) {
                    shot(mc, "crew-tags");
                    results.addProperty("quickRepliesShown", panel.quickRepliesShowing());
                    Perf.drain();
                }
                if (at(1, 4.2)) {
                    results.add("panelCrew", perf(Perf.drain()));
                    // The newest sender's chip: an older group may sit under the list's top fade by now.
                    float[] chip = panel.senderChip("Deniz");
                    pointAt(chip != null ? chip : panel.senderChip("Mert"));
                    go(Step.CREW_BUBBLE);
                }
            }
            case CREW_BUBBLE -> {
                if (at(0, 0.75)) {
                    shot(mc, "bubble-crew");
                    PanelInput.clearPointer();
                }
                if (at(1, 1.0)) {
                    dev(overlay, "joined", "name", "Kaan");
                }
                if (at(2, 1.5)) {
                    shot(mc, "joined-live");
                    dev(overlay, "tag-change");
                }
                if (at(3, 2.2)) {
                    shot(mc, "tag-changed");
                    go(Step.PICKER);
                }
            }
            case PICKER -> {
                if (panel == null) {
                    fail(mc, "panel not open");
                    return;
                }
                if (at(0, 0.2)) {
                    // A crew call (its card over the chat) and two recent stickers (the picker's first row): the
                    // shortest the conversation gets.
                    overlay.updateSettings(s -> s.withStickerUsed("pl1/gg").withStickerUsed("pl1/love"));
                    dev(overlay, "call");
                }
                if (at(1, 0.9)) {
                    shot(mc, "crew-with-call");
                    results.addProperty("quickRepliesWithCall", panel.quickRepliesShowing());
                    panel.toggleStickers();
                }
                if (at(2, 1.5)) {
                    shot(mc, "picker");
                    for (int i = 0; i < 4; i++) {
                        key(panel, InputConstants.KEY_DOWN);
                    }
                }
                if (at(3, 1.9)) {
                    shot(mc, "picker-down");
                    key(panel, InputConstants.KEY_ESCAPE);
                    // A reply: the quick replies step aside, as in the launcher.
                    panel.focusMessages();
                    key(panel, InputConstants.KEY_UP);
                    key(panel, InputConstants.KEY_RETURN);
                }
                if (at(4, 2.3)) {
                    for (int i = 0; i < 6; i++) {
                        key(panel, InputConstants.KEY_RIGHT);
                    }
                    key(panel, InputConstants.KEY_RETURN);
                    go(Step.REPLY);
                }
            }
            case REPLY -> {
                if (panel == null) {
                    fail(mc, "panel not open");
                    return;
                }
                if (at(0, 0.5)) {
                    shot(mc, "replying-no-quick");
                    results.addProperty("quickRepliesWhileReplying", panel.quickRepliesShowing());
                    key(panel, InputConstants.KEY_ESCAPE);
                    panel.showPage(OverlayScreen.Page.FRIENDS);
                    dev(overlay, "hide-crew");
                    go(Step.HIDDEN);
                }
            }
            case HIDDEN -> {
                if (at(0, 0.8)) {
                    // Ayşe's and Can's crew was hidden by its leader: their chips are gone.
                    shot(mc, "friends-crew-hidden");
                    dev(overlay, "tags-off");
                    go(Step.SWITCH);
                }
            }
            case SWITCH -> {
                if (at(0, 0.8)) {
                    shot(mc, "friends-tags-off");
                    results.addProperty("tagsOffSeen", !overlay.state().tagsOn());
                    dev(overlay, "tags-on");
                }
                if (at(1, 1.6)) {
                    if (panel != null) {
                        panel.close();
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
                    panel.showPage(OverlayScreen.Page.FRIENDS);
                }
                if (at(1, 0.9)) {
                    pointAt(panel.friendChip("f-deniz"));
                }
                if (at(2, 1.6)) {
                    shot(mc, "en-bubble");
                    PanelInput.clearPointer();
                    panel.showPage(OverlayScreen.Page.CHATS);
                    panel.showConversation(CREW);
                }
                if (at(3, 2.8)) {
                    shot(mc, "en-crew");
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

    /** Rests the driver's pointer on a chip (the panel draws as if the mouse were there). */
    private void pointAt(float[] center) {
        if (center == null) {
            event("point", "no chip");
            return;
        }
        PanelInput.pointAt(Math.round(center[0]), Math.round(center[1]));
        event("point", Math.round(center[0]) + "," + Math.round(center[1]));
    }

    private static void key(OverlayScreen panel, int key) {
        panel.keyPressed(new KeyEvent(key, 0, 0));
    }

    private void openWorld(Minecraft mc) {
        if (mc.getLevelSource().levelExists(WORLD)) {
            event("world", "open");
            mc.createWorldOpenFlows().openWorld(WORLD, () -> Compat.setScreen(mc, new TitleScreen()));
            return;
        }
        event("world", "create");
        LevelSettings settings = new LevelSettings("TEAM6-O scenario", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261024L, false, false),
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
        String file = String.format("team6-%02d-%s.png", ++shots, name);
        Screenshot.grab(mc.gameDirectory, file, DevCompat.mainTarget(mc), 1,
            message -> LOG.info("screenshot {}: {}", name, message.getString()));
        event("shot", file);
    }

    /** The toasts' own time per frame with the chips drawn (mode 0) and left out (mode 1). */
    private JsonObject toastSlices() {
        JsonObject o = new JsonObject();
        o.addProperty("chipsDrawnMicros", toastCalls[0] == 0 ? 0 : round(toastNanos[0] / 1e3 / toastCalls[0]));
        o.addProperty("chipsLeftOutMicros", toastCalls[1] == 0 ? 0 : round(toastNanos[1] / 1e3 / toastCalls[1]));
        o.addProperty("frames", toastCalls[0] + toastCalls[1]);
        return o;
    }

    private static JsonObject perf(long[] drained) {
        JsonObject o = new JsonObject();
        o.addProperty("toastCalls", drained[1]);
        o.addProperty("toastAvgMicros", drained[1] == 0 ? 0 : round(drained[0] / 1e3 / drained[1]));
        o.addProperty("panelCalls", drained[3]);
        o.addProperty("panelAvgMicros", drained[3] == 0 ? 0 : round(drained[2] / 1e3 / drained[3]));
        o.addProperty("layoutsMadePerCall", drained[3] == 0 ? 0 : round(drained[4] / (double) drained[3]));
        o.addProperty("layoutsReusedPerCall", drained[3] == 0 ? 0 : round(drained[5] / (double) drained[3]));
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
            Files.writeString(out.resolve("team6-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
    }

    private static double round(double v) {
        return Math.round(v * 1000.0) / 1000.0;
    }
}
