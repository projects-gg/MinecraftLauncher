package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.chat.ChatEmojiBar;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.toast.ToastCard;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.toast.ToastStack;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.panel.OverlayScreen;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.screen.v1.ScreenKeyboardEvents;
import net.fabricmc.fabric.api.client.screen.v1.ScreenMouseEvents;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.screens.ChatScreen;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
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
 * Development-only check of F7-OPEN, enabled with {@code -Dprojects.overlay.dev.script=f7} and the mock launcher
 * (no flags needed; {@code -Dprojects.overlay.lang=en} for the English run). In a flat world, with the keyboard taken
 * for a laptop's: a message toast says to press the key and, the key never having opened the panel here, that Fn may
 * be needed; in the chat the toast lights under the pointer and a press on it opens the panel on its conversation;
 * the overlay key opens the panel from the pause menu (and closes it back there), from the chat (whose draft the game
 * keeps) and from the inventory (which closes); afterwards the toasts no longer carry the Fn line; with no key bound
 * the toast says where to bind one. Keys and presses go through the screens' own events (never the OS); results and
 * screenshots go to {@code -Dprojects.overlay.dev.out}.
 */
public final class F7Scenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("F7-OPEN scenario");
    private static final String WORLD = "f7-scenario";
    /** The mock launcher's "one": PixelAvci writes in this conversation. */
    private static final String CONVERSATION = "dm:ayse";

    private record Step(double after, String name, Consumer<Minecraft> action) {
    }

    private final List<Step> steps = new ArrayList<>();
    private final JsonObject results = new JsonObject();
    private final JsonArray events = new JsonArray();
    private final long runStarted = System.nanoTime();
    private Path out;
    private String lang = "tr";
    private int next;
    private long stepAt = System.nanoTime();
    private int shots;
    private boolean started;
    private boolean done;
    private int worldTicks = -1;

    @Override
    public void onInitializeClient() {
        if (!"f7".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "f7-out")).toAbsolutePath();
        ToastLayer.pauseWhileAway = false;
        script();
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("F7-OPEN scenario driver on; results in {}", out);
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
        lang = Texts.current().code;
        results.addProperty("minecraft", Compat.VERSION);
        results.addProperty("lang", lang);
        results.addProperty("guiScale", mc.getWindow().getGuiScale());
        results.addProperty("key", overlay.key().keyName());
        results.addProperty("functionKey", overlay.key().functionKey());
        mc.player.setXRot(20f);
        return true;
    }

    private void script() {
        // A laptop's keyboard, the key never used here: the first toasts carry the Fn line.
        step(0.5, "laptop", mc -> {
            OverlayClient overlay = OverlayClient.get();
            overlay.laptopForDriver(true);
            overlay.updateSettings(s -> s.withKeyUsed(false));
        });
        step(0.3, "message", mc -> dev("one"));
        step(1.0, "shot toast", mc -> {
            ToastCard card = card(CONVERSATION);
            results.addProperty("toastShown", card != null);
            results.addProperty("fnLineOnFirstToast", card != null && card.fnTip());
            shot(mc, "toast-fn");
        });
        // GUI 3 at 720p (426 x 240 GUI pixels, the most common): the two hint lines inside the card.
        step(0.1, "gui 3", mc -> {
            mc.options.guiScale().set(3);
            mc.resizeGui();
        });
        step(0.4, "shot toast gui 3", mc -> shot(mc, "toast-fn-gui3"));
        step(0.1, "gui 2", mc -> {
            mc.options.guiScale().set(2);
            mc.resizeGui();
        });
        // The chat: the pointer is free there, so the toast is a button.
        step(0.2, "chat", mc -> Compat.openChat(mc));
        step(0.4, "point at the toast", mc -> {
            float[] box = OverlayClient.get().toasts().boxForDriver(CONVERSATION);
            if (box == null) {
                throw new IllegalStateException("no toast for " + CONVERSATION);
            }
            float[] at = {box[0] + box[2] / 2f, box[1] + box[3] / 2f};
            OverlayClient.get().toasts().pointForDriver(at);
            results.addProperty("pointingOnToast", OverlayClient.get().toasts().pointing(at[0], at[1]));
            results.addProperty("pointingBesideToast", OverlayClient.get().toasts().pointing(box[0] - 20, at[1]));
        });
        step(0.4, "shot hover", mc -> shot(mc, "chat-toast-hover"));
        step(0.1, "press the toast", mc -> {
            float[] box = OverlayClient.get().toasts().boxForDriver(CONVERSATION);
            Screen chat = Compat.screen(mc);
            boolean passedOn = box != null && ScreenMouseEvents.allowMouseClick(chat).invoker().allowMouseClick(chat,
                new MouseButtonEvent(box[0] + box[2] / 2f, box[1] + box[3] / 2f,
                    new MouseButtonInfo(InputConstants.MOUSE_BUTTON_LEFT, 0)));
            OverlayClient.get().toasts().pointForDriver(null);
            results.addProperty("toastPressTaken", !passedOn);
        });
        step(0.6, "panel from the toast", mc -> {
            OverlayScreen panel = panel(mc);
            results.addProperty("toastPressOpened", panel != null);
            results.addProperty("toastPressConversation", panel == null ? null : panel.openConversation());
            shot(mc, "panel-from-toast");
        });
        step(0.3, "close", mc -> need(mc).close());
        // The pause menu: the key opens the panel and closes it back to the menu.
        step(0.5, "pause", mc -> mc.pauseGame(false));
        step(0.5, "f7 on the pause menu", mc -> {
            Screen pause = Compat.screen(mc);
            results.addProperty("pauseOpen", pause instanceof PauseScreen);
            results.addProperty("pauseKeyTaken", !keyOnScreen(pause));
        });
        step(0.5, "panel from pause", mc -> {
            results.addProperty("pauseF7Opened", panel(mc) != null);
            shot(mc, "panel-from-pause");
        });
        step(0.1, "f7 in the panel", mc -> need(mc).keyPressed(f7()));
        step(0.5, "back to pause", mc -> {
            results.addProperty("backToPause", Compat.screen(mc) instanceof PauseScreen);
            shot(mc, "back-to-pause");
            Compat.setScreen(mc, null);
        });
        // The chat with words typed: the key opens the panel, the game keeps the words as the chat's draft.
        step(0.5, "chat with a draft", mc -> {
            Compat.openChat(mc);
        });
        step(0.3, "type", mc -> {
            ChatEmojiBar bar = ChatEmojiBar.last();
            if (bar == null) {
                throw new IllegalStateException("no chat line");
            }
            bar.typeForDriver("selam");
        });
        step(0.2, "f7 in the chat", mc -> {
            Screen chat = Compat.screen(mc);
            results.addProperty("chatOpen", chat instanceof ChatScreen);
            results.addProperty("chatKeyTaken", !keyOnScreen(chat));
        });
        step(0.5, "panel from chat", mc -> {
            results.addProperty("chatF7Opened", panel(mc) != null);
            shot(mc, "panel-from-chat");
            need(mc).close();
        });
        step(0.5, "chat again", mc -> Compat.openChat(mc));
        step(0.3, "draft", mc -> {
            ChatEmojiBar bar = ChatEmojiBar.last();
            results.addProperty("chatDraftKept", bar != null && "selam".equals(bar.lineForDriver()));
            if (bar != null) {
                bar.typeForDriver("");
            }
            Compat.setScreen(mc, null);
        });
        // The inventory: closed as E closes it, then the panel.
        step(0.5, "inventory", mc -> Compat.setScreen(mc, new InventoryScreen(mc.player)));
        step(0.4, "f7 in the inventory", mc -> {
            Screen inventory = Compat.screen(mc);
            results.addProperty("inventoryOpen", inventory instanceof AbstractContainerScreen<?>);
            results.addProperty("inventoryKeyTaken", !keyOnScreen(inventory));
        });
        step(0.5, "panel from the inventory", mc -> {
            results.addProperty("inventoryF7Opened", panel(mc) != null);
            results.addProperty("containerClosed", mc.player.containerMenu == mc.player.inventoryMenu);
            shot(mc, "panel-from-inventory");
            need(mc).close();
        });
        // The key has opened the panel: the next toast has no Fn line.
        step(0.8, "another message", mc -> {
            results.addProperty("keyUsed", OverlayClient.get().settings().keyUsed());
            dev("more");
        });
        step(1.0, "shot toast after", mc -> {
            ToastCard card = card(CONVERSATION);
            results.addProperty("fnLineAfterKeyUsed", card != null && card.fnTip());
            shot(mc, "toast-after-key");
        });
        // No key bound: the toast says where to bind one.
        step(6.5, "unbind", mc -> OverlayClient.get().key().bind("key.keyboard.unknown"));
        step(0.3, "message unbound", mc -> dev("fourth"));
        step(1.0, "shot unbound", mc -> {
            results.addProperty("keyUnbound", OverlayClient.get().key().keyName() == null);
            shot(mc, "toast-unbound");
        });
        step(0.2, "bind back", mc -> OverlayClient.get().key().bind(null));
    }

    /** The overlay key pressed on {@code screen} the way the game hands it on; true when the screen got it. */
    private static boolean keyOnScreen(Screen screen) {
        if (screen == null) {
            throw new IllegalStateException("no screen for the key");
        }
        return ScreenKeyboardEvents.allowKeyPress(screen).invoker().allowKeyPress(screen, f7());
    }

    private static KeyEvent f7() {
        return new KeyEvent(InputConstants.KEY_F7, 0, 0);
    }

    private static ToastCard card(String conversation) {
        for (ToastStack.Entry<ToastCard> e : OverlayClient.get().toasts().stack().entries()) {
            if (!e.leaving() && conversation.equals(e.content().conversation())) {
                return e.content();
            }
        }
        return null;
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

    private void finish(Minecraft mc) {
        if (done) {
            return;
        }
        done = true;
        results.addProperty("runSeconds", round((System.nanoTime() - runStarted) / 1e9));
        write();
        mc.stop();
    }

    private void dev(String cmd) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
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
        LevelSettings settings = new LevelSettings("F7-OPEN scenario", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261101L, false, false),
            DevCompat.flatWorld(), new TitleScreen());
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("f7-%s-%02d-%s.png", lang, ++shots, name);
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
            Files.writeString(out.resolve("f7-" + lang + "-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
    }

    private static double round(double v) {
        return Math.round(v * 1000.0) / 1000.0;
    }
}
