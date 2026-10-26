package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.chat.ChatEmojiBar;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.panel.OverlayScreen;
import gg.projects.overlay.ui.panel.PanelInput;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.function.Consumer;
import java.util.function.Predicate;
import java.util.stream.Stream;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.minecraft.SharedConstants;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.screens.ChatScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.metadata.pack.PackFormat;
import net.minecraft.server.packs.repository.PackRepository;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Development-only scenario for the chat's emoji picker (CHAT-EMOJI), with {@code -Dprojects.overlay.dev.script=emoji}
 * and the mock launcher's {@code --emojis}: the chat screen's button and its tooltip; the picker in a singleplayer world
 * (the unicode set), from the pointer and the keys (arrows, Enter writes at the caret); the same world shown as Projects
 * ({@link ChatEmojiBar#devHost}): the server's codes, a rank in the name line, the shortcuts; the server's resource pack
 * loaded and gone again (a test pack with the server's chat pictures from {@code -Dprojects.overlay.dev.packart}, as a
 * move from the lobby to Survival and back would load and drop it): its pictures, then the looks again; the line's length
 * near 256; GUI 3. With the panel ({@code -Dprojects.overlay.dev.emoji=panel}) also the settings' "Sohbette emoji seçici"
 * row, switched from the keys (the button goes) and back. Results and screenshots go to
 * {@code -Dprojects.overlay.dev.out}.
 */
public final class EmojiScenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("CHAT-EMOJI scenario");
    private static final String WORLD = "emoji-scenario";
    private static final String PACK = "projects-emoji-test";
    /** The server's chat pictures (ItemsAdder pro-emojis:chat_<name>) and their characters, as its unicode cache has them. */
    private static final String[][] PICTURES = {
        {"hmm", "e106"}, {"asik", "e107"}, {"zengin", "e108"}, {"agla", "e109"}, {"lol", "e10a"},
        {"kizgin", "e10b"}, {"havali", "e10c"}, {"sok", "e10d"}, {"uyku", "e10e"}, {"selam", "e10f"},
    };

    /** A step: its action runs {@code after} seconds after the last one, once {@code ready} holds (at most 30 s more). */
    private record Step(double after, String name, Consumer<Minecraft> action, Predicate<Minecraft> ready) {
    }

    /** How long a step waits for its {@code ready} beyond its own delay. */
    private static final double READY_WAIT = 30;
    /** Ticks the view's far chunks get to load and render before the first picture. */
    private static final int VIEW_SETTLE_TICKS = 300;

    private final List<Step> steps = new ArrayList<>();
    private final JsonObject results = new JsonObject();
    private final JsonArray events = new JsonArray();
    private final long runStarted = System.nanoTime();
    private Path out;
    private boolean panel;
    /**
     * The owner's screenshot setup (26 Oct): a normal world seen from {@code x,y,z,yaw,pitch}
     * ({@code -Dprojects.overlay.dev.emoji.view}, the run brings Iris, Complementary and a far render distance), or null:
     * the flat test world.
     */
    private String view;
    /** The view's time of day in ticks ({@code -Dprojects.overlay.dev.emoji.time}). */
    private String time;
    private int next;
    private long stepAt = System.nanoTime();
    private int shots;
    private boolean started;
    private boolean done;
    private int worldTicks = -1;

    @Override
    public void onInitializeClient() {
        if (!"emoji".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "emoji-out")).toAbsolutePath();
        panel = "panel".equals(System.getProperty("projects.overlay.dev.emoji"));
        view = System.getProperty("projects.overlay.dev.emoji.view");
        time = System.getProperty("projects.overlay.dev.emoji.time", "1000");
        ToastLayer.pauseWhileAway = false;
        script();
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("CHAT-EMOJI scenario driver on ({}); results in {}", panel ? "panel" : "picker alone", out);
    }

    private void step(double wait, String name, Consumer<Minecraft> action) {
        steps.add(new Step(wait, name, action, null));
    }

    /** A step that also waits until the game is done reloading its resources (a pack came or went). */
    private void stepAfterReload(double wait, String name, Consumer<Minecraft> action) {
        steps.add(new Step(wait, name, action, mc -> !Compat.overlayShowing(mc)));
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
        double since = (System.nanoTime() - stepAt) / 1e9;
        if (since < s.after()) {
            return;
        }
        if (s.ready() != null && !s.ready().test(mc) && since < s.after() + READY_WAIT) {
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
            if (ready && overlay.bridge().status() == BridgeClient.Status.ONLINE && overlay.emojis().heard()) {
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
        if (view != null) {
            if (worldTicks == 100) {
                placeView(mc);
            }
            // The far chunks around the view load and render (with shaders) before the first picture.
            if (worldTicks < 100 + VIEW_SETTLE_TICKS) {
                return false;
            }
        }
        results.addProperty("minecraft", Compat.VERSION);
        results.addProperty("guiScale", mc.getWindow().getGuiScale());
        results.addProperty("panel", overlay.panel());
        results.addProperty("emojiOnAtStart", overlay.emojis().on());
        results.addProperty("projectsEmojis", overlay.emojis().projects().emojis().size());
        if (view == null) {
            mc.player.setXRot(-12f);
        }
        return true;
    }

    /**
     * The view for the pictures: clear weather and a fixed time of day, the player flying at {@code view}. The integrated
     * server runs the commands with no answer in chat (the pictures show the chat).
     */
    private void placeView(Minecraft mc) {
        String[] v = view.split(",");
        serverCommands(mc, "gamerule advance_time false", "gamerule advance_weather false", "weather clear",
            "time set " + time, "tp @p " + String.join(" ", v));
        if (mc.player != null) {
            mc.player.getAbilities().flying = true;
            mc.player.onUpdateAbilities();
        }
        event("view", view);
    }

    private static void serverCommands(Minecraft mc, String... commands) {
        IntegratedServer server = mc.getSingleplayerServer();
        if (server == null) {
            return;
        }
        server.executeIfPossible(() -> {
            CommandSourceStack source = server.createCommandSourceStack().withSuppressedOutput();
            for (String command : commands) {
                server.getCommands().performPrefixedCommand(source, command);
            }
        });
    }

    private static ChatEmojiBar bar() {
        ChatEmojiBar bar = ChatEmojiBar.last();
        if (bar == null) {
            throw new IllegalStateException("no chat emoji bar");
        }
        return bar;
    }

    private static void openChat(Minecraft mc) {
        Compat.setScreen(mc, new ChatScreen("", false));
    }

    /** Points at the middle of a box (x, y, w, h). */
    private static void pointAt(float[] box) {
        if (box == null) {
            throw new IllegalStateException("nothing to point at");
        }
        PanelInput.pointAt(Math.round(box[0] + box[2] / 2f), Math.round(box[1] + box[3] / 2f));
    }

    /** Points at the first of these emojis the picker shows (the game's font may lack one of the unicode set). */
    private void pointAtFirst(String... codes) {
        for (String code : codes) {
            float[] box = bar().emojiBox(code);
            if (box != null) {
                pointAt(box);
                event("point", code);
                return;
            }
        }
        throw new IllegalStateException("none of the emojis is shown");
    }

    private static float[] buttonBox() {
        int[] b = bar().buttonBox();
        if (b == null) {
            throw new IllegalStateException("the button is not shown");
        }
        return new float[] {b[0], b[1], b[2], b[3]};
    }

    private void script() {
        if (panel) {
            panelScript();
            return;
        }
        step(0.5, "chat", EmojiScenario::openChat);
        step(0.8, "shot chat button", mc -> {
            results.addProperty("buttonShown", bar().buttonBox() != null);
            shot(mc, "chat-button");
        });
        step(0.1, "point at the button", mc -> pointAt(buttonBox()));
        step(0.8, "shot tooltip", mc -> shot(mc, "button-tip"));
        step(0.1, "press", mc -> bar().pressForDriver());
        step(0.6, "shot unicode set", mc -> {
            results.addProperty("pickerOpenAfterPress", bar().pickerOpen());
            pointAtFirst("🔥", "❤", "★");
        });
        step(0.4, "shot unicode hover", mc -> shot(mc, "picker-unicode-hover"));
        step(0.1, "keys", mc -> {
            PanelInput.clearPointer();
            bar().keyForDriver(new KeyEvent(InputConstants.KEY_RIGHT, 0, 0));
            bar().keyForDriver(new KeyEvent(InputConstants.KEY_RIGHT, 0, 0));
            bar().keyForDriver(new KeyEvent(InputConstants.KEY_DOWN, 0, 0));
        });
        step(0.4, "shot keys", mc -> shot(mc, "picker-unicode-keys"));
        step(0.1, "enter writes it", mc -> bar().keyForDriver(new KeyEvent(InputConstants.KEY_RETURN, 0, 0)));
        step(0.5, "shot written", mc -> {
            results.addProperty("lineAfterEnter", bar().lineForDriver());
            results.addProperty("pickerOpenAfterEnter", bar().pickerOpen());
            shot(mc, "written-unicode");
        });
        // The same world as Projects: the server's list.
        step(0.1, "as projects", mc -> {
            ChatEmojiBar.devHost = "play.projects.gg";
            bar().typeForDriver("selam");
        });
        // A frame later, as a player's Ctrl+E after typing would come: the way this version's keyboard reports it (26.3:
        // SDL's keycode), through the chat screen's keys.
        step(0.2, "ctrl+e", mc -> bar().keyForDriver(Compat.emojiShortcutForDriver()));
        step(0.6, "point at a VIP+ emoji", mc -> {
            results.addProperty("pickerOpenAfterShortcut", bar().pickerOpen());
            results.addProperty("pictureBeforePack", pictureShown());
            shot(mc, "picker-projects-nopack");
            pointAt(bar().emojiBox(":ates:"));
        });
        step(0.4, "shot rank", mc -> shot(mc, "picker-projects-rank"));
        step(0.1, "point at a pack emoji", mc -> pointAt(bar().emojiBox(":hmm:")));
        step(0.4, "shot pack look", mc -> shot(mc, "picker-projects-packlook"));
        step(0.1, "scroll to the shortcuts", mc -> {
            PanelInput.clearPointer();
            bar().scrollForDriver(-20);
        });
        step(0.8, "shot shortcuts", mc -> {
            shot(mc, "picker-projects-shortcuts");
            pointAt(bar().emojiBox(":ping:"));
        });
        step(0.4, "shot shortcut hover", mc -> shot(mc, "picker-projects-shortcut-hover"));
        step(0.1, "pick :kalp:", mc -> {
            PanelInput.clearPointer();
            results.addProperty("kalpWritten", bar().pickForDriver(":kalp:", false));
        });
        step(0.5, "shot code written", mc -> {
            results.addProperty("lineAfterCode", bar().lineForDriver());
            shot(mc, "written-code");
        });
        // The server's resource pack loads (as on Survival).
        step(0.2, "pack on", mc -> {
            results.addProperty("packWritten", writePack(mc));
            selectPack(mc, true);
        });
        stepAfterReload(2.0, "chat after the pack", mc -> {
            results.addProperty("reloadedWithPack", !Compat.overlayShowing(mc));
            openChat(mc);
        });
        step(0.6, "picker with the pack", mc -> bar().shortcutForDriver());
        step(0.6, "shot pack pictures", mc -> {
            results.addProperty("pictureWithPack", pictureShown());
            shot(mc, "picker-projects-pack");
            pointAt(bar().emojiBox(":hmm:"));
        });
        step(0.4, "shot pack hover", mc -> shot(mc, "picker-projects-pack-hover"));
        // Back to the lobby: the pack goes.
        step(0.2, "pack off", mc -> {
            PanelInput.clearPointer();
            selectPack(mc, false);
        });
        stepAfterReload(2.0, "chat after the pack went", mc -> {
            results.addProperty("reloadedWithoutPack", !Compat.overlayShowing(mc));
            openChat(mc);
        });
        step(0.6, "picker after the pack went", mc -> bar().shortcutForDriver());
        step(0.6, "shot pack gone", mc -> {
            results.addProperty("pictureAfterPack", pictureShown());
            shot(mc, "picker-projects-pack-gone");
        });
        // The line near the game's limit.
        step(0.1, "a long line", mc -> {
            bar().typeForDriver("a".repeat(214));
        });
        step(0.4, "shot length", mc -> shot(mc, "line-length"));
        step(0.1, "full", mc -> bar().typeForDriver("a".repeat(256)));
        step(0.4, "shot full", mc -> {
            bar().shortcutForDriver();
            results.addProperty("fullWritten", bar().pickForDriver(":kalp:", true));
        });
        step(0.3, "shot line full note", mc -> shot(mc, "line-full"));
        // GUI 3.
        step(0.2, "gui 3", mc -> {
            Compat.setScreen(mc, null);
            mc.options.guiScale().set(3);
            mc.resizeGui();
            openChat(mc);
        });
        step(0.6, "picker gui 3", mc -> {
            bar().typeForDriver("");
            bar().shortcutForDriver();
        });
        step(0.6, "shot gui 3", mc -> {
            shot(mc, "gui3-picker-projects");
            pointAt(bar().emojiBox(":gg:"));
        });
        step(0.4, "shot gui 3 hover", mc -> shot(mc, "gui3-picker-hover"));
        step(0.1, "gui 3 unicode", mc -> {
            PanelInput.clearPointer();
            ChatEmojiBar.devHost = null;
            Compat.setScreen(mc, null);
            openChat(mc);
        });
        step(0.6, "gui 3 unicode picker", mc -> bar().pressForDriver());
        step(0.6, "shot gui 3 unicode", mc -> shot(mc, "gui3-picker-unicode"));
        step(0.2, "gui 2", mc -> {
            Compat.setScreen(mc, null);
            mc.options.guiScale().set(2);
            mc.resizeGui();
        });
        // The launcher's switch off during the game: the button goes; back on.
        step(0.2, "launcher switch off", mc -> devCmd("emojis-off"));
        step(0.6, "chat without the button", mc -> openChat(mc));
        step(0.6, "shot no button", mc -> {
            results.addProperty("buttonWhenOff", bar().buttonBox() != null && OverlayClient.get().emojis().on());
            shot(mc, "switch-off");
            Compat.setScreen(mc, null);
            devCmd("emojis-on");
        });
        step(0.6, "done", mc -> results.addProperty("onAtEnd", OverlayClient.get().emojis().on()));
    }

    private void panelScript() {
        step(0.5, "open the panel", mc -> OverlayClient.get().togglePanel());
        step(0.4, "settings", mc -> need(mc).showPage(OverlayScreen.Page.SETTINGS));
        step(0.6, "shot settings", mc -> shot(mc, "settings-emoji-row"));
        // "Sohbette emoji seçici" from the keys: Home (Dil), Down, Enter.
        step(0.1, "switch it off", mc -> {
            PanelInput.clearPointer();
            need(mc).setFocused(need(mc).settings());
            key(need(mc), InputConstants.KEY_HOME);
            key(need(mc), InputConstants.KEY_DOWN);
            key(need(mc), InputConstants.KEY_RETURN);
        });
        step(0.6, "shot off", mc -> {
            results.addProperty("onAfterSettings", OverlayClient.get().emojis().on());
            shot(mc, "settings-emoji-off");
            need(mc).close();
        });
        step(0.4, "chat without it", EmojiScenario::openChat);
        step(0.6, "shot chat without it", mc -> {
            shot(mc, "chat-without-button");
            Compat.setScreen(mc, null);
        });
        step(0.3, "back on", mc -> {
            OverlayClient.get().togglePanel();
        });
        step(0.4, "settings again", mc -> need(mc).showPage(OverlayScreen.Page.SETTINGS));
        step(0.3, "switch it on", mc -> {
            need(mc).setFocused(need(mc).settings());
            key(need(mc), InputConstants.KEY_HOME);
            key(need(mc), InputConstants.KEY_DOWN);
            key(need(mc), InputConstants.KEY_RETURN);
        });
        step(0.6, "shot on", mc -> {
            results.addProperty("onAfterSettingsAgain", OverlayClient.get().emojis().on());
            shot(mc, "settings-emoji-on");
            need(mc).close();
        });
        step(0.4, "chat with it", EmojiScenario::openChat);
        step(0.8, "shot chat with it", mc -> shot(mc, "chat-with-button"));
        step(0.2, "gui 3 settings", mc -> {
            Compat.setScreen(mc, null);
            mc.options.guiScale().set(3);
            mc.resizeGui();
            OverlayClient.get().togglePanel();
        });
        step(0.4, "settings gui 3", mc -> need(mc).showPage(OverlayScreen.Page.SETTINGS));
        step(0.6, "shot settings gui 3", mc -> {
            shot(mc, "gui3-settings-emoji-row");
            need(mc).close();
            mc.options.guiScale().set(2);
            mc.resizeGui();
        });
    }

    private static OverlayScreen need(Minecraft mc) {
        if (Compat.screen(mc) instanceof OverlayScreen p) {
            return p;
        }
        throw new IllegalStateException("the panel is not open (" + Compat.screen(mc) + ")");
    }

    private static void key(OverlayScreen panel, int key) {
        panel.keyPressed(new KeyEvent(key, 0, 0));
    }

    /** Whether the picker would draw the server pack's picture for ":hmm:" now. */
    private static boolean pictureShown() {
        return Fonts.gameGlyphs() != null && Fonts.gameGlyphs().getGlyph(0xE106)
            != Fonts.gameGlyphs().getGlyph(0x10FFFF);
    }

    private void devCmd(String cmd) {
        JsonObject o = new JsonObject();
        o.addProperty("t", "dev");
        o.addProperty("cmd", cmd);
        OverlayClient.get().bridge().send(o);
        event("dev", cmd);
    }

    /** A resource pack with the server's chat pictures at their characters, as the server's pack has them. */
    private boolean writePack(Minecraft mc) {
        String art = System.getProperty("projects.overlay.dev.packart");
        if (art == null) {
            event("pack", "no art given");
            return false;
        }
        Path root = mc.gameDirectory.toPath().resolve("resourcepacks").resolve(PACK);
        try {
            PackFormat format = SharedConstants.getCurrentVersion().packVersion(PackType.CLIENT_RESOURCES);
            Files.createDirectories(root.resolve("assets/minecraft/font"));
            Files.createDirectories(root.resolve("assets/minecraft/textures/font/projects_chat"));
            Files.writeString(root.resolve("pack.mcmeta"), String.format(Locale.ROOT,
                "{\"pack\":{\"description\":\"CHAT-EMOJI test\",\"pack_format\":%d,\"min_format\":[%d,%d],"
                    + "\"max_format\":[%d,%d]}}%n", format.major(), format.major(), format.minor(), format.major(),
                format.minor()), StandardCharsets.UTF_8);
            StringBuilder font = new StringBuilder("{\"providers\":[");
            for (int i = 0; i < PICTURES.length; i++) {
                Path from = Path.of(art, PICTURES[i][0] + ".png");
                Files.copy(from, root.resolve("assets/minecraft/textures/font/projects_chat/" + PICTURES[i][0] + ".png"),
                    java.nio.file.StandardCopyOption.REPLACE_EXISTING);
                font.append(i == 0 ? "" : ",").append("{\"type\":\"bitmap\",\"file\":\"minecraft:font/projects_chat/")
                    .append(PICTURES[i][0]).append(".png\",\"height\":12,\"ascent\":10,\"chars\":[\"\\u")
                    .append(PICTURES[i][1]).append("\"]}");
            }
            font.append("]}\n");
            Files.writeString(root.resolve("assets/minecraft/font/default.json"), font, StandardCharsets.UTF_8);
            try (Stream<Path> files = Files.walk(root)) {
                event("pack", "written: " + files.count() + " paths");
            }
            return true;
        } catch (IOException | RuntimeException e) {
            event("pack", "failed: " + e);
            return false;
        }
    }

    private void selectPack(Minecraft mc, boolean on) {
        PackRepository repo = mc.getResourcePackRepository();
        repo.reload();
        String id = "file/" + PACK;
        boolean changed = on ? repo.addPack(id) : repo.removePack(id);
        event("pack", (on ? "on " : "off ") + changed + " " + repo.getSelectedIds());
        mc.reloadResourcePacks();
    }

    private void finish(Minecraft mc) {
        if (done) {
            return;
        }
        done = true;
        ChatEmojiBar.devHost = null;
        PanelInput.clearPointer();
        results.addProperty("langAtEnd", Texts.current().code);
        results.addProperty("runSeconds", Math.round((System.nanoTime() - runStarted) / 1e6) / 1000.0);
        results.add("events", events);
        try {
            Files.createDirectories(out);
            Files.writeString(out.resolve("emoji-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
        mc.stop();
    }

    private void openWorld(Minecraft mc) {
        // A view gets a normal world of its own (real terrain); the checks keep the flat one.
        String world = view != null ? WORLD + "-view" : WORLD;
        if (mc.getLevelSource().levelExists(world)) {
            event("world", "open " + world);
            mc.createWorldOpenFlows().openWorld(world, () -> Compat.setScreen(mc, new TitleScreen()));
            return;
        }
        event("world", "create " + world);
        LevelSettings settings = new LevelSettings("CHAT-EMOJI", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(world, settings, new WorldOptions(20261026L, view != null, false),
            view != null ? DevCompat.normalWorld() : DevCompat.flatWorld(), new TitleScreen());
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("emoji-%02d-%s.png", ++shots, name);
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
