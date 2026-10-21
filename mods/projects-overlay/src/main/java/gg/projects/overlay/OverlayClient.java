package gg.projects.overlay;

import com.google.gson.JsonObject;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.BridgeConfig;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.ui.OverlayScreen;
import gg.projects.overlay.ui.OverlayToast;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientLifecycleEvents;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.keymapping.v1.KeyMappingHelper;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import org.lwjgl.glfw.GLFW;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Entry point. Without the launcher's bridge properties (another launcher, or an old Projects Launcher) the
 * mod registers nothing at all: no key, no threads, no render hooks.
 */
public final class OverlayClient implements ClientModInitializer {
    public static final String MOD_ID = "projects_overlay";
    private static final Logger LOG = LoggerFactory.getLogger("Projects Overlay");

    private static volatile OverlayClient instance;

    private OverlayKey key;
    private OverlayState state;
    private BridgeClient bridge;

    @Override
    public void onInitializeClient() {
        BridgeConfig config = BridgeConfig.fromSystemProperties();
        if (config == null) {
            LOG.info("Not started by Projects Launcher; the overlay stays off.");
            return;
        }
        state = new OverlayState();
        key = new OverlayKey(KeyMapping.Category.register(Identifier.fromNamespaceAndPath(MOD_ID, "main")));
        KeyMappingHelper.registerKeyMapping(key);
        ClientTickEvents.END_CLIENT_TICK.register(mc -> {
            while (key.consumeClick()) {
                togglePanel();
            }
        });

        bridge = new BridgeClient(config, version(MOD_ID), version("minecraft"),
            () -> Minecraft.getInstance().options.languageCode, new BridgeClient.Listener() {
                @Override
                public void onStatus(BridgeClient.Status status, String reason) {
                    Minecraft.getInstance().execute(() -> onBridgeStatus(status, reason));
                }

                @Override
                public void onFrame(JsonObject frame) {
                    Minecraft.getInstance().execute(() -> handleFrame(frame));
                }
            });
        ClientLifecycleEvents.CLIENT_STARTED.register(mc -> bridge.start());
        ClientLifecycleEvents.CLIENT_STOPPING.register(mc -> bridge.stop());
        instance = this;
        LOG.info("Overlay on; launcher bridge at 127.0.0.1:{}", config.port());
    }

    /** The running overlay, or null when the game was not started by Projects Launcher. */
    public static OverlayClient get() {
        return instance;
    }

    public OverlayState state() {
        return state;
    }

    public OverlayKey key() {
        return key;
    }

    public BridgeClient bridge() {
        return bridge;
    }

    /** Called by the keyboard mixin for every key event; returns true when the Shift+Tab chord was used. */
    public static boolean consumeChord(long window, int action, KeyEvent event) {
        OverlayClient self = instance;
        if (self == null || action != GLFW.GLFW_PRESS || !self.key.usesChord() || !OverlayKey.isChord(event)) {
            return false;
        }
        Minecraft mc = Minecraft.getInstance();
        if (window != mc.getWindow().handle() || mc.screen != null) {
            return false;
        }
        self.togglePanel();
        return true;
    }

    /** Opens the panel from the game (never over another screen) or closes it. */
    public void togglePanel() {
        Minecraft mc = Minecraft.getInstance();
        if (mc.screen instanceof OverlayScreen) {
            mc.setScreen(null);
            return;
        }
        if (mc.screen != null) {
            return;
        }
        // A background retry is pending: try now. After the launcher's goodbye only "Try again" retries, so the
        // panel keeps saying why it is offline.
        if (bridge.status() == BridgeClient.Status.OFFLINE) {
            bridge.retryNow();
        }
        bridge.send(Protocol.panel(true));
        mc.setScreen(new OverlayScreen(this));
    }

    public void panelClosed() {
        bridge.send(Protocol.panel(false));
    }

    public void openConversation(String conversation) {
        bridge.send(Protocol.openConversation(conversation));
    }

    public void sendChat(String conversation, String text) {
        String clientId = state.addPending(conversation, text, System.currentTimeMillis());
        if (!bridge.send(Protocol.send(clientId, conversation, text))) {
            state.markFailed(clientId, "offline");
        }
        refreshScreen();
    }

    public void answer(String call, String answer) {
        bridge.send(Protocol.answer(call, answer));
    }

    /** Asks the launcher where the friend plays. The game connects only after the player confirms in-game. */
    public void join(String friend) {
        JsonObject o = Protocol.message("join");
        o.addProperty("friend", friend);
        bridge.send(o);
    }

    public void retry() {
        state.setStatus(BridgeClient.Status.CONNECTING, null);
        bridge.retryNow();
        refreshScreen();
    }

    /** Applies a launcher frame on the render thread. Public for the spike driver, which replays frames. */
    public void handleFrame(JsonObject frame) {
        OverlayState.ToastRequest toast = state.apply(frame, System.currentTimeMillis());
        if (toast != null) {
            showToast(toast);
        }
        refreshScreen();
    }

    private void onBridgeStatus(BridgeClient.Status status, String reason) {
        state.setStatus(status, reason);
        refreshScreen();
    }

    private void showToast(OverlayState.ToastRequest t) {
        Minecraft mc = Minecraft.getInstance();
        if (t.conversation() != null && mc.screen instanceof OverlayScreen screen
            && t.conversation().equals(screen.openConversation())) {
            return;
        }
        // Calls and invites carry names only; the wording comes from the game's language.
        OverlayToast.Kind kind;
        String body;
        switch (t.kind()) {
            case "call" -> {
                kind = OverlayToast.Kind.CALL;
                body = Component.translatable("projects_overlay.toast.call", t.body()).getString();
            }
            case "invite" -> {
                kind = OverlayToast.Kind.CALL;
                body = Component.translatable("projects_overlay.toast.invite").getString();
            }
            case "info" -> {
                kind = OverlayToast.Kind.INFO;
                body = t.body();
            }
            default -> {
                kind = OverlayToast.Kind.MESSAGE;
                body = t.body().isEmpty() ? Component.translatable("projects_overlay.toast.new_message").getString()
                    : t.body();
            }
        }
        Component hint = kind == OverlayToast.Kind.INFO ? null : key.getTranslatedKeyMessage();
        OverlayToast.show(mc, kind, t.key(), t.title(), body, hint, state.reducedMotion());
    }

    private void refreshScreen() {
        if (Minecraft.getInstance().screen instanceof OverlayScreen screen) {
            screen.onStateChanged();
        }
    }

    private static String version(String modId) {
        return FabricLoader.getInstance().getModContainer(modId)
            .map(c -> c.getMetadata().getVersion().getFriendlyString())
            .orElse("unknown");
    }
}
