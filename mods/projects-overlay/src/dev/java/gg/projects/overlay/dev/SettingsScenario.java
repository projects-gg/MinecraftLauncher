package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.panel.OverlayScreen;
import gg.projects.overlay.ui.panel.PanelInput;
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
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.input.MouseButtonInfo;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Development-only scenario for OVL-FIX's settings changes, enabled with {@code -Dprojects.overlay.dev.script=settings}
 * and the mock launcher's {@code --chat2 --team6 --people --people2 --lang tr --lang-file <file>}: the end of the settings
 * page ("Hareket" with the help line under "Animasyonları azalt", "Kısayol", "Varsayılana dön"); the overlay key rebound
 * as the game's Controls would (the reset turns on); the reset from the keys (End, Enter) and its "Geri al" (Enter), then
 * from the pointer; the offer running out; the reduced motion row flipped from the keys; GUI 3; English at GUI 3 and 2.
 * Results and screenshots go to {@code -Dprojects.overlay.dev.out}.
 */
public final class SettingsScenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("OVL-FIX settings scenario");
    private static final String WORLD = "settings-scenario";

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
        if (!"settings".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "settings-out")).toAbsolutePath();
        ToastLayer.pauseWhileAway = false;
        script();
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("OVL-FIX settings scenario driver on; results in {}", out);
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
            if (ready && overlay.bridge().status() == BridgeClient.Status.ONLINE && overlay.state().welcomed()) {
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
        results.addProperty("keyAtStart", OverlayClient.get().key().saveString());
        mc.player.setXRot(25f);
        return true;
    }

    private static OverlayScreen need(Minecraft mc) {
        if (Compat.screen(mc) instanceof OverlayScreen p) {
            return p;
        }
        throw new IllegalStateException("the panel is not open (" + Compat.screen(mc) + ")");
    }

    private void script() {
        step(0.5, "open", mc -> OverlayClient.get().togglePanel());
        step(0.4, "settings", mc -> need(mc).showPage(OverlayScreen.Page.SETTINGS));
        step(0.5, "to the end", mc -> need(mc).settings().scrollToEnd());
        step(0.5, "shot end, all at their defaults", mc -> {
            results.addProperty("resetAvailableAtDefaults", need(mc).settings().resetAvailable());
            shot(mc, "end-defaults");
        });
        // The tester's case: only the key changes (as the game's Controls would bind it).
        step(0.1, "key to F8", mc -> OverlayClient.get().key().bind("key.keyboard.f8"));
        step(0.8, "shot key changed", mc -> {
            results.addProperty("keyAfterBind", OverlayClient.get().key().saveString());
            results.addProperty("resetAvailableAfterKey", need(mc).settings().resetAvailable());
            shot(mc, "end-key-changed");
        });
        // The keys: End chooses "Varsayılana dön", Enter presses it, Enter again is "Geri al".
        step(0.1, "choose the reset from the keys", mc -> {
            PanelInput.clearPointer();
            need(mc).setFocused(need(mc).settings());
            key(need(mc), InputConstants.KEY_END);
        });
        step(0.4, "shot reset chosen", mc -> shot(mc, "end-reset-chosen"));
        step(0.1, "enter", mc -> key(need(mc), InputConstants.KEY_RETURN));
        step(0.5, "shot after the reset", mc -> {
            results.addProperty("keyAfterKeysReset", OverlayClient.get().key().saveString());
            results.addProperty("keyDefaultAfterKeysReset", OverlayClient.get().key().isDefault());
            shot(mc, "end-reset-undo-offer");
        });
        step(0.1, "enter again: undo", mc -> key(need(mc), InputConstants.KEY_RETURN));
        step(0.5, "after the undo", mc -> {
            results.addProperty("keyAfterUndo", OverlayClient.get().key().saveString());
            shot(mc, "end-undone");
        });
        // The pointer.
        step(0.2, "click the reset", mc -> {
            float[] at = need(mc).settings().resetButton();
            if (at == null) {
                throw new IllegalStateException("no reset row");
            }
            PanelInput.pointAt(Math.round(at[0]), Math.round(at[1]));
            click(mc, at);
        });
        step(0.5, "shot after the click", mc -> {
            results.addProperty("keyAfterClick", OverlayClient.get().key().saveString());
            shot(mc, "end-reset-mouse");
            PanelInput.clearPointer();
        });
        step(6.6, "the offer ran out", mc -> {
            results.addProperty("resetAvailableAfterOffer", need(mc).settings().resetAvailable());
            shot(mc, "end-offer-gone");
        });
        // "Animasyonları azalt" from the keys: up from the reset passes the key row.
        step(0.1, "choose reduced motion", mc -> {
            need(mc).setFocused(need(mc).settings());
            key(need(mc), InputConstants.KEY_END);
            key(need(mc), InputConstants.KEY_UP);
            key(need(mc), InputConstants.KEY_UP);
            key(need(mc), InputConstants.KEY_RETURN);
        });
        step(0.5, "shot reduced motion on", mc -> {
            results.addProperty("reduceMotionAfterEnter", OverlayClient.get().settings().reduceMotion());
            results.addProperty("resetAvailableAfterMotion", need(mc).settings().resetAvailable());
            shot(mc, "end-motion-on");
        });
        step(0.1, "reduced motion off", mc -> key(need(mc), InputConstants.KEY_RETURN));
        step(0.4, "gui 3", mc -> {
            mc.options.guiScale().set(3);
            mc.resizeGui();
            need(mc).showPage(OverlayScreen.Page.SETTINGS);
        });
        step(0.4, "to the end at gui 3", mc -> need(mc).settings().scrollToEnd());
        step(0.5, "shot gui 3", mc -> shot(mc, "end-gui3"));
        step(0.1, "key to F8 at gui 3", mc -> OverlayClient.get().key().bind("key.keyboard.f8"));
        step(0.8, "shot gui 3 key changed", mc -> shot(mc, "end-gui3-key-changed"));
        // English: the "Dil" row (Home, then right), the panel built anew.
        step(0.2, "english", mc -> {
            need(mc).setFocused(need(mc).settings());
            key(need(mc), InputConstants.KEY_HOME);
            key(need(mc), InputConstants.KEY_RIGHT);
        });
        step(0.9, "english settings at gui 3", mc -> {
            results.addProperty("langAfterPick", Texts.current().code);
            need(mc).showPage(OverlayScreen.Page.SETTINGS);
            need(mc).settings().scrollToEnd();
        });
        step(0.5, "shot english gui 3", mc -> shot(mc, "end-en-gui3"));
        step(0.2, "gui 2", mc -> {
            mc.options.guiScale().set(2);
            mc.resizeGui();
            need(mc).showPage(OverlayScreen.Page.SETTINGS);
        });
        step(0.4, "to the end at gui 2", mc -> need(mc).settings().scrollToEnd());
        step(0.5, "shot english gui 2", mc -> shot(mc, "end-en-gui2"));
        // Back as it was: Turkish, F7.
        step(0.2, "turkish", mc -> {
            need(mc).setFocused(need(mc).settings());
            key(need(mc), InputConstants.KEY_HOME);
            key(need(mc), InputConstants.KEY_LEFT);
        });
        step(0.9, "f7 again", mc -> {
            OverlayClient.get().key().bind(null);
            results.addProperty("keyAtEnd", OverlayClient.get().key().saveString());
            results.addProperty("langAtEnd", Texts.current().code);
        });
        step(0.3, "close", mc -> need(mc).close());
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
            Files.writeString(out.resolve("settings-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
        mc.stop();
    }

    /** A key as the game would send it, through the panel's own handling. */
    private static void key(OverlayScreen panel, int key) {
        panel.keyPressed(new KeyEvent(key, 0, 0));
    }

    /** A left click at {@code at} through the screen's own handling (never the OS pointer). */
    private void click(Minecraft mc, float[] at) {
        Screen screen = Compat.screen(mc);
        if (screen == null) {
            throw new IllegalStateException("no screen to click");
        }
        MouseButtonEvent press = new MouseButtonEvent(Math.round(at[0]), Math.round(at[1]),
            new MouseButtonInfo(InputConstants.MOUSE_BUTTON_LEFT, 0));
        screen.mouseClicked(press, false);
        screen.mouseReleased(press);
        event("click", Math.round(at[0]) + "," + Math.round(at[1]));
    }

    private void openWorld(Minecraft mc) {
        if (mc.getLevelSource().levelExists(WORLD)) {
            event("world", "open");
            mc.createWorldOpenFlows().openWorld(WORLD, () -> Compat.setScreen(mc, new TitleScreen()));
            return;
        }
        event("world", "create");
        LevelSettings settings = new LevelSettings("OVL-FIX settings", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261025L, false, false),
            DevCompat.flatWorld(), new TitleScreen());
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("settings-%02d-%s.png", ++shots, name);
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
