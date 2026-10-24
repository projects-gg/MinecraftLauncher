package gg.projects.overlay.dev;

import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.blaze3d.platform.InputConstants;
import gg.projects.overlay.OverlayClient;
import gg.projects.overlay.bridge.BridgeClient;
import gg.projects.overlay.compat.Compat;
import gg.projects.overlay.link.LinkPayload;
import gg.projects.overlay.toast.ToastLayer;
import gg.projects.overlay.ui.panel.DiscordLinkScreen;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.regex.Pattern;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.networking.v1.ServerPlayNetworking;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Development-only scenario for SKIN1-O (codeless Discord linking), enabled with
 * {@code -Dprojects.overlay.dev.script=link} and the mock launcher's {@code --link}. In a flat test world the world's
 * own server plays ProBotLinker on {@code projects:link} (the dev mixin lets the overlay take it for Projects): it
 * asks, hears {@code pending} and the launcher's permit ({@code intent} with {@code ui}), sends the question as
 * ProBot answered it, and hears the player's answer. The real mod, the real game's networking and the mock
 * launcher's bridge run end to end. Rounds: "Evet, eşle" with the keys after an early Enter and an early press that
 * must not count; Escape (nothing goes); long names; English with "No". Results and screenshots go to
 * {@code -Dprojects.overlay.dev.out}.
 */
public final class LinkScenario implements ClientModInitializer {
    private static final Logger LOG = LoggerFactory.getLogger("SKIN1-O scenario");
    private static final String WORLD = "link-scenario";
    private static final String ASK = "{\"t\":\"ask\",\"v\":1}";
    private static final String TOKEN = "Ab3_x-Yz9Q";
    private static final Pattern NONCE = Pattern.compile("^i[A-Za-z0-9_-]{32}$");

    private enum Step { TITLE, LOADING, SETTLE, ROUND, QUIT, DONE }

    /** One question: whose Discord it names, what the driver does on it, and in which language. */
    private record Round(String name, String displayName, String username, String action, boolean english) {
    }

    private static final List<Round> ROUNDS = List.of(
        new Round("tr", "Ayşe Yılmaz", "ayse.yilmaz", "keys-yes", false),
        new Round("tr-escape", "Ayşe Yılmaz", "ayse.yilmaz", "escape", false),
        new Round("tr-long", "Çağlayan Gökdemir Şahinoğlu Ünal", "caglayan.gokdemir.sahinoglu42", "escape", false),
        new Round("en", "Ayşe Yılmaz", "ayse.yilmaz", "keys-no", true));

    private final JsonObject results = new JsonObject();
    private final JsonArray events = new JsonArray();
    private final JsonArray rounds = new JsonArray();
    private final long runStarted = System.nanoTime();
    /** What the world's server heard on {@code projects:link}, per round (the server thread adds). */
    private final JsonArray heard = new JsonArray();
    private volatile Round current;
    private Path out;
    private Step step = Step.TITLE;
    private long stepStarted = System.nanoTime();
    private int stage;
    private int shots;
    private int roundIndex;
    private long askedAt;
    private long shownAt;
    private JsonObject round;
    private CompletableFuture<Void> reload;

    @Override
    public void onInitializeClient() {
        if (!"link".equals(System.getProperty("projects.overlay.dev.script"))) {
            return;
        }
        out = Path.of(System.getProperty("projects.overlay.dev.out", "link-out")).toAbsolutePath();
        ToastLayer.pauseWhileAway = false;
        // The world's own server stands in for ProBotLinker: it answers a permit with the question ProBot would
        // have made of it, and notes everything it hears.
        ServerPlayNetworking.registerGlobalReceiver(LinkPayload.TYPE, (payload, context) -> {
            JsonObject heardNow = describe(payload.json());
            synchronized (heard) {
                heard.add(heardNow);
            }
            Round r = current;
            if ("intent".equals(heardNow.get("t").getAsString()) && r != null) {
                JsonObject question = new JsonObject();
                question.addProperty("t", "question");
                question.addProperty("v", 1);
                question.addProperty("token", TOKEN);
                question.addProperty("displayName", r.displayName());
                question.addProperty("username", r.username());
                ServerPlayNetworking.send(context.player(), new LinkPayload(question.toString()));
            }
        });
        ClientTickEvents.END_CLIENT_TICK.register(this::tick);
        LOG.info("SKIN1-O scenario driver on; results in {}", out);
    }

    /** A heard message as the results keep it: its type and what was right about it (never the permit itself). */
    private static JsonObject describe(String json) {
        JsonObject o = new JsonObject();
        try {
            JsonObject m = JsonParser.parseString(json).getAsJsonObject();
            String t = m.has("t") ? m.get("t").getAsString() : "?";
            o.addProperty("t", t);
            if ("intent".equals(t)) {
                o.addProperty("ui", m.has("ui") ? m.get("ui").getAsInt() : 0);
                o.addProperty("permitWellFormed", m.has("nonce") && NONCE.matcher(m.get("nonce").getAsString()).matches());
            } else if ("answer".equals(t)) {
                o.addProperty("answer", m.get("answer").getAsString());
                o.addProperty("token", TOKEN.equals(m.get("token").getAsString()));
            }
            o.addProperty("bytes", json.getBytes(StandardCharsets.UTF_8).length);
        } catch (RuntimeException e) {
            o.addProperty("t", "unreadable");
        }
        return o;
    }

    private double secs() {
        return (System.nanoTime() - stepStarted) / 1e9;
    }

    private void go(Step next) {
        event("step", next.name());
        step = next;
        stepStarted = System.nanoTime();
        stage = 0;
    }

    private boolean at(int index, double atSeconds) {
        if (stage == index && secs() >= atSeconds) {
            stage++;
            return true;
        }
        return false;
    }

    private void tick(Minecraft mc) {
        OverlayClient overlay = OverlayClient.get();
        if (overlay == null) {
            return;
        }
        mc.options.pauseOnLostFocus = false;
        BridgeClient.Status status = overlay.bridge().status();
        Screen screen = Compat.screen(mc);
        switch (step) {
            case TITLE -> {
                boolean ready = screen instanceof TitleScreen && !Compat.overlayShowing(mc);
                if (ready && secs() > 3 && (status == BridgeClient.Status.ONLINE || secs() > 30)) {
                    results.addProperty("bridgeOnlineAtTitle", status == BridgeClient.Status.ONLINE);
                    results.addProperty("welcomeLink", overlay.state().has("link"));
                    openWorld(mc);
                    go(Step.LOADING);
                }
            }
            case LOADING -> {
                if (mc.level != null && mc.player != null && screen == null) {
                    results.addProperty("worldLoadSeconds", round(secs()));
                    go(Step.SETTLE);
                } else if (secs() > 180) {
                    fail(mc, "world did not load");
                }
            }
            case SETTLE -> {
                if (secs() > 5) {
                    results.addProperty("minecraft", Compat.VERSION);
                    results.addProperty("guiScale", mc.getWindow().getGuiScale());
                    results.addProperty("window", mc.getWindow().getWidth() + "x" + mc.getWindow().getHeight());
                    mc.player.setXRot(20f);
                    startRound();
                }
            }
            case ROUND -> round(mc, overlay, screen);
            case QUIT -> {
                if (secs() > 1) {
                    results.add("rounds", rounds);
                    results.addProperty("runSeconds", round((System.nanoTime() - runStarted) / 1e9));
                    write();
                    go(Step.DONE);
                    mc.stop();
                }
            }
            case DONE -> {
            }
        }
    }

    private void startRound() {
        current = ROUNDS.get(roundIndex);
        synchronized (heard) {
            while (!heard.isEmpty()) {
                heard.remove(0);
            }
        }
        round = new JsonObject();
        round.addProperty("name", current.name());
        shownAt = 0;
        go(Step.ROUND);
    }

    private void round(Minecraft mc, OverlayClient overlay, Screen screen) {
        Round r = current;
        // 0: the language; 1: the ask; 2: the question comes up; 3+: on the question; then the end of the round.
        if (stage == 0) {
            boolean english = "en_us".equals(mc.options.languageCode);
            if (r.english() != english && reload == null) {
                mc.options.languageCode = r.english() ? "en_us" : "tr_tr";
                mc.getLanguageManager().setSelected(mc.options.languageCode);
                reload = mc.reloadResourcePacks();
                return;
            }
            if (reload != null && (!reload.isDone() || Compat.overlayShowing(mc))) {
                return;
            }
            reload = null;
            overlay.toasts().clear();
            stage = 1;
            stepStarted = System.nanoTime();
            return;
        }
        if (stage == 1 && secs() > 1.0) {
            stage = 2;
            askedAt = System.nanoTime();
            MinecraftServer server = mc.getSingleplayerServer();
            if (server == null) {
                fail(mc, "no world server");
                return;
            }
            server.execute(() -> {
                List<ServerPlayer> players = server.getPlayerList().getPlayers();
                if (!players.isEmpty()) {
                    ServerPlayNetworking.send(players.get(0), new LinkPayload(ASK));
                }
            });
            event("ask", r.name());
            return;
        }
        if (stage == 2) {
            if (screen instanceof DiscordLinkScreen) {
                shownAt = System.nanoTime();
                round.addProperty("askToQuestionMs", round((shownAt - askedAt) / 1e6));
                stage = 3;
                event("question", r.name());
            } else if ((System.nanoTime() - askedAt) / 1e9 > 15) {
                round.addProperty("failed", "no question");
                endRound(mc);
            }
            return;
        }
        if (stage < 3) {
            return;
        }
        double on = (System.nanoTime() - shownAt) / 1e9;
        DiscordLinkScreen question = screen instanceof DiscordLinkScreen s ? s : null;
        switch (stage) {
            case 3 -> {
                if (on >= 0.10) {
                    shot(mc, r.name() + "-enter");
                    stage++;
                }
            }
            case 4 -> {
                if (on >= 0.20) {
                    // A reflexive Enter right as it appears: nothing has the focus, nothing happens.
                    if (question != null) {
                        question.keyPressed(new KeyEvent(InputConstants.KEY_RETURN, 0, 0));
                    }
                    stage++;
                }
            }
            case 5 -> {
                if (on >= 0.30) {
                    // A key straight on "Evet, eşle" before the question is armed: not an answer either.
                    GuiEventListener yes = question == null || question.children().size() < 2 ? null
                        : question.children().get(1);
                    if (yes != null) {
                        yes.keyPressed(new KeyEvent(InputConstants.KEY_RETURN, 0, 0));
                    }
                    round.addProperty("earlyKeysIgnored", Compat.screen(mc) instanceof DiscordLinkScreen);
                    stage++;
                }
            }
            case 6 -> {
                if (on >= 1.0) {
                    shot(mc, r.name());
                    stage++;
                }
            }
            case 7 -> {
                if (on >= 1.2 && question != null) {
                    switch (r.action()) {
                        case "escape" -> question.keyPressed(new KeyEvent(InputConstants.KEY_ESCAPE, 0, 0));
                        case "keys-no" -> question.keyPressed(new KeyEvent(InputConstants.KEY_TAB, 0, 0));
                        default -> {
                            question.keyPressed(new KeyEvent(InputConstants.KEY_TAB, 0, 0));
                            question.keyPressed(new KeyEvent(InputConstants.KEY_TAB, 0, 0));
                        }
                    }
                    stage++;
                } else if (question == null) {
                    round.addProperty("failed", "the question closed by itself");
                    endRound(mc);
                }
            }
            case 8 -> {
                if (on >= 1.5) {
                    if (!"escape".equals(r.action())) {
                        shot(mc, r.name() + "-focus");
                        if (question != null) {
                            question.keyPressed(new KeyEvent(InputConstants.KEY_RETURN, 0, 0));
                        }
                    }
                    stage++;
                }
            }
            default -> {
                if (on >= 3.0) {
                    round.addProperty("closed", !(Compat.screen(mc) instanceof DiscordLinkScreen));
                    endRound(mc);
                }
            }
        }
    }

    private void endRound(Minecraft mc) {
        if (Compat.screen(mc) instanceof DiscordLinkScreen s) {
            s.onClose();
        }
        synchronized (heard) {
            round.add("heard", heard.deepCopy());
        }
        rounds.add(round);
        event("round", round.toString());
        roundIndex++;
        if (roundIndex < ROUNDS.size()) {
            startRound();
        } else {
            current = null;
            go(Step.QUIT);
        }
    }

    private void openWorld(Minecraft mc) {
        if (mc.getLevelSource().levelExists(WORLD)) {
            event("world", "open");
            mc.createWorldOpenFlows().openWorld(WORLD, () -> Compat.setScreen(mc, new TitleScreen()));
            return;
        }
        event("world", "create");
        LevelSettings settings = new LevelSettings("SKIN1-O scenario", GameType.CREATIVE,
            new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true,
            WorldDataConfiguration.DEFAULT);
        mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261024L, false, false),
            DevCompat.flatWorld(), new TitleScreen());
    }

    private void shot(Minecraft mc, String name) {
        String file = String.format("link-%02d-%s.png", ++shots, name);
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

    private void fail(Minecraft mc, String why) {
        results.addProperty("failed", why);
        LOG.error("scenario failed: {}", why);
        results.add("rounds", rounds);
        write();
        go(Step.DONE);
        mc.stop();
    }

    private void write() {
        results.add("events", events);
        try {
            Files.createDirectories(out);
            Files.writeString(out.resolve("link-results.json"),
                new GsonBuilder().setPrettyPrinting().create().toJson(results), StandardCharsets.UTF_8);
        } catch (IOException e) {
            LOG.error("could not write results", e);
        }
    }

    private static double round(double v) {
        return Math.round(v * 1000.0) / 1000.0;
    }
}
