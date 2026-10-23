package gg.projects.overlay;

import com.google.gson.JsonObject;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.BridgeConfig;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.connect.Connector;
import gg.projects.overlay.connect.MinecraftGame;
import gg.projects.overlay.connect.ServerTarget;
import gg.projects.overlay.settings.OverlaySettings;
import gg.projects.overlay.settings.SettingsStore;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.stickers.StickerArt;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.draw.Shapes;
import gg.projects.overlay.ui.panel.JoinScreen;
import gg.projects.overlay.ui.panel.OverlayScreen;
import java.util.function.UnaryOperator;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientLifecycleEvents;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.keymapping.v1.KeyMappingHelper;
import net.fabricmc.fabric.api.client.rendering.v1.hud.HudElementRegistry;
import net.fabricmc.fabric.api.client.screen.v1.ScreenEvents;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.resources.Identifier;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Entry point. Without the launcher's bridge properties (another launcher, or an old Projects Launcher) the
 * mod registers nothing at all: no key, no threads, no HUD layer.
 */
public final class OverlayClient implements ClientModInitializer {
    public static final String MOD_ID = "projects_overlay";
    /** Where "Katıl" goes for a friend on Projects. */
    public static final String PROJECTS_HOST = ServerTarget.PROJECTS_HOST;
    private static final Logger LOG = LoggerFactory.getLogger("Projects Overlay");
    /** A toast this recent decides which conversation the panel opens on. */
    private static final long RECENT_TOAST_MS = 30_000;
    /** Characters baked into the fonts' glyph pages a few at a time after a world opens. */
    private static final String WARM =
        "abcçdefgğhıijklmnoöprsştuüvyzqwxABCÇDEFGĞHIİJKLMNOÖPRSŞTUÜVYZQWX0123456789 .,:;!?'\"()-/%+…";
    private static final int WARM_PER_TICK = 18;
    /** "Yazıyor…" goes at most this often (CHAT2 §4; the launcher and the service pace it too). */
    private static final long TYPING_EVERY_MS = 4_000;

    private static volatile OverlayClient instance;

    private OverlayKey key;
    private OverlayState state;
    private BridgeClient bridge;
    private SettingsStore settings;
    private ToastLayer toasts;
    private MinecraftGame game;
    private Connector connector;
    private OverlayScreen.Page lastPage = OverlayScreen.Page.CHATS;
    private String lastConversation;
    private String lastToastConversation;
    private long lastToastAt;
    private boolean hadLevel;
    private int warmIndex = -1;
    private int warmScale = -1;
    private String typedIn;
    private long typedAt;

    @Override
    public void onInitializeClient() {
        BridgeConfig config = BridgeConfig.fromSystemProperties();
        if (config == null) {
            LOG.info("Not started by Projects Launcher; the overlay stays off.");
            return;
        }
        state = new OverlayState();
        settings = new SettingsStore(FabricLoader.getInstance().getGameDir());
        key = new OverlayKey(KeyMapping.Category.register(Identifier.fromNamespaceAndPath(MOD_ID, "main")));
        KeyMappingHelper.registerKeyMapping(key);
        toasts = new ToastLayer();
        HudElementRegistry.addLast(Identifier.fromNamespaceAndPath(MOD_ID, "toasts"), toasts);
        // The travel toast stays above the saving, connecting and loading screens (and the pause menu).
        ScreenEvents.AFTER_INIT.register((mc, screen, width, height) ->
            ScreenEvents.afterExtract(screen).register((s, g, mouseX, mouseY, delta) -> toasts.extractOverScreen(g)));
        game = new MinecraftGame(this);
        connector = new Connector(game, frame -> bridge.send(frame));
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);

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
        ClientLifecycleEvents.CLIENT_STARTED.register(mc -> {
            bridge.start();
            Shapes.prepare();
        });
        ClientLifecycleEvents.CLIENT_STOPPING.register(mc -> {
            bridge.stop();
            settings.close();
        });
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

    public ToastLayer toasts() {
        return toasts;
    }

    public OverlaySettings settings() {
        return settings.get();
    }

    public void updateSettings(UnaryOperator<OverlaySettings> change) {
        settings.set(change.apply(settings.get()));
    }

    /** The player's "Animasyonları azalt" or the launcher's "Hareketi azalt". */
    public boolean reducedMotion() {
        return settings.get().reduceMotion() || state.reducedMotion();
    }

    public OverlayScreen.Page lastPage() {
        return lastPage;
    }

    public void rememberPage(OverlayScreen.Page page) {
        lastPage = page;
    }

    public void rememberConversation(String id) {
        lastConversation = id;
    }

    /** The conversation the panel should open on: a recent toast's, else the last one looked at. */
    public String preferredConversation() {
        long now = System.currentTimeMillis();
        if (lastToastConversation != null && now - lastToastAt < RECENT_TOAST_MS) {
            return lastToastConversation;
        }
        return lastConversation;
    }

    private void tick(Minecraft mc) {
        while (key.consumeClick()) {
            togglePanel();
        }
        boolean hasLevel = mc.level != null;
        if (hadLevel && !hasLevel) {
            toasts.reset();
            StickerArt.releaseAll();
        }
        if (!hadLevel && hasLevel) {
            warmIndex = 0;
        }
        hadLevel = hasLevel;
        connector.tick(monotonicMs());
        Shapes.prepare();
        StickerArt.tick(monotonicMs());
        warmFonts(mc);
    }

    /**
     * Bakes the bundled fonts' common glyphs for the current GUI scale a few per tick after a world opens,
     * so the first toast does not pay for them in its frame.
     */
    private void warmFonts(Minecraft mc) {
        int scale = mc.getWindow().getGuiScale();
        if (scale != warmScale) {
            warmScale = scale;
            warmIndex = mc.level != null ? 0 : -1;
        }
        if (warmIndex < 0 || mc.level == null) {
            return;
        }
        int end = Math.min(WARM.length(), warmIndex + WARM_PER_TICK);
        String chunk = WARM.substring(warmIndex, end);
        for (Fonts.Face face : Fonts.Face.values()) {
            Fonts.get(face).width(chunk);
        }
        warmIndex = end >= WARM.length() ? -1 : end;
    }

    /** Opens the panel from the game (never over another screen) or closes it. */
    public void togglePanel() {
        Minecraft mc = Minecraft.getInstance();
        if (Compat.screen(mc) instanceof OverlayScreen screen) {
            screen.close();
            return;
        }
        if (Compat.screen(mc) != null) {
            return;
        }
        // A background retry is pending: try now. After the launcher's goodbye only "Yeniden dene" retries,
        // so the panel keeps saying why it is offline.
        if (bridge.status() == BridgeClient.Status.OFFLINE) {
            bridge.retryNow();
        }
        Compat.setScreen(mc, new OverlayScreen(this));
    }

    public void panelOpened() {
        toasts.clear();
        bridge.send(Protocol.panel(true));
    }

    public void panelClosed() {
        bridge.send(Protocol.panel(false));
        // No "yazıyor…" comes while the panel is closed: what was shown would only go stale.
        state.clearTyping();
        typedIn = null;
    }

    public void openConversation(String conversation) {
        lastConversation = conversation;
        bridge.send(Protocol.openConversation(conversation));
    }

    public void sendChat(String conversation, String text) {
        sendChat(conversation, text, null);
    }

    /** The player's message, as a reply to the message {@code quote} stands for when not null (CHAT2). */
    public void sendChat(String conversation, String text, OverlayState.Reply quote) {
        long reply = quote == null ? 0 : quote.sid();
        String clientId = state.addPending(conversation, text, null, reply > 0 ? quote : null,
            System.currentTimeMillis());
        if (!bridge.send(Protocol.send(clientId, conversation, text, reply))) {
            state.markFailed(clientId, "offline");
        }
        typedIn = null;
        refreshScreen();
    }

    /** A Projects sticker ({@code pl1/gg}), maybe as a reply (CHAT2 §8). */
    public void sendSticker(String conversation, String sticker, OverlayState.Reply quote) {
        long reply = quote == null ? 0 : quote.sid();
        String clientId = state.addPending(conversation, "", sticker, reply > 0 ? quote : null,
            System.currentTimeMillis());
        if (!bridge.send(Protocol.sendSticker(clientId, conversation, sticker, reply))) {
            state.markFailed(clientId, "offline");
        }
        updateSettings(s -> s.withStickerUsed(sticker));
        refreshScreen();
    }

    /**
     * Gives ({@code on}) or takes back the player's reaction (CHAT2 §7): the launcher shows it at once and
     * sends the message back with it. The limits the panel knows are told without asking.
     */
    public void react(String conversation, OverlayState.ChatLine line, String emoji, String shown, boolean on) {
        long now = System.currentTimeMillis();
        if (on && line.reaction(shown) == null && line.reactions.size() >= OverlayState.REACTION_KINDS) {
            state.note(conversation, "react.kinds", now);
        } else if (on && line.ownReactions() >= OverlayState.REACTIONS_PER_PLAYER) {
            state.note(conversation, "react.full", now);
        } else if (!bridge.send(Protocol.react(conversation, line.sid, emoji, on))) {
            state.note(conversation, "react.failed", now);
        }
        refreshScreen();
    }

    /** The player writes in the panel's field: "yazıyor…" at most every 4 s (CHAT2 §4). */
    public void typing(String conversation) {
        long now = monotonicMs();
        if (conversation.equals(typedIn) && now - typedAt < TYPING_EVERY_MS) {
            return;
        }
        if (bridge.send(Protocol.typing(conversation))) {
            typedIn = conversation;
            typedAt = now;
        }
    }

    public void answer(String call, String answer) {
        bridge.send(Protocol.answer(call, answer));
    }

    public void retry() {
        state.setStatus(BridgeClient.Status.CONNECTING, null);
        bridge.retryNow();
        refreshScreen();
    }

    /** Whether the game is on Projects now. */
    public boolean onProjects() {
        ServerTarget here = game.currentServer();
        return here != null && here.projects();
    }

    /** Whether the game is on {@code host} now. */
    public boolean onServer(String host) {
        ServerTarget here = game.currentServer();
        return here != null && here.sameServer(ServerTarget.parse(host));
    }

    /**
     * Goes to a friend's (or a caller's) server after the player confirmed in the overlay: the travel toast,
     * then the current game is left the way the pause menu does (a singleplayer world is saved).
     */
    public void connect(JoinScreen.Target target) {
        ServerTarget to = ServerTarget.parse(target.host());
        if (to == null) {
            LOG.warn("Not joining: the address is not valid");
            return;
        }
        if (!connector.go(to, monotonicMs())) {
            LOG.info("Not joining now: the game is loading, saving or connecting");
            return;
        }
        if (target.friendId() != null) {
            bridge.send(Protocol.join(target.friendId()));
        }
    }

    public Connector connector() {
        return connector;
    }

    /** Applies a launcher frame on the render thread. Public for the scenario driver, which replays frames. */
    public void handleFrame(JsonObject frame) {
        if ("connect".equals(Protocol.type(frame))) {
            connector.request(frame, monotonicMs());
            return;
        }
        OverlayState.ToastRequest toast = state.apply(frame, System.currentTimeMillis());
        if (toast != null) {
            showToast(toast);
        }
        refreshScreen();
    }

    private void onBridgeStatus(BridgeClient.Status status, String reason) {
        state.setStatus(status, reason);
        if (status != BridgeClient.Status.ONLINE) {
            toasts.clear();
        }
        refreshScreen();
    }

    private void showToast(OverlayState.ToastRequest t) {
        Minecraft mc = Minecraft.getInstance();
        OverlayScreen panel = Compat.screen(mc) instanceof OverlayScreen s ? s : null;
        OverlayState.Conversation conv = state.conversation(t.conversation());
        if (t.conversation() != null) {
            lastToastConversation = t.conversation();
            lastToastAt = System.currentTimeMillis();
        }
        toasts.show(t, conv != null && conv.crew(), panel == null ? null : panel.openConversation(), panel != null,
            settings.get(), key.keyName(), reducedMotion());
    }

    private void refreshScreen() {
        if (Compat.screen(Minecraft.getInstance()) instanceof OverlayScreen screen) {
            screen.onStateChanged();
        }
    }

    private static long monotonicMs() {
        return System.nanoTime() / 1_000_000L;
    }

    private static String version(String modId) {
        return FabricLoader.getInstance().getModContainer(modId)
            .map(c -> c.getMetadata().getVersion().getFriendlyString())
            .orElse("unknown");
    }
}
