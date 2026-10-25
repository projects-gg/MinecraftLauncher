package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
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
 * Development-only scenario for OVL-LANG, enabled with {@code -Dprojects.overlay.dev.script=lang} and the mock launcher's
 * {@code --chat2 --team6 --people --people2 --lang <launcher's> --lang-file <file>}.
 *
 * <p>Run {@code a} ({@code -Dprojects.overlay.dev.lang=a}, the default; the game in English, the launcher in Turkish, no
 * pick yet): the overlay starts in the launcher's language, not the game's; the settings' "Dil" row; a pick from the
 * keys rebuilds the panel in English where it stood; a toast and the lists in English; a pick with the mouse; the
 * launcher's own language changing while a pick holds (nothing) and after the pick is cleared (followed both ways); a
 * last pick of English for run {@code b}; crew tags with Turkish letters at GUI scales 1 to 3.
 *
 * <p>Run {@code b} (the next game, the mock greeting 3 s late): the overlay shows the kept pick from its first tick,
 * before the launcher said anything, and after. Results and screenshots go to {@code -Dprojects.overlay.dev.out}.
 */
public final class LangScenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("OVL-LANG scenario");
    private static final String WORLD = "lang-scenario";

    private record Step(double after, String name, Consumer<Minecraft> action) {
    }

    private final List<Step> steps = new ArrayList<>();
    private final JsonObject results = new JsonObject();
    private final JsonArray events = new JsonArray();
    private final long runStarted = System.nanoTime();
    private String variant;
    private Path out;
    private int next;
    private long stepAt = System.nanoTime();
    private int shots;
    private boolean started;
    private boolean firstTick = true;
    private boolean done;
    private OverlayScreen before;

    @Override
    public void onInitializeClient() {
        if (!"lang".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        variant = System.getProperty("projects.overlay.dev.lang", "a");
        out = Path.of(System.getProperty("projects.overlay.dev.out", "lang-out")).toAbsolutePath();
        ToastLayer.pauseWhileAway = false;
        if ("b".equals(variant)) {
            restartScript();
        } else {
            script();
        }
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("OVL-LANG scenario driver on (run {}); results in {}", variant, out);
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
        if (firstTick) {
            // Before the launcher said anything: the language the game started in (G2).
            firstTick = false;
            results.addProperty("variant", variant);
            results.addProperty("gameLanguage", mc.options.languageCode);
            results.addProperty("langAtFirstTick", Texts.current().code);
            results.addProperty("managedAtFirstTick", Texts.language().managed());
            results.addProperty("statusAtFirstTick", overlay.bridge().status().name());
        }
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

    private int worldTicks = -1;

    private boolean startWorld(Minecraft mc, OverlayClient overlay) {
        Screen screen = Compat.screen(mc);
        if (worldTicks < 0) {
            boolean ready = screen instanceof TitleScreen && !Compat.overlayShowing(mc);
            if (ready && overlay.bridge().status() == BridgeClient.Status.ONLINE && overlay.state().welcomed()) {
                results.addProperty("langAfterWelcome", Texts.current().code);
                results.addProperty("editableAfterWelcome", Texts.language().editable());
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
        mc.player.setXRot(25f);
        return true;
    }

    private static OverlayScreen panel(Minecraft mc) {
        return Compat.screen(mc) instanceof OverlayScreen p ? p : null;
    }

    private static OverlayScreen need(Minecraft mc) {
        OverlayScreen p = panel(mc);
        if (p == null) {
            throw new IllegalStateException("the panel is not open (" + Compat.screen(mc) + ")");
        }
        return p;
    }

    private void script() {
        // The launcher's language (Turkish) while the game speaks English: the settings' first row.
        step(0.5, "open", mc -> OverlayClient.get().togglePanel());
        step(0.4, "settings", mc -> need(mc).showPage(OverlayScreen.Page.SETTINGS));
        step(0.7, "shot settings tr", mc -> {
            shot(mc, "settings-tr");
            results.addProperty("langInPanel", Texts.current().code);
        });
        // The keys: the row chosen, then right: English, the panel built anew where it stood.
        step(0.1, "choose the row", mc -> {
            PanelInput.clearPointer();
            need(mc).setFocused(need(mc).settings());
            // Home: the list's first row from the keys, which is "Dil".
            key(need(mc), InputConstants.KEY_HOME);
            results.addProperty("rowShown", need(mc).settings().chooseLanguageRow());
        });
        step(0.4, "shot row from the keys", mc -> shot(mc, "settings-tr-row"));
        step(0.1, "right", mc -> {
            before = need(mc);
            key(need(mc), InputConstants.KEY_RIGHT);
        });
        step(0.15, "shot the change", mc -> shot(mc, "settings-changing"));
        step(0.7, "english", mc -> {
            results.addProperty("langAfterKeys", Texts.current().code);
            results.addProperty("rebuilt", panel(mc) != null && panel(mc) != before);
            results.addProperty("pageAfterKeys", String.valueOf(need(mc).page()));
            shot(mc, "settings-en");
        });
        // A toast, the friend list and crew chat in English.
        step(0.3, "close", mc -> need(mc).close());
        step(0.8, "sticker toast", mc -> dev("sticker-in", "conv", "dm:ayse", "from", "Ayşe", "sticker", "pl1/gg"));
        step(0.9, "shot toast en", mc -> shot(mc, "toast-en"));
        step(2.5, "open again", mc -> OverlayClient.get().togglePanel());
        step(0.5, "friends en", mc -> need(mc).showPage(OverlayScreen.Page.FRIENDS));
        step(0.6, "shot friends en", mc -> {
            shot(mc, "friends-en");
            results.addProperty("langAfterReopen", Texts.current().code);
        });
        step(0.1, "crew chat en", mc -> {
            need(mc).showPage(OverlayScreen.Page.CHATS);
            need(mc).showConversation("crew:gece");
        });
        step(0.8, "shot crew chat en", mc -> shot(mc, "crew-en"));
        // The mouse: "Türkçe".
        step(0.1, "settings again", mc -> need(mc).showPage(OverlayScreen.Page.SETTINGS));
        step(0.6, "click türkçe", mc -> {
            float[] at = need(mc).settings().languageOption(0);
            if (at == null) {
                throw new IllegalStateException("no Dil row");
            }
            PanelInput.pointAt(Math.round(at[0]), Math.round(at[1]));
            click(mc, at);
        });
        step(0.8, "turkish again", mc -> {
            results.addProperty("langAfterMouse", Texts.current().code);
            shot(mc, "settings-tr-mouse");
            PanelInput.clearPointer();
        });
        // The launcher's own language while a pick holds: nothing changes.
        step(0.2, "launcher english", mc -> dev("launcher-lang", "lang", "en"));
        step(0.8, "pick holds", mc -> results.addProperty("langWhilePicked", Texts.current().code));
        // Without a pick the overlay follows the launcher, both ways.
        step(0.2, "clear the pick", mc -> dev("clear-pick"));
        step(0.8, "follows english", mc -> {
            results.addProperty("langAfterClear", Texts.current().code);
            shot(mc, "settings-follow-en");
        });
        step(0.2, "launcher turkish", mc -> dev("launcher-lang", "lang", "tr"));
        step(0.8, "follows turkish", mc -> {
            results.addProperty("langFollowsBack", Texts.current().code);
            shot(mc, "settings-follow-tr");
        });
        // English picked again: the next game (run b) starts in it.
        step(0.6, "pick english for the next game", mc -> {
            need(mc).setFocused(need(mc).settings());
            key(need(mc), InputConstants.KEY_HOME);
            key(need(mc), InputConstants.KEY_RIGHT);
        });
        step(0.8, "picked", mc -> results.addProperty("langPickedLast", Texts.current().code));
        tags();
    }

    /**
     * Crew tags with Turkish letters (OVL-LANG's legibility check): Ayşe and Can wear "ĞÜÇ", Mert and the player "GKŞ";
     * the friend list and crew chat at GUI scales 1, 2 and 3.
     */
    private void tags() {
        step(0.2, "tag letters", mc -> dev("tag-text", "crew", "t-gk", "tag", "ĞÜÇ"));
        for (int scale : new int[] {1, 2, 3}) {
            step(0.6, "gui " + scale, mc -> {
                mc.options.guiScale().set(scale);
                mc.resizeGui();
                need(mc).showPage(OverlayScreen.Page.FRIENDS);
            });
            step(0.9, "shot tags friends gui " + scale, mc -> shot(mc, "tags-gui" + scale + "-friends"));
            step(0.1, "crew chat gui " + scale, mc -> {
                need(mc).showPage(OverlayScreen.Page.CHATS);
                need(mc).showConversation("crew:gece");
            });
            step(0.9, "shot tags crew gui " + scale, mc -> shot(mc, "tags-gui" + scale + "-crew"));
        }
        step(0.3, "gui 2", mc -> {
            mc.options.guiScale().set(2);
            mc.resizeGui();
        });
        step(0.3, "close", mc -> need(mc).close());
    }

    /** The next game: the kept pick from the first tick (checked there), the settings in it. */
    private void restartScript() {
        step(0.5, "open", mc -> OverlayClient.get().togglePanel());
        step(0.4, "settings", mc -> need(mc).showPage(OverlayScreen.Page.SETTINGS));
        step(0.7, "shot settings after restart", mc -> {
            shot(mc, "restart-settings");
            results.addProperty("langInPanel", Texts.current().code);
        });
        step(0.1, "friends", mc -> need(mc).showPage(OverlayScreen.Page.FRIENDS));
        step(0.7, "shot friends after restart", mc -> shot(mc, "restart-friends"));
        step(0.3, "close", mc -> need(mc).close());
    }

    private void finish(Minecraft mc) {
        if (done) {
            return;
        }
        done = true;
        results.addProperty("runSeconds", round((System.nanoTime() - runStarted) / 1e9));
        write();
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

    private void dev(String cmd, String... pairs) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
        for (int i = 0; i + 1 < pairs.length; i += 2) {
            o.addProperty(pairs[i], pairs[i + 1]);
        }
        boolean sent = OverlayClient.get().bridge().send(o);
        event("dev", cmd + (sent ? "" : " (not sent)"));
    }

    private void openWorld(Minecraft mc) {
        if (mc.getLevelSource().levelExists(WORLD)) {
            event("world", "open");
            mc.createWorldOpenFlows().openWorld(WORLD, () -> Compat.setScreen(mc, new TitleScreen()));
            return;
        }
        event("world", "create");
        LevelSettings settings = new LevelSettings("OVL-LANG scenario", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261025L, false, false),
            DevCompat.flatWorld(), new TitleScreen());
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("lang-%s-%02d-%s.png", variant, ++shots, name);
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

    private void write() {
        results.add("events", events);
        try {
            Files.createDirectories(out);
            Files.writeString(out.resolve("lang-" + variant + "-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
    }

    private static double round(double v) {
        return Math.round(v * 1000.0) / 1000.0;
    }
}
