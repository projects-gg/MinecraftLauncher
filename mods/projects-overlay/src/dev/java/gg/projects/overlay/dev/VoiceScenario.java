package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.panel.OverlayScreen;
import gg.projects.overlay.voice.VoiceState;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
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
 * Development-only scenario for VOICE-O, enabled with {@code -Dprojects.overlay.dev.script=voice} and the mock launcher's
 * {@code --voice --people --people2 --lang tr}: the HUD's corner while PixelAvci speaks and after, the player's muted
 * microphone's sign, NetherKral's ring card, the panel with the ring and the room's strip, the room after "Katıl", the
 * room page, a person's menu (volume first) and its volume steps, a crew room of six with a weak connection and while it
 * reconnects, and the corner with five speakers. Everything through the overlay's own paths (frames from the mock
 * launcher, the overlay's presses); no OS input. Results and screenshots go to {@code -Dprojects.overlay.dev.out}.
 */
public final class VoiceScenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("VOICE-O scenario");
    private static final String WORLD = "voice-scenario";
    private static final String RING_ROOM = "c1111111111111111";

    private record Step(double after, String name, Consumer<Minecraft> action) {
    }

    private final List<Step> steps = new ArrayList<>();
    private final JsonObject results = new JsonObject();
    private final JsonArray events = new JsonArray();
    private final long runStarted = System.nanoTime();
    private Path out;
    private int next;
    private long stepAt = System.nanoTime();
    private int shots;
    private boolean started;
    private boolean done;
    private int worldTicks = -1;

    @Override
    public void onInitializeClient() {
        if (!"voice".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "voice-out")).toAbsolutePath();
        ToastLayer.pauseWhileAway = false;
        script();
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("VOICE-O scenario driver on; results in {}", out);
    }

    private void step(double wait, String name, Consumer<Minecraft> action) {
        steps.add(new Step(wait, name, action));
    }

    private void tick(Minecraft mc) {
        OverlayClient overlay = OverlayClient.get();
        if (overlay == null || done) {
            return;
        }
        mc.options.pauseOnLostFocus = false;
        if (!started) {
            started = startWorld(mc, overlay);
            stepAt = System.nanoTime();
            return;
        }
        if (next >= steps.size()) {
            finish(mc);
            return;
        }
        Step s = steps.get(next);
        if ((System.nanoTime() - stepAt) / 1e9 < s.after()) {
            return;
        }
        event("step", s.name());
        try {
            s.action().accept(mc);
        } catch (RuntimeException e) {
            LOG.error("step {} failed", s.name(), e);
            results.addProperty("failed", s.name() + ": " + e);
            finish(mc);
            return;
        }
        next++;
        stepAt = System.nanoTime();
    }

    private boolean startWorld(Minecraft mc, OverlayClient overlay) {
        Screen screen = Compat.screen(mc);
        if (worldTicks < 0) {
            boolean ready = screen instanceof TitleScreen && !Compat.overlayShowing(mc);
            if (ready && overlay.bridge().status() == BridgeClient.Status.ONLINE && overlay.voice().heard()) {
                openWorld(mc);
                worldTicks = 0;
            }
            return false;
        }
        if (mc.level == null || mc.player == null || screen != null) {
            return false;
        }
        if (++worldTicks < 100) {
            return false;
        }
        results.addProperty("minecraft", Compat.VERSION);
        results.addProperty("guiScale", mc.getWindow().getGuiScale());
        mc.player.setXRot(20f);
        return true;
    }

    private static OverlayScreen need(Minecraft mc) {
        if (Compat.screen(mc) instanceof OverlayScreen p) {
            return p;
        }
        throw new IllegalStateException("the panel is not open (" + Compat.screen(mc) + ")");
    }

    private void script() {
        // The corner: PixelAvci speaks, stops (a second, then out).
        step(0.5, "speak", mc -> dev("voice-speak"));
        step(0.5, "shot corner speaking", mc -> {
            results.addProperty("speakersAtFirst", OverlayClient.get().voice().speakers().toString());
            shot(mc, "corner-speaking");
        });
        step(0.1, "quiet", mc -> dev("voice-quiet"));
        step(0.6, "shot corner held", mc -> shot(mc, "corner-held"));
        step(1.2, "shot corner gone", mc -> shot(mc, "corner-gone"));
        step(0.1, "mute", mc -> OverlayClient.get().voiceMute(true));
        step(0.6, "shot muted sign", mc -> {
            VoiceState.Room room = OverlayClient.get().voice().room();
            results.addProperty("mutedAfterPress", room != null && room.me().mute());
            shot(mc, "corner-muted");
        });
        // A ring: its card, then the panel with it and the room's strip.
        step(0.1, "ring", mc -> dev("voice-ring"));
        step(0.8, "shot ring card", mc -> shot(mc, "ring-card"));
        step(0.1, "open", mc -> OverlayClient.get().togglePanel());
        step(0.6, "shot panel with the ring", mc -> shot(mc, "panel-ring-strip"));
        step(0.1, "join", mc -> OverlayClient.get().voiceAnswer(RING_ROOM, true));
        step(0.4, "shot joining", mc -> shot(mc, "panel-joining"));
        step(1.2, "shot joined", mc -> {
            VoiceState.Room room = OverlayClient.get().voice().room();
            results.addProperty("roomAfterJoin", room == null ? "none" : room.id());
            shot(mc, "panel-joined");
        });
        // The room page and a person's menu.
        step(0.1, "room page", mc -> need(mc).showPage(OverlayScreen.Page.VOICE));
        step(0.6, "shot room page", mc -> shot(mc, "room-page"));
        step(0.1, "speak on the page", mc -> dev("voice-speak"));
        step(0.4, "shot speaking ring", mc -> shot(mc, "room-page-speaking"));
        step(0.1, "menu", mc -> results.addProperty("menuOpened", need(mc).voiceMenu("f-ayse")));
        step(0.5, "shot person menu", mc -> shot(mc, "person-menu"));
        step(0.1, "close menu", mc -> need(mc).close());
        // A crew room of six (one weak), while it reconnects too.
        step(0.5, "crew room", mc -> {
            dev("voice-crew");
            OverlayClient.get().togglePanel();
        });
        step(0.6, "crew page", mc -> need(mc).showPage(OverlayScreen.Page.VOICE));
        step(0.6, "shot crew room", mc -> shot(mc, "crew-room"));
        step(0.1, "reconnecting", mc -> dev("voice-phase"));
        step(0.5, "shot reconnecting", mc -> shot(mc, "crew-reconnecting"));
        step(0.1, "back", mc -> need(mc).showPage(OverlayScreen.Page.CHATS));
        step(0.5, "shot strip reconnecting", mc -> shot(mc, "strip-reconnecting"));
        step(0.1, "close", mc -> need(mc).close());
        // The corner with five speakers at once.
        step(0.5, "five speak", mc -> devIds("voice-speak", "u-0", "u-1", "u-2", "u-3", "u-4"));
        step(0.6, "shot corner five", mc -> shot(mc, "corner-five"));
        step(0.1, "out", mc -> dev("voice-out"));
        step(0.8, "shot nothing", mc -> {
            results.addProperty("roomAtEnd", OverlayClient.get().voice().inRoom());
            shot(mc, "corner-out");
        });
    }

    private void dev(String cmd) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
        boolean sent = OverlayClient.get().bridge().send(o);
        event("dev", cmd + (sent ? "" : " (not sent)"));
    }

    private void devIds(String cmd, String... ids) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
        JsonArray list = new JsonArray();
        for (String id : ids) {
            list.add(id);
        }
        o.add("ids", list);
        boolean sent = OverlayClient.get().bridge().send(o);
        event("dev", cmd + (sent ? "" : " (not sent)"));
    }

    private void finish(Minecraft mc) {
        if (done) {
            return;
        }
        done = true;
        results.addProperty("runSeconds", Math.round((System.nanoTime() - runStarted) / 1e6) / 1000.0);
        results.add("events", events);
        try {
            Files.createDirectories(out);
            Files.writeString(out.resolve("voice-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
        mc.stop();
    }

    private void openWorld(Minecraft mc) {
        if (mc.getLevelSource().levelExists(WORLD)) {
            event("world", "open");
            mc.createWorldOpenFlows().openWorld(WORLD, () -> Compat.setScreen(mc, new TitleScreen()));
            return;
        }
        event("world", "create");
        LevelSettings settings = new LevelSettings("VOICE-O", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261102L, false, false),
            DevCompat.flatWorld(), new TitleScreen());
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("voice-%02d-%s.png", ++shots, name);
        Screenshot.grab(mc.gameDirectory, file, DevCompat.mainTarget(mc), 1,
            message -> LOG.info("screenshot {}: {}", name, message.getString()));
        event("shot", file);
    }

    private void event(String kind, String value) {
        JsonObject e = new JsonObject();
        e.addProperty("t", Math.round((System.nanoTime() - runStarted) / 1e6) / 1000.0);
        e.addProperty(kind, value);
        events.add(e);
        LOG.info("{}: {}", kind, value);
    }
}
