package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.Perf;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.bridge.Protocol;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.host.HostGate;
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
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Development-only scenario for HOST1-M (opening a singleplayer world to friends), enabled with
 * {@code -Dprojects.overlay.dev.script=host} and the mock launcher's {@code --host}. In a flat test world: the pause
 * menu's button, the world page (picking friends, "Davet et"), the real publish on loopback (checked: published, our
 * session, no LAN announcement, nothing listening on another address), the mock launcher's mirror and its probes of
 * the gate, guests in the page, the remove question, the pause menu while hosting, the panel's card, the host's notes,
 * "Paylaşımı kapat", a friend's world invite (toast, card, question), a failed opening, English, and the travel toast
 * of a friend's world. Results and screenshots go to {@code -Dprojects.overlay.dev.out}.
 */
public final class HostScenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("HOST1-M scenario");
    private static final String WORLD = "host-scenario";
    private static final double STEP_TIMEOUT_S = 25;

    /** One step: it runs once {@code after} seconds passed since the last, again each tick until it says it is done. */
    private record Act(String name, double after, Function<Minecraft, Boolean> run) {
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
        act("title", 0, mc -> {
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
        act("loading", 0, mc -> mc.level != null && mc.player != null && Compat.screen(mc) == null);
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
        act("mirror", 3, mc -> {
            dev("host-mirror", "guests", "[{\"guest\":\"g1\",\"friend\":\"f-ayse\",\"label\":\"Ayşe\",\"name\":\"Notch\","
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
        act("travel", 0.6, mc -> {
            dev("world-connect");
            return true;
        });
        act("shot-travel", 0.35, mc -> {
            shot(mc, "travel");
            return true;
        });
        act("quit", 4, mc -> {
            finish(mc);
            return true;
        });
    }

    private void act(String name, double after, Function<Minecraft, Boolean> run) {
        acts.add(new Act(name, after, run));
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
        if (!finished && secs() - a.after() > STEP_TIMEOUT_S) {
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
        MouseButtonEvent press = new MouseButtonEvent(x, Math.round(y), new MouseButtonInfo(0, 0));
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
