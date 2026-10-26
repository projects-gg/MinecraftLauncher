package gg.projects.overlay;

import com.google.gson.JsonObject;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.BridgeConfig;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.chat.ChatEmojiBar;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.connect.Connector;
import gg.projects.overlay.connect.MinecraftGame;
import gg.projects.overlay.connect.ServerTarget;
import gg.projects.overlay.connect.WorldResume;
import gg.projects.overlay.emoji.EmojiState;
import gg.projects.overlay.host.HostController;
import gg.projects.overlay.host.HostGate;
import gg.projects.overlay.lang.Language;
import gg.projects.overlay.lang.LanguageChoice;
import gg.projects.overlay.link.CodelessLink;
import gg.projects.overlay.link.LinkMessages;
import gg.projects.overlay.link.LinkPayload;
import gg.projects.overlay.settings.OverlaySettings;
import gg.projects.overlay.settings.SettingsStore;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.stickers.StickerArt;
import gg.projects.overlay.toast.ToastCard;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.Fonts;
import gg.projects.overlay.ui.Texts;
import gg.projects.overlay.ui.draw.Shapes;
import gg.projects.overlay.ui.panel.DiscordLinkScreen;
import gg.projects.overlay.ui.panel.HostPauseButton;
import gg.projects.overlay.ui.panel.JoinScreen;
import gg.projects.overlay.ui.panel.OverlayScreen;
import gg.projects.overlay.ui.panel.SettingsList;
import java.util.List;
import java.util.function.UnaryOperator;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientLifecycleEvents;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.keymapping.v1.KeyMappingHelper;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents;
import net.fabricmc.fabric.api.client.rendering.v1.hud.HudElementRegistry;
import net.fabricmc.fabric.api.client.rendering.v1.hud.VanillaHudElements;
import net.fabricmc.fabric.api.client.screen.v1.ScreenEvents;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;
import net.fabricmc.fabric.api.networking.v1.ServerPlayConnectionEvents;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.resources.Identifier;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Entry point. Without the launcher's bridge properties (another launcher, or an old Projects Launcher) the
 * mod registers nothing at all: no key, no threads, no HUD layer. With "Oyun içi katman" off and "Sohbette emoji seçici"
 * on (CHAT-EMOJI: {@code -Dprojects.overlay.panel=off}) it runs the chat's emoji picker alone ({@link #startPickerOnly}).
 */
public final class OverlayClient implements ClientModInitializer, ChatEmojiBar.Host {
    public static final String MOD_ID = "projects_overlay";
    /** {@code off} while the launcher's overlay switch is off and the jar is there for the emoji picker alone. */
    public static final String PANEL_PROPERTY = "projects.overlay.panel";
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
    /** The chat emoji picker's key (F9 by default; CHAT-EMOJI), in both modes. */
    private EmojiKey emojiKey;
    private OverlayState state;
    private BridgeClient bridge;
    private SettingsStore settings;
    private ToastLayer toasts;
    private MinecraftGame game;
    private Connector connector;
    /** Codeless Discord linking with a Projects server (SKIN1-O). */
    private CodelessLink link;
    /** Opening the player's singleplayer world to friends (HOST1). */
    private HostController host;
    /** The launcher's "Devam et" world, opened once the title screen drew a frame (OVL-FIX); null: none. */
    private WorldResume resume;
    /** The friend whose world "Katıl" was pressed for, and when (the launcher's {@code connect} follows). */
    private String pendingWorld;
    private long pendingWorldAt;
    /** The friend's world the game went to with a label, and where that is (its "Katıl" hides while there). */
    private String worldFriend;
    private ServerTarget worldTarget;
    private OverlayScreen.Page lastPage = OverlayScreen.Page.CHATS;
    private String lastConversation;
    private String lastToastConversation;
    private long lastToastAt;
    private boolean hadLevel;
    private int warmIndex = -1;
    private int warmScale = -1;
    private String typedIn;
    private long typedAt;
    /** The HUD draws inside the area beside the open panel this frame (render thread). */
    private boolean hudClipped;
    /** The chat's emoji picker: its switch and the server's emojis (CHAT-EMOJI). */
    private EmojiState emojis;
    /** The panel runs (else the emoji picker alone). */
    private boolean panel = true;

    @Override
    public void onInitializeClient() {
        BridgeConfig config = BridgeConfig.fromSystemProperties();
        if (config == null) {
            LOG.info("Not started by Projects Launcher; the overlay stays off.");
            return;
        }
        // The overlay speaks the launcher's language from its first frame (OVL-LANG): the game starts with it, every
        // welcome names it again; an older launcher names none and the game's own language stays.
        Texts.speak(new LanguageChoice(System.getProperty(LanguageChoice.PROPERTY), Texts::gameLanguage));
        state = new OverlayState();
        resume = WorldResume.fromSystemProperty();
        settings = new SettingsStore(FabricLoader.getInstance().getGameDir());
        emojis = new EmojiState(EmojiState.onAtStart(System.getProperty(EmojiState.SWITCH_PROPERTY)));
        panel = !"off".equalsIgnoreCase(String.valueOf(System.getProperty(PANEL_PROPERTY)).strip());
        // One "Projects" category in the game's Controls for both keys; the emoji picker's is there in both modes.
        KeyMapping.Category category = KeyMapping.Category.register(Identifier.fromNamespaceAndPath(MOD_ID, "main"));
        emojiKey = new EmojiKey(category);
        KeyMappingHelper.registerKeyMapping(emojiKey);
        if (!panel) {
            startPickerOnly(config);
            return;
        }
        key = new OverlayKey(category);
        KeyMappingHelper.registerKeyMapping(key);
        toasts = new ToastLayer();
        HudElementRegistry.addLast(Identifier.fromNamespaceAndPath(MOD_ID, "toasts"), toasts);
        // While the panel is open the game's HUD draws only beside it: the aim mark and the hotbar showed through the
        // panel at a high GUI scale (CHAT2-O2 follow-up). Everything else of the HUD stays where it is.
        HudElementRegistry.addFirst(Identifier.fromNamespaceAndPath(MOD_ID, "panel_clip"), (g, delta) -> {
            Minecraft mc = Minecraft.getInstance();
            int right = OverlayScreen.hudRightOf(Compat.screen(mc));
            if (right > 0) {
                g.enableScissor(0, 0, right, mc.getWindow().getGuiScaledHeight());
                hudClipped = true;
            }
        });
        HudElementRegistry.addLast(Identifier.fromNamespaceAndPath(MOD_ID, "panel_clip_end"), (g, delta) -> {
            if (hudClipped) {
                g.disableScissor();
                hudClipped = false;
            }
        });
        // The aim mark means nothing while the pointer is out, and a tag's bubble opens beside the panel, often right
        // over the middle of the screen where the mark sits (TEAM6-O).
        HudElementRegistry.replaceElement(VanillaHudElements.CROSSHAIR, crosshair -> (g, delta) -> {
            // Nor under the codeless link question (SKIN1-O), whose card sits on the middle of the screen.
            Screen on = Compat.screen(Minecraft.getInstance());
            if (OverlayScreen.hudRightOf(on) == 0 && !(on instanceof DiscordLinkScreen)) {
                crosshair.extractRenderState(g, delta);
            }
        });
        // The travel toast stays above the saving, connecting and loading screens (and the pause menu).
        ScreenEvents.AFTER_INIT.register((mc, screen, width, height) -> {
            ScreenEvents.afterExtract(screen).register((s, g, mouseX, mouseY, delta) -> toasts.extractOverScreen(g));
            // "Devam et" (OVL-FIX): the world waits for the title screen and a frame after it, when every mod has had
            // its first frame.
            WorldResume waiting = resume;
            if (waiting != null && screen instanceof TitleScreen) {
                waiting.titleScreen(WorldResume.frames(), monotonicMs());
                ScreenEvents.afterExtract(screen).register((s, g, mouseX, mouseY, delta) -> waiting.titleFrame());
            }
            // "Arkadaşlarını çağır" in the pause menu of a singleplayer world (HOST1, host1.md §4.8).
            if (screen instanceof PauseScreen pause && pause.showsPauseMenu()) {
                HostPauseButton.attach(this, pause);
            }
            // The chat's emoji button and its picker (CHAT-EMOJI).
            ChatEmojiBar.attach(this, screen);
        });
        game = new MinecraftGame(this);
        connector = new Connector(game, frame -> bridge.send(frame));
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);

        bridge = new BridgeClient(config, version(MOD_ID), version("minecraft"),
            () -> Texts.language().helloCode(), new BridgeClient.Listener() {
                @Override
                public void onStatus(BridgeClient.Status status, String reason) {
                    Minecraft.getInstance().execute(() -> onBridgeStatus(status, reason));
                }

                @Override
                public void onFrame(JsonObject frame) {
                    Minecraft.getInstance().execute(() -> handleFrame(frame));
                }
            });
        host = new HostController(this, new HostController.View() {
            @Override
            public void changed() {
                if (Compat.screen(Minecraft.getInstance()) instanceof OverlayScreen screen) {
                    screen.hostChanged();
                }
            }

            @Override
            public void guest(String label, boolean in) {
                showHostNote(label, Texts.str(in ? "host.joined" : "host.left"), false, null, "host-guest:" + label);
            }
        });
        // The gate's key comes from the bridge token (PROTOCOL.md §7.1); the world's server reaches the session there.
        HostGate.init(config.gateKey(), host);
        ServerPlayConnectionEvents.JOIN.register((handler, sender, server) -> HostGate.joined(server, handler.player));
        ServerLifecycleEvents.SERVER_STOPPING.register(HostGate::serverStopping);
        ServerLifecycleEvents.SERVER_STOPPED.register(HostGate::serverStopped);
        link = new CodelessLink(new CodelessLink.Out() {
            @Override
            public void toServer(String json) {
                LinkPayload.send(json);
            }

            @Override
            public void toLauncher(JsonObject frame) {
                bridge.send(frame);
            }
        });
        LinkPayload.register(this::onLinkMessage);
        // The pause menu's Projects button in its bottom right corner (OVL-UX; world sharing's "Arkadaşlarını çağır"
        // takes the top right one).
        ScreenEvents.AFTER_INIT.register((mc, screen, width, height) ->
            gg.projects.overlay.ui.panel.PausePanelButton.attach(this, screen));
        // A new connection (or the proxy moving the game to another of its servers) starts over; leaving ends the
        // question: its token means nothing anywhere else.
        ClientPlayConnectionEvents.JOIN.register((handler, sender, mc) -> mc.execute(() -> {
            leftServer();
            // A new server (a proxy's move too, Lobi to Survival) may bring its resource pack or take it away.
            ChatEmojiBar.serverChanged();
        }));
        ClientPlayConnectionEvents.DISCONNECT.register((handler, mc) -> mc.execute(this::leftServer));
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

    /**
     * The launcher's overlay switch is off and its emoji picker on (CHAT-EMOJI, E2): the mod runs the chat's emoji picker
     * alone, with what the launcher's "Devam et" needs of it (the world opened after the title screen, and its note when the
     * world is gone). No key, panel, friends' toasts, pause menu button, world sharing or linking; the launcher counts the
     * game as not connected and tells it only its words and the emojis.
     */
    private void startPickerOnly(BridgeConfig config) {
        toasts = new ToastLayer();
        HudElementRegistry.addLast(Identifier.fromNamespaceAndPath(MOD_ID, "toasts"), toasts);
        ScreenEvents.AFTER_INIT.register((mc, screen, width, height) -> {
            ScreenEvents.afterExtract(screen).register((s, g, mouseX, mouseY, delta) -> toasts.extractOverScreen(g));
            WorldResume waiting = resume;
            if (waiting != null && screen instanceof TitleScreen) {
                waiting.titleScreen(WorldResume.frames(), monotonicMs());
                ScreenEvents.afterExtract(screen).register((s, g, mouseX, mouseY, delta) -> waiting.titleFrame());
            }
            ChatEmojiBar.attach(this, screen);
        });
        game = new MinecraftGame(this);
        ClientTickEvents.END_CLIENT_TICK.register(mc -> {
            emojiKeyTick(mc);
            if (resume != null) {
                resume.tick(game, WorldResume.frames(), monotonicMs());
                if (resume.pending() == null) {
                    resume = null;
                }
            }
            Shapes.prepare();
        });
        bridge = new BridgeClient(config, version(MOD_ID), version("minecraft"),
            () -> Texts.language().helloCode(), false, new BridgeClient.Listener() {
                @Override
                public void onStatus(BridgeClient.Status status, String reason) {
                    Minecraft.getInstance().execute(() -> {
                        state.setStatus(status, reason);
                        if (status != BridgeClient.Status.ONLINE) {
                            Texts.language().offline();
                        }
                        // Both switches went off during the game: nothing is left for it.
                        if (status == BridgeClient.Status.ENDED && "disabled".equals(reason)) {
                            emojis.endedByLauncher();
                        }
                    });
                }

                @Override
                public void onFrame(JsonObject frame) {
                    Minecraft.getInstance().execute(() -> handleFrame(frame));
                }
            });
        ClientPlayConnectionEvents.JOIN.register((handler, sender, mc) -> mc.execute(ChatEmojiBar::serverChanged));
        ClientLifecycleEvents.CLIENT_STARTED.register(mc -> {
            bridge.start();
            Shapes.prepare();
        });
        ClientLifecycleEvents.CLIENT_STOPPING.register(mc -> {
            bridge.stop();
            settings.close();
        });
        instance = this;
        LOG.info("Overlay's chat emoji picker on (the overlay itself is off); launcher bridge at 127.0.0.1:{}",
            config.port());
    }

    /** The running overlay, or null when the game was not started by Projects Launcher. */
    public static OverlayClient get() {
        return instance;
    }

    /** The overlay runs its panel (else the chat's emoji picker alone, CHAT-EMOJI). */
    public boolean panel() {
        return panel;
    }

    /** The chat's emoji picker: the launcher's switch and the server's emojis (CHAT-EMOJI). */
    @Override
    public EmojiState emojis() {
        return emojis;
    }

    @Override
    public String serverHost() {
        ServerTarget here = game.currentServer();
        return here == null ? null : here.host();
    }

    /** The chat emoji picker's key (F9 by default), in the game's Controls under "Projects" (CHAT-EMOJI). */
    @Override
    public EmojiKey emojiKey() {
        return emojiKey;
    }

    /**
     * The emoji key in the game with no screen open (CHAT-EMOJI): the chat opens the way T opens it (the game's own
     * checks for a chat that is off), with the picker open. On the chat screen the bar takes the key itself.
     */
    private void emojiKeyTick(Minecraft mc) {
        while (emojiKey.consumeClick()) {
            if (Compat.screen(mc) == null && mc.player != null && emojis.on()) {
                // The chat's first frame opens the picker (a chat the game keeps closed lets the wish lapse).
                ChatEmojiBar.openPickerOnNextChat();
                Compat.openChat(mc);
            }
        }
    }

    /**
     * The settings' "Sohbette emoji seçici" row (CHAT-EMOJI): the launcher's switch, shown in the game too; it takes a flip
     * while the launcher listens (it keeps it and answers with {@code emojis}).
     */
    public SettingsList.EmojiSwitch emojiSwitch() {
        return new SettingsList.EmojiSwitch() {
            @Override
            public boolean on() {
                return emojis.on();
            }

            @Override
            public boolean editable() {
                return emojis.heard() && state.online();
            }

            @Override
            public void set(boolean on) {
                if (emojis.heard() && bridge.send(Protocol.emoji(on))) {
                    emojis.switchTo(on);
                    refreshScreen();
                }
            }
        };
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

    /** Codeless Discord linking with a Projects server (SKIN1-O). */
    public CodelessLink link() {
        return link;
    }

    /** Opening the player's singleplayer world to friends (HOST1). */
    public HostController host() {
        return host;
    }

    /** The pause menu's "Arkadaşlarını çağır": the panel opens on world sharing and goes back to the menu. */
    public void openWorldSharing(Screen from) {
        if (bridge.status() == BridgeClient.Status.OFFLINE) {
            bridge.retryNow();
        }
        Compat.setScreen(Minecraft.getInstance(), new OverlayScreen(this, OverlayScreen.Page.WORLD, from));
    }

    /**
     * "Katıl" on a friend's world invite, confirmed (H15): the launcher prepares the matching install and then moves
     * the game ({@code connect} with the world's label) or restarts it. A short note says it is on its way.
     */
    public void joinWorld(String friendId) {
        if (!bridge.send(Protocol.join(friendId))) {
            return;
        }
        pendingWorld = friendId;
        pendingWorldAt = monotonicMs();
        toasts.notice(ToastCard.travel(Texts.str("world.join.preparing")), settings.get(), reducedMotion());
    }

    /**
     * "Projects'te buluşun", confirmed: the picked friends get a "Birlikte oyna" invite once the player is on Projects
     * (the launcher's part), and the game goes to Projects now (a singleplayer world is saved first).
     */
    public void meetOnProjects(List<String> friends) {
        bridge.send(Protocol.hostMeet(friends));
        connect(new JoinScreen.Target(null, "Projects", PROJECTS_HOST, true));
    }

    /** The game is in this friend's world now (it went there with their invite). */
    public boolean inWorldOf(String friendId) {
        if (friendId == null || !friendId.equals(worldFriend) || worldTarget == null) {
            return false;
        }
        ServerTarget here = game.currentServer();
        return here != null && here.sameServer(worldTarget);
    }

    /**
     * A note of world sharing for the host (the world opened, a friend came in or left); public for the scenario
     * driver, which shows them as a session would.
     */
    public void showHostNote(String title, String body, boolean sprite, String hintKey, String toastKey) {
        if (Compat.screen(Minecraft.getInstance()) instanceof OverlayScreen panel
            && panel.page() == OverlayScreen.Page.WORLD) {
            // The page shows it already.
            return;
        }
        toasts.host(ToastCard.host(title, body, sprite, hintKey, key.keyName()), toastKey, settings.get(),
            reducedMotion());
    }

    /**
     * A failed opening of the player's world (HOST1): one short line as a note while the panel is closed; the panel's
     * card and world page say it themselves.
     */
    public void showHostFailure(String line) {
        if (Compat.screen(Minecraft.getInstance()) instanceof OverlayScreen) {
            return;
        }
        showHostNote(line, null, true, null, "host-failed");
    }

    @Override
    public OverlaySettings settings() {
        return settings.get();
    }

    @Override
    public void updateSettings(UnaryOperator<OverlaySettings> change) {
        settings.set(change.apply(settings.get()));
    }

    /** The player's "Animasyonları azalt" or the launcher's "Hareketi azalt". */
    @Override
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
        emojiKeyTick(mc);
        boolean hasLevel = mc.level != null;
        if (hadLevel && !hasLevel) {
            toasts.reset();
            StickerArt.releaseAll();
        }
        if (!hadLevel && hasLevel) {
            warmIndex = 0;
        }
        hadLevel = hasLevel;
        if (resume != null) {
            resume.tick(game, WorldResume.frames(), monotonicMs());
            if (resume.pending() == null) {
                resume = null;
            }
        }
        connector.tick(monotonicMs());
        host.tick(mc);
        tickLink(mc);
        Shapes.prepare();
        StickerArt.tick(monotonicMs());
        warmFonts(mc);
    }

    /** Codeless linking: an unanswered ask runs out; the server's question comes up once the game is free for it. */
    private void tickLink(Minecraft mc) {
        long now = monotonicMs();
        link.tick(now);
        if (!link.waitingForScreen()) {
            // Nearly always: nothing to look at.
            return;
        }
        boolean free = Compat.screen(mc) == null && mc.level != null && mc.player != null
            && !Compat.overlayShowing(mc) && onProjects();
        LinkMessages.Question question = link.takeQuestion(now, free);
        if (question != null) {
            Compat.setScreen(mc, new DiscordLinkScreen(this, question));
        }
    }

    /** A message from the server on {@code projects:link} (the game's thread). Only Projects is ever answered. */
    private void onLinkMessage(String json) {
        long now = monotonicMs();
        switch (LinkMessages.read(json)) {
            case LinkMessages.Ask ask -> link.onAsk(now, onProjects(), Minecraft.getInstance().getUser().getName(),
                state.online() && state.has("link"));
            case LinkMessages.Question question -> link.onQuestion(now, question, onProjects());
            case LinkMessages.Other other -> {
            }
        }
    }

    /** The game left its server (or the proxy moved it): that connection's ask and question end. */
    private void leftServer() {
        link.onLeave();
        if (Compat.screen(Minecraft.getInstance()) instanceof DiscordLinkScreen screen) {
            screen.withdraw();
        }
    }

    /** "Evet, eşle" ({@code yes}) or "Hayır" on the codeless link question. */
    public void answerLink(LinkMessages.Question question, boolean yes) {
        link.answer(question, yes);
    }

    /** The codeless link question closed without an answer: "not now". */
    public void dismissLink(LinkMessages.Question question) {
        link.dismissed(question);
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
        if (swappingPanel) {
            return;
        }
        toasts.clear();
        bridge.send(Protocol.panel(true));
    }

    public void panelClosed() {
        if (swappingPanel) {
            return;
        }
        bridge.send(Protocol.panel(false));
        // No "yazıyor…" comes while the panel is closed: what was shown would only go stale.
        state.clearTyping();
        typedIn = null;
    }

    // ---- The overlay's language (OVL-LANG) ----------------------------------------------------------------

    /** The panel is being built anew in another language: it neither closes nor opens for the launcher. */
    private boolean swappingPanel;

    /** The open panel gives way to {@code next}, the same panel in the overlay's new language. */
    public void swapPanel(OverlayScreen next) {
        swappingPanel = true;
        try {
            Compat.setScreen(Minecraft.getInstance(), next);
        } finally {
            swappingPanel = false;
        }
    }

    /** The settings' "Dil" row: the overlay's language, kept by the launcher. */
    public SettingsList.Languages languages() {
        return new SettingsList.Languages() {
            @Override
            public Language current() {
                return Texts.current();
            }

            @Override
            public boolean editable() {
                return Texts.language().editable() && state.online();
            }

            @Override
            public void pick(Language language) {
                pickLanguage(language);
            }
        };
    }

    /**
     * The player picked the overlay's language in its settings: the launcher keeps it and answers with a welcome in it,
     * which is when the panel changes (never before: its words and the launcher's stay one language).
     */
    public void pickLanguage(Language language) {
        String code = Texts.language().pick(language, monotonicMs());
        if (code != null) {
            bridge.send(Protocol.lang(code));
        }
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

    // ---- The friend menu (OVL-UX) ------------------------------------------------------------------------

    /** "Birlikte oyna" to a friend (the launcher's play-together invite; on a server only). */
    public void inviteFriend(String friendId) {
        friendAction(friendId, Protocol.invite(friendId), "invite_failed");
    }

    /** "Discord'dan mesaj at": the launcher opens their profile (the Discord app when it is installed). */
    public void openDiscord(String friendId) {
        friendAction(friendId, Protocol.discord(friendId), "failed");
    }

    /** "Discord'dan mesaj at" on a crewmate who is not a friend (OVL-UX2, Q15): the name crew chat showed. */
    public void openDiscord(String id, String name) {
        personAction(id, name, null, Protocol.discord(id), "failed");
    }

    /** "Arkadaşlıktan çıkar", after the panel's question. */
    public void unfriend(String friendId) {
        friendAction(friendId, Protocol.unfriend(friendId), "failed");
    }

    /** "Engelle", after the panel's question. */
    public void block(String friendId) {
        friendAction(friendId, Protocol.block(friendId), "failed");
    }

    /** "Engelle" on someone the panel names (a crewmate from crew chat, OVL-UX2), after the panel's question. */
    public void block(String id, String name) {
        personAction(id, name, null, Protocol.block(id), "failed");
    }

    /** "Rapor et": a player's reason, a note for moderators, maybe a block too. */
    public void report(String friendId, String reason, String note, boolean alsoBlock) {
        state.reportSent(friendId, alsoBlock);
        OverlayState.Friend friend = state.friend(friendId);
        personAction(friendId, friend == null ? reportName(friendId) : friend.name(), null,
            Protocol.report(friendId, reason, note, alsoBlock), "report_failed");
    }

    /** Sends a friend menu frame; when it cannot go at all the panel says so at once. */
    private void friendAction(String friendId, JsonObject frame, String failedKey) {
        OverlayState.Friend friend = state.friend(friendId);
        personAction(friendId, friend == null ? "" : friend.name(), null, frame, failedKey);
    }

    /**
     * Sends a person menu frame about {@code id} (a friend, or a crewmate from crew chat), keeping their {@code name}
     * and what the answer's words name besides them ({@code detail}); when it cannot go at all the panel says so at
     * once.
     */
    private void personAction(String id, String name, String detail, JsonObject frame, String failedKey) {
        state.actionSent(id, name, detail);
        if (!bridge.send(frame)) {
            state.personNote(failedKey, name, true, System.currentTimeMillis());
        }
        refreshScreen();
    }

    // ---- The rest of the launcher's person menu (OVL-UX2) -------------------------------------------------

    /** Whom the report dialog is about while it is not a friend (a crewmate from crew chat), and their name. */
    private String reportedId;
    private String reportedName = "";

    /** The report dialog opens about someone who is not on the friend list (a crewmate): their name for the note. */
    public void reportingAbout(String id, String name) {
        reportedId = id;
        reportedName = name == null ? "" : name;
    }

    private String reportName(String id) {
        return id.equals(reportedId) ? reportedName : "";
    }

    /** "Favorilere ekle" ({@code on}) or "Favorilerden çıkar": the launcher keeps it; the row's star follows. */
    public void favorite(String friendId, boolean on) {
        markAction(friendId, Protocol.favorite(friendId, on), on ? "favorited" : "unfavorited");
    }

    /** "Çevrimiçi olunca bildir" on or off for a friend. */
    public void alert(String friendId, boolean on) {
        markAction(friendId, Protocol.alert(friendId, on), on ? "alert_on" : "alert_off");
    }

    /** A favourite or an online notice: said under the list at once (the launcher answers only a refusal). */
    private void markAction(String friendId, JsonObject frame, String doneKey) {
        OverlayState.Friend friend = state.friend(friendId);
        String name = friend == null ? "" : friend.name();
        state.actionSent(friendId, name, null);
        boolean sent = bridge.send(frame);
        state.personNote(sent ? doneKey : "failed", name, !sent, System.currentTimeMillis());
        refreshScreen();
    }

    /** "Kategoriye taşı": a category by id ({@code ""} for "Arkadaşlar"), and its name for the note. */
    public void moveFriend(String friendId, String group, String groupName) {
        OverlayState.Friend friend = state.friend(friendId);
        personAction(friendId, friend == null ? "" : friend.name(), groupName, Protocol.move(friendId, group),
            "failed");
    }

    /** "Ekibe davet et" to one crew, and its name for the note. */
    public void crewInvite(String friendId, String crew, String crewName) {
        OverlayState.Friend friend = state.friend(friendId);
        personAction(friendId, friend == null ? "" : friend.name(), crewName, Protocol.crewInvite(friendId, crew),
            "failed");
    }

    /** "Arkadaş ekle" or "İsteği kabul et" on a crewmate from crew chat. */
    public void addCrewmate(String id, String name) {
        personAction(id, name, null, Protocol.addFriend(id), "failed");
    }

    /**
     * The pause menu's Projects button (OVL-UX): opens the panel where it was last, as F7 does; closing it goes back to
     * the pause menu and the game stays paused.
     */
    public void openPanelFrom(Screen from) {
        if (bridge.status() == BridgeClient.Status.OFFLINE) {
            bridge.retryNow();
        }
        Compat.setScreen(Minecraft.getInstance(), new OverlayScreen(this, null, from));
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
        // The chat's emoji picker (CHAT-EMOJI, E3): its switch and the server's emojis.
        if ("emojis".equals(Protocol.type(frame))) {
            emojis.apply(frame);
            refreshScreen();
            return;
        }
        if (!panel) {
            // The picker alone hears only its words (and the reduced-motion choice) besides.
            if ("welcome".equals(Protocol.type(frame))) {
                Texts.language().welcome(Protocol.str(frame, "lang", null));
                state.apply(frame, System.currentTimeMillis());
            }
            return;
        }
        // World sharing's commands (HOST1): opening, closing, sending a guest off.
        if (host.onFrame(frame)) {
            return;
        }
        switch (Protocol.type(frame)) {
            case "connect" -> {
                noteWorldConnect(frame);
                connector.request(frame, monotonicMs());
                return;
            }
            // Codeless linking (SKIN1-O): the permit goes straight on to the server that asked, never into state.
            case "link-offer" -> {
                link.onOffer(Protocol.str(frame, "nonce", ""), Protocol.str(frame, "name", ""),
                    Protocol.num(frame, "expiresAt", 0L), System.currentTimeMillis());
                return;
            }
            case "link-clear" -> {
                link.onClear();
                return;
            }
            default -> {
            }
        }
        if ("welcome".equals(Protocol.type(frame))) {
            // The overlay's language (G3): the open panel builds itself anew in another one on its next tick.
            Texts.language().welcome(Protocol.str(frame, "lang", null));
        }
        OverlayState.HostView before = state.host();
        OverlayState.ToastRequest toast = state.apply(frame, System.currentTimeMillis());
        if (toast != null) {
            showToast(toast);
        }
        host.mirrorChanged(before, state.host());
        refreshScreen();
    }

    /** A {@code connect} with a label right after the player's "Katıl" on a world invite: that friend's world. */
    private void noteWorldConnect(JsonObject frame) {
        String label = Protocol.str(frame, "label", "");
        if (label.isEmpty() || pendingWorld == null || monotonicMs() - pendingWorldAt > 15 * 60_000L) {
            return;
        }
        worldFriend = pendingWorld;
        worldTarget = ServerTarget.of(Protocol.str(frame, "host", null), Protocol.num(frame, "port", -1));
        pendingWorld = null;
    }

    private void onBridgeStatus(BridgeClient.Status status, String reason) {
        host.onBridgeStatus(status);
        state.setStatus(status, reason);
        if (status != BridgeClient.Status.ONLINE) {
            // No launcher keeps a language pick until the next welcome.
            Texts.language().offline();
            toasts.clear();
            // No permit can come now: a server waiting for one hears there is none.
            link.onClear();
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
