package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.mojang.blaze3d.platform.InputConstants;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.Perf;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.compat.HostCompat;
import gg.projects.overlay.host.HostController;
import gg.projects.overlay.host.HostGate;
import gg.projects.overlay.host.HostSession;
import gg.projects.overlay.state.OverlayState;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.panel.ActionButton;
import gg.projects.overlay.ui.panel.ConfirmScreen;
import gg.projects.overlay.ui.panel.OverlayScreen;
import gg.projects.overlay.ui.panel.PanelInput;
import java.io.IOException;
import java.lang.reflect.Field;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.NetworkInterface;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.function.Function;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.input.MouseButtonInfo;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.core.UUIDUtil;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import net.minecraft.world.level.storage.LevelResource;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Development-only scenario for HOST1-M (opening a singleplayer world to friends), enabled with
 * {@code -Dprojects.overlay.dev.script=host} and the mock launcher's {@code --host}. In a flat test world: the pause
 * menu's button, the world page (picking friends, "Davet et"), the real publish on loopback (checked: published, our
 * session, no LAN announcement, nothing listening on another address), the mock launcher's mirror and its probes of
 * the gate, guests in the page, the remove question, the pause menu while hosting, the panel's card, the host's notes,
 * "Paylaşımı kapat", a friend's world invite (toast, card, question), a failed opening, English, and the travel toast
 * of a friend's world. With {@code -Dprojects.overlay.dev.guest=<name>} (and the mock's {@code --relay-as <name>}), a
 * real client joins as that guest first: it must be in the world and in the page, bound in the world's guest file,
 * in the publish's game mode and without commands; then it is removed and its retry must stay out. With a second
 * name ({@code <name>,<name2>}, the runner's guest plan to match) another friend's try with the first name is
 * refused, sharing opens again, the second guest joins and the host leaves the world with it inside (instead of the
 * travel toast). Results and screenshots go to {@code -Dprojects.overlay.dev.out}.
 */
public final class HostScenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("HOST1-M scenario");
    private static final String WORLD = "host-scenario";
    private static final double STEP_TIMEOUT_S = 25;

    /** One step: it runs once {@code after} seconds passed since the last, again each tick until it says it is done. */
    private record Act(String name, double after, double timeout, Function<Minecraft, Boolean> run) {
    }

    private final List<Act> acts = new ArrayList<>();
    private final JsonObject results = new JsonObject();
    private final JsonArray events = new JsonArray();
    private final JsonArray failures = new JsonArray();
    private final long runStarted = System.nanoTime();
    private Path out;
    private int index;
    private long stepStarted = System.nanoTime();
    private int shots;
    private OverlayClient overlay;
    private Screen pause;
    private Object reload;
    private boolean done;
    private CompletableFuture<JsonObject> serverAsk;
    private String guestId = "";
    private long leaveStarted;

    @Override
    public void onInitializeClient() {
        if (!"host".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "host-out")).toAbsolutePath();
        ToastLayer.pauseWhileAway = false;
        script();
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("HOST1-M scenario driver on; results in {}", out);
    }

    private void script() {
        // A slow start (resources, a busy machine) can take minutes to reach the title screen.
        act("title", 0, 240, mc -> {
            boolean ready = Compat.screen(mc) instanceof TitleScreen && !Compat.overlayShowing(mc);
            BridgeClient.Status status = overlay.bridge().status();
            if (!ready || secs() < 3 || (status != BridgeClient.Status.ONLINE && secs() < 30)) {
                return false;
            }
            results.addProperty("bridgeOnlineAtTitle", status == BridgeClient.Status.ONLINE);
            results.addProperty("welcomeHost", overlay.state().has("host"));
            openWorld(mc);
            return true;
        });
        act("loading", 0, 120, mc -> mc.level != null && mc.player != null && Compat.screen(mc) == null);
        act("settle", 4, mc -> {
            results.addProperty("minecraft", Compat.VERSION);
            results.addProperty("guiScale", mc.getWindow().getGuiScale());
            results.addProperty("window", mc.getWindow().getWidth() + "x" + mc.getWindow().getHeight());
            results.addProperty("canInvite", overlay.host().canInvite());
            mc.player.setXRot(20f);
            return true;
        });
        act("pause", 0.3, mc -> {
            pause = new PauseScreen(true);
            Compat.setScreen(mc, pause);
            return true;
        });
        act("shot-pause", 0.8, mc -> {
            results.addProperty("pauseButton", hasButton(Compat.screen(mc), "Arkadaşlarını çağır", "Invite friends"));
            shot(mc, "pause");
            return true;
        });
        act("open-page", 0.3, mc -> {
            overlay.openWorldSharing(pause);
            return true;
        });
        act("shot-pick", 0.8, mc -> {
            results.addProperty("pageOnOpen", panel(mc) == null ? "none" : panel(mc).hostMode());
            shot(mc, "pick");
            return true;
        });
        act("tick-ayse", 0.3, mc -> click(mc, panel(mc).hostRow("f-ayse")));
        act("tick-mert", 0.25, mc -> click(mc, panel(mc).hostRow("f-mert")));
        act("shot-picked", 0.4, mc -> {
            results.add("picks", strings(panel(mc).hostPicks()));
            shot(mc, "picked");
            return true;
        });
        act("invite", 0.3, mc -> {
            panel(mc).pressHost("primary");
            return true;
        });
        act("shot-opening", 0.1, mc -> {
            results.addProperty("pageAfterInvite", panel(mc).hostMode());
            shot(mc, "opening");
            return true;
        });
        act("wait-open", 0, mc -> {
            OverlayScreen p = panel(mc);
            if (p == null || !"OPEN".equals(p.hostMode())) {
                return false;
            }
            results.addProperty("openSeconds", round(secs()));
            checkPublish(mc);
            return true;
        });
        act("measure-open", 0.6, mc -> {
            shot(mc, "open");
            Perf.drain();
            return true;
        });
        act("measured-open", 3, mc -> {
            results.add("panelWorldOpen", perf(Perf.drain()));
            return true;
        });
        String[] guests = System.getProperty("projects.overlay.dev.guest", "").split(",");
        String guest = guests[0].trim();
        String guest2 = guests.length > 1 ? guests[1].trim() : "";
        if (!guest.isBlank()) {
            guestSteps(guest, !guest2.isBlank());
        }
        act("mirror", 3, mc -> {
            dev("host-mirror", "guests",
                "[{\"guest\":\"g1\",\"friend\":\"f-ayse\",\"label\":\"Ayşe\",\"name\":\"Notch\","
                + "\"state\":\"in\"},{\"guest\":\"g2\",\"friend\":\"f-mert\",\"label\":\"Mert\",\"state\":\"joining\"},"
                + "{\"guest\":\"\",\"friend\":\"f-deniz\",\"label\":\"Deniz\",\"state\":\"invited\"}]");
            return true;
        });
        act("shot-guests", 0.8, mc -> {
            shot(mc, "guests");
            return true;
        });
        act("hover-row", 0.2, mc -> {
            float[] row = panel(mc).hostRow("f-ayse");
            if (row == null) {
                fail("no guest row");
                return true;
            }
            PanelInput.pointAt(Math.round(row[0]), Math.round(row[1]));
            return true;
        });
        act("shot-row-hover", 0.4, mc -> {
            shot(mc, "row-hover");
            return true;
        });
        act("remove-ask", 0.2, mc -> {
            float[] row = panel(mc).hostRow("f-ayse");
            PanelInput.clearPointer();
            // The remove sign sits at the row's right end.
            return clickAt(mc, Math.round(row[2]), row[1]);
        });
        act("shot-remove-ask", 0.5, mc -> {
            results.addProperty("removeAsked", Compat.screen(mc) instanceof ConfirmScreen);
            shot(mc, "remove-ask");
            if (Compat.screen(mc) instanceof ConfirmScreen ask) {
                ask.onClose();
            }
            return true;
        });
        act("pause-hosting", 0.4, mc -> {
            if (panel(mc) != null) {
                panel(mc).close();
            }
            pause = new PauseScreen(true);
            Compat.setScreen(mc, pause);
            return true;
        });
        act("shot-pause-hosting", 0.8, mc -> {
            results.addProperty("vanillaShareDimmed", vanillaShareDimmed(Compat.screen(mc)));
            shot(mc, "pause-hosting");
            Compat.setScreen(mc, null);
            return true;
        });
        act("card", 0.5, mc -> {
            overlay.togglePanel();
            return true;
        });
        act("card-friends", 0.3, mc -> {
            panel(mc).showPage(OverlayScreen.Page.FRIENDS);
            return true;
        });
        act("shot-card", 0.8, mc -> {
            shot(mc, "card-open");
            panel(mc).close();
            return true;
        });
        act("notes", 0.5, mc -> {
            overlay.showHostNote(gg.projects.overlay.ui.Texts.str("host.opened"), null, true, "toast.hint.manage",
                "host-open");
            overlay.showHostNote("Ayşe", gg.projects.overlay.ui.Texts.str("host.joined"), false, null,
                "host-guest:Ayşe");
            return true;
        });
        act("shot-notes", 0.7, mc -> {
            shot(mc, "notes");
            overlay.toasts().clear();
            return true;
        });
        act("stop-page", 0.8, mc -> {
            overlay.openWorldSharing(null);
            return true;
        });
        act("stop-ask", 0.6, mc -> {
            panel(mc).pressHost("stop");
            return true;
        });
        act("shot-stop-ask", 0.5, mc -> {
            results.addProperty("stopAsked", Compat.screen(mc) instanceof ConfirmScreen);
            shot(mc, "stop-ask");
            if (Compat.screen(mc) instanceof ConfirmScreen ask) {
                ask.confirm();
            }
            return true;
        });
        act("stopped", 0.8, mc -> {
            IntegratedServer server = mc.getSingleplayerServer();
            results.addProperty("hostingAfterStop", overlay.host().hosting());
            results.addProperty("gateSessionAfterStop", HostGate.session() != null);
            results.addProperty("publishedAfterStop", server != null && server.isPublished());
            results.addProperty("pageAfterStop", panel(mc) == null ? "none" : panel(mc).hostMode());
            shot(mc, "stopped");
            if (panel(mc) != null) {
                panel(mc).close();
            }
            return true;
        });
        act("world-invite", 0.6, mc -> {
            dev("world-invite");
            return true;
        });
        act("shot-world-invite", 0.7, mc -> {
            shot(mc, "world-invite-toast");
            return true;
        });
        act("invite-card", 1.5, mc -> {
            overlay.togglePanel();
            return true;
        });
        act("invite-card-friends", 0.3, mc -> {
            panel(mc).showPage(OverlayScreen.Page.FRIENDS);
            return true;
        });
        act("shot-invite-card", 0.8, mc -> {
            shot(mc, "invite-card");
            panel(mc).pressHost("card-join");
            return true;
        });
        act("shot-join-ask", 0.5, mc -> {
            results.addProperty("joinAsked", Compat.screen(mc) instanceof ConfirmScreen);
            shot(mc, "join-ask");
            if (Compat.screen(mc) instanceof ConfirmScreen ask) {
                ask.onClose();
            }
            if (panel(mc) != null) {
                panel(mc).close();
            }
            return true;
        });
        act("failed", 0.6, mc -> {
            overlay.openWorldSharing(null);
            dev("host-fail");
            return true;
        });
        act("shot-failed", 0.8, mc -> {
            results.addProperty("pageAfterFailure", panel(mc) == null ? "none" : panel(mc).hostMode());
            shot(mc, "failed");
            panel(mc).close();
            return true;
        });
        act("english", 0.5, mc -> {
            if (reload == null) {
                mc.options.languageCode = "en_us";
                mc.getLanguageManager().setSelected("en_us");
                reload = mc.reloadResourcePacks();
                return false;
            }
            java.util.concurrent.CompletableFuture<?> f = (java.util.concurrent.CompletableFuture<?>) reload;
            return f.isDone() && !Compat.overlayShowing(mc);
        });
        act("en-page", 1, mc -> {
            overlay.openWorldSharing(null);
            return true;
        });
        act("shot-en-page", 0.8, mc -> {
            shot(mc, "en-failed");
            panel(mc).close();
            return true;
        });
        act("en-card", 0.5, mc -> {
            overlay.togglePanel();
            return true;
        });
        act("en-card-friends", 0.3, mc -> {
            panel(mc).showPage(OverlayScreen.Page.FRIENDS);
            return true;
        });
        act("shot-en-card", 0.8, mc -> {
            shot(mc, "en-invite-card");
            panel(mc).close();
            return true;
        });
        if (guest2.isBlank()) {
            act("travel", 0.6, mc -> {
                dev("world-connect");
                return true;
            });
            act("shot-travel", 0.35, mc -> {
                shot(mc, "travel");
                return true;
            });
        } else {
            leaveSteps(guest2);
        }
        act("quit", 4, mc -> {
            finish(mc);
            return true;
        });
    }

    /**
     * A real client (the AFK client, offline, as {@code name}) comes in through the mock launcher's relay: it must be
     * in the world and in the page, bound in the world's guest file, in the publish's game mode, without commands. Then
     * the host removes it; the client's retry (the runner starts it again) must stay out.
     */
    private void guestSteps(String name, boolean takenTry) {
        act("guest-in", 0, 150, mc -> {
            JsonObject facts = askServer(mc, server -> guestFacts(server, name));
            if (facts == null || !facts.get("in").getAsBoolean() || !mirrorShows(name)) {
                return false;
            }
            results.addProperty("guestInSeconds", round(secs()));
            return true;
        });
        act("guest-facts", 1, mc -> {
            JsonObject facts = askServer(mc, server -> guestFacts(server, name));
            if (facts == null) {
                return false;
            }
            results.add("guest", facts);
            guestId = facts.has("guestId") ? facts.get("guestId").getAsString() : "";
            return true;
        });
        act("shot-guest-in", 0.3, mc -> {
            results.addProperty("pageWithGuest", panel(mc) == null ? "none" : panel(mc).hostMode());
            shot(mc, "real-guest-in");
            return true;
        });
        act("guest-remove-ask", 0.3, mc -> {
            float[] row = panel(mc) == null ? null : panel(mc).hostRow("f-ayse");
            if (row == null) {
                fail("no row for the real guest");
                return true;
            }
            return clickAt(mc, Math.round(row[2]), row[1]);
        });
        act("shot-guest-remove-ask", 0.5, mc -> {
            results.addProperty("guestRemoveAsked", Compat.screen(mc) instanceof ConfirmScreen);
            shot(mc, "real-guest-remove-ask");
            if (Compat.screen(mc) instanceof ConfirmScreen ask) {
                ask.confirm();
            }
            return true;
        });
        act("guest-gone", 0, 20, mc -> {
            JsonObject facts = askServer(mc, server -> guestFacts(server, name));
            if (facts == null || facts.get("in").getAsBoolean()) {
                return false;
            }
            results.addProperty("guestGoneSeconds", round(secs()));
            HostSession s = HostGate.session();
            results.addProperty("guestRemovedInSession", s != null && !guestId.isEmpty() && s.removed(guestId));
            return true;
        });
        act("shot-guest-removed", 0.8, mc -> {
            shot(mc, "real-guest-removed");
            return true;
        });
        act("guest-retry", 0, 90, mc -> Files.exists(out.resolve("guest-done-2")));
        act("guest-after-retry", 0.5, mc -> {
            JsonObject facts = askServer(mc, server -> guestFacts(server, name));
            if (facts == null) {
                return false;
            }
            results.addProperty("guestInAfterRetry", facts.get("in").getAsBoolean());
            return true;
        });
        if (takenTry) {
            // Another friend's guest asks for the same name: bound to the first friend in this world.
            act("guest-taken", 0, 90, mc -> Files.exists(out.resolve("guest-done-3")));
            act("guest-after-taken", 0.3, mc -> {
                JsonObject facts = askServer(mc, server -> guestFacts(server, name));
                if (facts == null) {
                    return false;
                }
                results.addProperty("guestInAfterTaken", facts.get("in").getAsBoolean());
                return true;
            });
        }
    }

    /**
     * The friend menu's way (owner, 24 Oct: an invite from the overlay in a singleplayer world opens it by itself):
     * from the friends page an invite to Deniz is refused first (the card says why), then opens the world; one more
     * invite goes while it is open. Then the second guest comes in through its own relay, and the host leaves the world
     * with it inside: the world must close without waiting, the guest must read why (checked in the client's log).
     */
    private void leaveSteps(String name) {
        act("refuse-next", 0.6, mc -> {
            dev("host-refuse", "error", "limit");
            overlay.togglePanel();
            return true;
        });
        act("friends-page", 0.4, mc -> {
            panel(mc).showPage(OverlayScreen.Page.FRIENDS);
            return true;
        });
        act("invite-friend-refused", 0.4, mc -> {
            results.addProperty("inviteFriendRefused", overlay.host().inviteFriend("f-deniz").name());
            return true;
        });
        act("refused-card", 0, mc -> {
            if (overlay.host().failure() == null) {
                return false;
            }
            results.addProperty("failureAfterRefusal", overlay.host().failure());
            return true;
        });
        act("shot-refused-card", 0.5, mc -> {
            shot(mc, "invite-friend-refused");
            return true;
        });
        act("invite-friend", 0.3, mc -> {
            HostController.WorldInvite r = overlay.host().inviteFriend("f-deniz");
            results.addProperty("inviteFriend", r.name());
            return true;
        });
        act("shot-card-opening", 0.15, mc -> {
            shot(mc, "invite-friend-opening");
            return true;
        });
        act("reopen-open", 0, mc -> {
            OverlayState.HostView v = overlay.state().host();
            if (!overlay.host().hosting() || v == null || !v.open()) {
                return false;
            }
            IntegratedServer server = mc.getSingleplayerServer();
            results.addProperty("reopened", server != null && server.isPublished() && HostGate.session() != null);
            return true;
        });
        act("shot-card-open", 0.5, mc -> {
            shot(mc, "invite-friend-open");
            return true;
        });
        act("invite-friend-more", 0.3, mc -> {
            results.addProperty("inviteFriendWhileOpen", overlay.host().inviteFriend("f-mert").name());
            return true;
        });
        act("guest2-go", 0.3, mc -> {
            try {
                Files.writeString(out.resolve("guest-go-" + name), "1\n", StandardCharsets.UTF_8);
            } catch (IOException e) {
                fail("could not let the second guest go: " + e);
            }
            return true;
        });
        act("guest2-in", 0, 120, mc -> {
            JsonObject facts = askServer(mc, server -> guestFacts(server, name));
            if (facts == null || !facts.get("in").getAsBoolean() || !mirrorShows(name)) {
                return false;
            }
            facts.remove("bindingText");
            results.add("guest2", facts);
            return true;
        });
        act("shot-guest2-in", 0.8, mc -> {
            shot(mc, "real-guest2-in");
            return true;
        });
        act("leave-pause", 0.3, mc -> {
            if (panel(mc) != null) {
                panel(mc).close();
            }
            pause = new PauseScreen(true);
            Compat.setScreen(mc, pause);
            return true;
        });
        act("leave-press", 0.6, mc -> {
            net.minecraft.client.gui.components.Button quit = vanillaButton(Compat.screen(mc), "menu.returnToMenu",
                "menu.disconnect");
            if (quit == null) {
                fail("no button back to the title screen");
                return true;
            }
            leaveStarted = System.nanoTime();
            quit.mouseClicked(new MouseButtonEvent(quit.getX() + quit.getWidth() / 2.0,
                quit.getY() + quit.getHeight() / 2.0, new MouseButtonInfo(InputConstants.MOUSE_BUTTON_LEFT, 0)), false);
            event("click", "back to title");
            return true;
        });
        act("left-world", 0, 60, mc -> {
            if (mc.level != null || mc.getSingleplayerServer() != null || !(Compat.screen(mc) instanceof TitleScreen)) {
                return false;
            }
            results.addProperty("leftWorldSeconds", round((System.nanoTime() - leaveStarted) / 1e9));
            results.addProperty("hostingAfterLeave", overlay.host().hosting());
            results.addProperty("gateSessionAfterLeave", HostGate.session() != null);
            return true;
        });
        act("guest2-out", 0, 30, mc -> Files.exists(out.resolve("guest-done-4")));
    }

    private static net.minecraft.client.gui.components.Button vanillaButton(Screen screen, String... keys) {
        if (screen == null) {
            return null;
        }
        for (GuiEventListener child : screen.children()) {
            if (child instanceof net.minecraft.client.gui.components.Button b
                && b.getMessage().getContents() instanceof net.minecraft.network.chat.contents.TranslatableContents t) {
                for (String key : keys) {
                    if (key.equals(t.getKey())) {
                        return b;
                    }
                }
            }
        }
        return null;
    }

    /** The launcher's mirror (here the mock's) shows the guest in the world. */
    private boolean mirrorShows(String name) {
        OverlayState.HostView view = overlay.state().host();
        if (view == null) {
            return false;
        }
        for (OverlayState.HostGuest g : view.guests()) {
            if (g.in() && name.equalsIgnoreCase(g.name())) {
                return true;
            }
        }
        return false;
    }

    /** Asks the world's server a question on its own thread: null until it answered (ask again next tick). */
    private JsonObject askServer(Minecraft mc, Function<IntegratedServer, JsonObject> question) {
        IntegratedServer server = mc.getSingleplayerServer();
        if (server == null) {
            JsonObject o = new JsonObject();
            o.addProperty("in", false);
            o.addProperty("server", false);
            return o;
        }
        if (serverAsk == null) {
            serverAsk = server.submit(() -> question.apply(server));
            return null;
        }
        if (!serverAsk.isDone()) {
            return null;
        }
        CompletableFuture<JsonObject> answered = serverAsk;
        serverAsk = null;
        try {
            return answered.join();
        } catch (RuntimeException e) {
            JsonObject o = new JsonObject();
            o.addProperty("in", false);
            o.addProperty("error", e.toString());
            return o;
        }
    }

    /** On the server thread: where the guest {@code name} stands. */
    private static JsonObject guestFacts(IntegratedServer server, String name) {
        JsonObject o = new JsonObject();
        ServerPlayer p = server.getPlayerList().getPlayerByName(name);
        o.addProperty("in", p != null);
        o.addProperty("players", server.getPlayerCount());
        HostSession s = HostGate.session();
        o.addProperty("publishMode", s == null ? "" : s.publish().mode());
        o.addProperty("publishCommands", s != null && s.publish().commands());
        if (p != null) {
            o.addProperty("gameMode", p.gameMode.getGameModeForPlayer().getName());
            o.addProperty("language", String.valueOf(HostCompat.language(p)));
            o.addProperty("offlineUuid", p.getUUID().equals(UUIDUtil.createOfflinePlayerUUID(name)));
            HostSession.Guest g = s == null ? null : s.byUuid(p.getUUID());
            o.addProperty("guestId", g == null ? "" : g.identity.guest());
            o.addProperty("stage", g == null ? "" : g.stage().name());
            boolean commands;
            try {
                server.getCommands().getDispatcher().execute("time query daytime",
                    p.createCommandSourceStack().withSuppressedOutput());
                commands = true;
            } catch (CommandSyntaxException e) {
                commands = false;
            }
            o.addProperty("guestCommands", commands);
        }
        Path file = server.getWorldPath(LevelResource.ROOT).resolve("projects-guests.json");
        o.addProperty("bindingFile", Files.exists(file));
        if (Files.exists(file)) {
            try {
                String text = Files.readString(file, StandardCharsets.UTF_8);
                o.addProperty("bindingHasName", text.contains("\"" + name + "\""));
                o.addProperty("bindingText", text.replaceAll("\\s+", " "));
            } catch (IOException e) {
                o.addProperty("bindingError", e.toString());
            }
        }
        return o;
    }

    private void act(String name, double after, Function<Minecraft, Boolean> run) {
        acts.add(new Act(name, after, STEP_TIMEOUT_S, run));
    }

    private void act(String name, double after, double timeout, Function<Minecraft, Boolean> run) {
        acts.add(new Act(name, after, timeout, run));
    }

    private void tick(Minecraft mc) {
        if (done) {
            return;
        }
        if (overlay == null) {
            overlay = OverlayClient.get();
            if (overlay == null) {
                return;
            }
        }
        mc.options.pauseOnLostFocus = false;
        if (index >= acts.size()) {
            return;
        }
        Act a = acts.get(index);
        if (secs() < a.after()) {
            return;
        }
        boolean finished;
        try {
            finished = a.run().apply(mc);
        } catch (RuntimeException e) {
            LOG.error("step {} threw", a.name(), e);
            fail(a.name() + ": " + e);
            finished = true;
        }
        if (!finished && secs() - a.after() > a.timeout()) {
            fail(a.name() + ": timed out");
            finished = true;
        }
        if (finished) {
            event("step", a.name());
            index++;
            stepStarted = System.nanoTime();
        }
    }

    private double secs() {
        return (System.nanoTime() - stepStarted) / 1e9;
    }

    private static OverlayScreen panel(Minecraft mc) {
        return Compat.screen(mc) instanceof OverlayScreen p ? p : null;
    }

    private boolean click(Minecraft mc, float[] at) {
        if (at == null) {
            fail("nothing to click");
            return true;
        }
        return clickAt(mc, Math.round(at[0]), at[1]);
    }

    private boolean clickAt(Minecraft mc, int x, float y) {
        Screen screen = Compat.screen(mc);
        if (screen == null) {
            fail("no screen to click");
            return true;
        }
        // 26.3 numbers the mouse buttons from 1: the version's own constant.
        MouseButtonEvent press = new MouseButtonEvent(x, Math.round(y),
            new MouseButtonInfo(InputConstants.MOUSE_BUTTON_LEFT, 0));
        screen.mouseClicked(press, false);
        screen.mouseReleased(press);
        event("click", x + "," + Math.round(y));
        return true;
    }

    /** The world is published by our session, on loopback only, with no LAN announcement. */
    private void checkPublish(Minecraft mc) {
        IntegratedServer server = mc.getSingleplayerServer();
        results.addProperty("published", server != null && server.isPublished());
        results.addProperty("gateSession", HostGate.session() != null);
        if (server == null) {
            return;
        }
        int port = server.getPort();
        results.addProperty("portOnLoopback", connects(InetAddress.getLoopbackAddress(), port));
        // Nothing may answer on this computer's other addresses.
        boolean elsewhere = false;
        int tried = 0;
        try {
            for (NetworkInterface nic : Collections.list(NetworkInterface.getNetworkInterfaces())) {
                if (!nic.isUp() || nic.isLoopback()) {
                    continue;
                }
                for (InetAddress address : Collections.list(nic.getInetAddresses())) {
                    tried++;
                    elsewhere |= connects(address, port);
                }
            }
        } catch (IOException e) {
            LOG.warn("could not list the network interfaces", e);
        }
        results.addProperty("otherAddressesTried", tried);
        results.addProperty("answersElsewhere", elsewhere);
        try {
            Field pinger = IntegratedServer.class.getDeclaredField("lanPinger");
            pinger.setAccessible(true);
            results.addProperty("lanPingerNull", pinger.get(server) == null);
        } catch (ReflectiveOperationException e) {
            results.addProperty("lanPingerNull", "unknown: " + e.getClass().getSimpleName());
        }
    }

    private static boolean connects(InetAddress address, int port) {
        try (Socket socket = new Socket()) {
            socket.connect(new InetSocketAddress(address, port), 400);
            return true;
        } catch (IOException e) {
            return false;
        }
    }

    private static boolean hasButton(Screen screen, String... labels) {
        if (screen == null) {
            return false;
        }
        for (GuiEventListener child : screen.children()) {
            if (child instanceof ActionButton b) {
                for (String label : labels) {
                    if (b.getMessage().getString().equals(label)) {
                        return true;
                    }
                }
            }
        }
        return false;
    }

    private static boolean vanillaShareDimmed(Screen screen) {
        if (screen == null || gg.projects.overlay.compat.HostCompat.VANILLA_SHARE_KEY == null) {
            return false;
        }
        for (GuiEventListener child : screen.children()) {
            if (child instanceof net.minecraft.client.gui.components.Button b
                && b.getMessage().getContents() instanceof net.minecraft.network.chat.contents.TranslatableContents t
                && gg.projects.overlay.compat.HostCompat.VANILLA_SHARE_KEY.equals(t.getKey())) {
                return !b.active;
            }
        }
        return false;
    }

    private void dev(String cmd, String... pairs) {
        JsonObject o = Protocol.message("dev");
        o.addProperty("cmd", cmd);
        for (int i = 0; i + 1 < pairs.length; i += 2) {
            o.addProperty(pairs[i], pairs[i + 1]);
        }
        boolean sent = overlay.bridge().send(o);
        event("dev", cmd + (sent ? "" : " (not sent)"));
    }

    private void openWorld(Minecraft mc) {
        if (mc.getLevelSource().levelExists(WORLD)) {
            event("world", "open");
            mc.createWorldOpenFlows().openWorld(WORLD, () -> Compat.setScreen(mc, new TitleScreen()));
            return;
        }
        event("world", "create");
        LevelSettings settings = new LevelSettings("HOST1-M scenario", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261024L, false, false),
            DevCompat.flatWorld(), new TitleScreen());
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("host-%02d-%s.png", ++shots, name);
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

    private static JsonArray strings(List<String> list) {
        JsonArray a = new JsonArray();
        list.forEach(a::add);
        return a;
    }

    private void event(String kind, String value) {
        JsonObject e = new JsonObject();
        e.addProperty("t", round((System.nanoTime() - runStarted) / 1e9));
        e.addProperty(kind, value);
        events.add(e);
        LOG.info("{}: {}", kind, value);
    }

    private void fail(String why) {
        failures.add(why);
        LOG.error("scenario check failed: {}", why);
    }

    private void finish(Minecraft mc) {
        done = true;
        results.add("failures", failures);
        results.addProperty("runSeconds", round((System.nanoTime() - runStarted) / 1e9));
        results.add("events", events);
        try {
            Files.createDirectories(out);
            Files.writeString(out.resolve("host-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
        mc.stop();
    }

    private static double round(double v) {
        return Math.round(v * 1000.0) / 1000.0;
    }
}
